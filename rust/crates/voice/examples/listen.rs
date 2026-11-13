//! Join a channel as a listener, decode the voice that arrives and measure it, so sound can
//! be checked without a person: packet rate, gaps, loudness, and how strong a tone is.
//!
//! cargo run -p teamhub-voice --example listen -- <host> <port> <nickname> <seconds> [--tone HZ] [--wav FILE] [--channel NAME]

use std::time::{Duration, Instant};

use opus::{Channels, Decoder};
use teamhub_ts_proto::{ClientOptions, Event, Identity, TsClient};

const VOICE_HEADER: usize = 5;

fn option(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|arg| arg == name).and_then(|at| args.get(at + 1)).cloned()
}

/// Strength of one frequency relative to the whole signal, from 0 to 1.
fn tone_share(samples: &[i16], frequency: f64) -> f64 {
    let coefficient = 2.0 * (2.0 * std::f64::consts::PI * frequency / 48_000.0).cos();
    let (mut previous, mut before) = (0.0f64, 0.0f64);
    let mut energy = 0.0;
    for sample in samples {
        let value = f64::from(*sample);
        energy += value * value;
        let current = value + coefficient * previous - before;
        before = previous;
        previous = current;
    }
    let power = previous * previous + before * before - coefficient * previous * before;
    if energy == 0.0 {
        0.0
    } else {
        2.0 * power / (samples.len() as f64 * energy)
    }
}

fn rms(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / samples.len() as f64).sqrt()
}

fn write_wav(path: &str, stereo: &[i16]) -> std::io::Result<()> {
    let bytes = (stereo.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + bytes as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + bytes).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&48_000u32.to_le_bytes());
    out.extend_from_slice(&192_000u32.to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&bytes.to_le_bytes());
    for sample in stereo {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    std::fs::write(path, out)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [host, port, nickname, seconds, ..] = args.as_slice() else {
        return Err("usage: listen <host> <port> <nickname> <seconds> [--tone HZ] [--wav FILE] [--channel NAME]".into());
    };
    let tone: Option<f64> = option(&args, "--tone").map(|value| value.parse()).transpose()?;

    let identity = tokio::task::spawn_blocking(|| Identity::generate(8)).await??;
    let (client, mut events) = TsClient::connect(ClientOptions {
        host: host.clone(),
        port: port.parse()?,
        identity,
        nickname: nickname.clone(),
        server_password: None,
        default_channel: option(&args, "--channel"),
        channel_password: None,
    })
    .await?;
    println!("listening in channel {}", client.current_channel());

    let mut decoder = Decoder::new(48_000, Channels::Stereo)?;
    let mut samples: Vec<i16> = Vec::new();
    let mut frame = [0i16; 11_520];
    let (mut packets, mut stops, mut bad, mut gaps, mut largest_gap) = (0u64, 0u64, 0u64, 0u64, Duration::ZERO);
    let (mut first, mut last): (Option<Instant>, Option<Instant>) = (None, None);
    let mut nicknames: Vec<String> = Vec::new();

    let deadline = tokio::time::sleep(Duration::from_secs(seconds.parse()?));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => break,
            event = events.recv() => match event {
                Some(Event::Voice(data)) if data.len() == VOICE_HEADER => stops += 1,
                Some(Event::Voice(data)) if data.len() > VOICE_HEADER => {
                    let now = Instant::now();
                    if let Some(previous) = last {
                        let gap = now - previous;
                        largest_gap = largest_gap.max(gap);
                        if gap > Duration::from_millis(100) {
                            gaps += 1;
                        }
                    }
                    first.get_or_insert(now);
                    last = Some(now);
                    packets += 1;
                    match decoder.decode(&data[VOICE_HEADER..], &mut frame, false) {
                        Ok(count) => samples.extend_from_slice(&frame[..count * 2]),
                        Err(_) => bad += 1,
                    }
                }
                Some(Event::Command(command)) if command.name == "notifyclientupdated" => {
                    if let Some(name) = command.params.get("client_nickname") {
                        nicknames.push(name.clone());
                    }
                }
                Some(Event::Disconnected) | None => break,
                _ => {}
            },
        }
    }

    let span = match (first, last) {
        (Some(first), Some(last)) => (last - first).as_secs_f64(),
        _ => 0.0,
    };
    let rate = if span > 0.0 { packets as f64 / span } else { 0.0 };
    println!("packets={packets} stops={stops} undecodable={bad} span={span:.1}s rate={rate:.2}/s");
    println!("gaps_over_100ms={gaps} largest_gap={}ms", largest_gap.as_millis());
    println!("audio_seconds={:.1} rms={:.0}", samples.len() as f64 / 96_000.0, rms(&samples));
    if let Some(frequency) = tone {
        let left: Vec<i16> = samples.iter().step_by(2).copied().collect();
        println!("tone_share@{frequency}Hz={:.3}", tone_share(&left, frequency));
        println!("tone_share@{}Hz={:.3}", frequency * 1.5, tone_share(&left, frequency * 1.5));
    }
    println!("nicknames={nicknames:?}");
    if let Some(path) = option(&args, "--wav") {
        write_wav(&path, &samples)?;
        println!("wrote wav");
    }
    client.disconnect();
    let _ = tokio::time::timeout(Duration::from_secs(1), events.recv()).await;
    Ok(())
}
