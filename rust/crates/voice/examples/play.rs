//! Play one source on a real TeamSpeak server.
//!
//! cargo run -p teamhub-voice --example play -- <host> <port> <nickname> <file|url> <seconds> [options]
//!
//! Options: --live (radio), --youtube, --volume N, --channel NAME, --title TEXT, --level N,
//! --music-dir DIR, --cookies FILE,
//! --at "SECOND:ACTION,..." with pause, resume, skip, stop, seek=N, volume=N.

use std::sync::Arc;
use std::time::{Duration, Instant};

use teamhub_media::cache::MediaCache;
use teamhub_media::youtube::YtDlpOptions;
use teamhub_ts_proto::Identity;
use teamhub_voice::{BotConfig, Media, QueueItem, Source, VoiceBot};

fn option(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|arg| arg == name).and_then(|at| args.get(at + 1)).cloned()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [host, port, nickname, source, seconds, ..] = args.as_slice() else {
        return Err("usage: play <host> <port> <nickname> <file|url> <seconds> [options]".into());
    };
    let level: u32 = option(&args, "--level").map_or(Ok(8), |value| value.parse())?;
    let volume: u8 = option(&args, "--volume").map_or(Ok(100), |value| value.parse())?;
    let live = args.iter().any(|arg| arg == "--live");
    let youtube = args.iter().any(|arg| arg == "--youtube");
    let music_dir = std::path::PathBuf::from(option(&args, "--music-dir").unwrap_or_else(|| "music".to_owned()));
    std::fs::create_dir_all(&music_dir)?;

    let identity = tokio::task::spawn_blocking(move || Identity::generate(level)).await??;
    let bot = VoiceBot::start(BotConfig {
        host: host.clone(),
        port: port.parse()?,
        identity,
        nickname: nickname.clone(),
        server_password: None,
        default_channel: option(&args, "--channel"),
        channel_password: None,
        volume,
        media: Media {
            cache: MediaCache::new(&music_dir),
            yt_dlp: YtDlpOptions { cookie_file: option(&args, "--cookies").map(Into::into) },
        },
        queue: Arc::default(),
    })
    .await?;
    println!("connected");

    bot.play(QueueItem {
        id: "test".to_owned(),
        title: option(&args, "--title").unwrap_or_else(|| "Test".to_owned()),
        artist: None,
        duration: None,
        source: if live { Source::Radio } else if youtube { Source::Youtube } else { Source::Local },
        file_path: if live || youtube { String::new() } else { source.clone() },
        source_url: youtube.then(|| source.clone()),
        stream_url: live.then(|| source.clone()),
    });

    let mut script: Vec<(u64, String)> = option(&args, "--at")
        .unwrap_or_default()
        .split(',')
        .filter_map(|step| step.split_once(':'))
        .map(|(at, action)| Ok((at.parse()?, action.to_owned())))
        .collect::<Result<_, std::num::ParseIntError>>()?;
    script.sort();

    let started = Instant::now();
    let total = Duration::from_secs(seconds.parse()?);
    let mut next_report = 1;
    while started.elapsed() < total {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let elapsed = started.elapsed().as_secs();
        while script.first().is_some_and(|(at, _)| *at <= elapsed) {
            let (_, action) = script.remove(0);
            println!("{elapsed:>3}s action {action}");
            match action.split_once('=') {
                Some(("seek", value)) => bot.seek(value.parse()?),
                Some(("volume", value)) => bot.set_volume(value.parse()?),
                _ => match action.as_str() {
                    "pause" => bot.pause(),
                    "resume" => bot.resume(),
                    "skip" => bot.skip(),
                    "stop" => bot.stop_audio(),
                    other => return Err(format!("unknown action {other}").into()),
                },
            }
        }
        if elapsed >= next_report {
            next_report = elapsed + 5;
            let state = bot.snapshot();
            println!(
                "{elapsed:>3}s {} frames={} position={:.1} title={:?} error={:?}",
                state.status.as_str(),
                state.frames_sent,
                state.position_seconds,
                state.now_playing.as_ref().map(|item| item.title.as_str()),
                state.last_error,
            );
        }
    }
    let state = bot.snapshot();
    println!("end {} frames={} position={:.2} error={:?}", state.status.as_str(), state.frames_sent, state.position_seconds, state.last_error);
    bot.disconnect().await;
    println!("left");
    Ok(())
}
