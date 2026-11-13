//! Connect to a real TeamSpeak server, list what the client sees, then leave.
//!
//! cargo run -p teamhub-ts-proto --example connect -- <host> <port> <nickname> <seconds> [security level]

use std::time::Duration;

use teamhub_ts_proto::{ClientOptions, Event, Identity, TsClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [host, port, nickname, seconds, ..] = args.as_slice() else {
        return Err("usage: connect <host> <port> <nickname> <seconds> [security level]".into());
    };
    let level: u32 = args.get(4).map_or(Ok(8), |v| v.parse())?;

    let identity = tokio::task::spawn_blocking(move || Identity::generate(level)).await??;
    println!("identity uid={} level={}", identity.uid, identity.security_level());

    let started = std::time::Instant::now();
    let (client, mut events) = TsClient::connect(ClientOptions {
        host: host.clone(),
        port: port.parse()?,
        identity,
        nickname: nickname.clone(),
        server_password: None,
        default_channel: None,
        channel_password: None,
    })
    .await?;
    println!("CONNECTED in {:?} clid={}", started.elapsed(), client.client_id());

    let deadline = tokio::time::sleep(Duration::from_secs(seconds.parse()?));
    tokio::pin!(deadline);
    let mut commands = 0;
    loop {
        tokio::select! {
            _ = &mut deadline => break,
            event = events.recv() => match event {
                Some(Event::Command(_)) => commands += 1,
                Some(Event::TextMessage(params)) => println!("text: {params:?}"),
                Some(Event::Error(message)) => println!("error: {message}"),
                Some(Event::Disconnected) | None => { println!("disconnected early"); return Ok(()); }
                _ => {}
            },
        }
    }

    println!("commands received: {commands}");
    println!("home channel: {}", client.current_channel());
    println!("others in channel: {:?}", client.channel_members());
    for channel in client.channels() {
        println!("channel {} parent={} {:?}", channel.cid, channel.parent_id, channel.name);
    }

    client.disconnect();
    while let Some(event) = events.recv().await {
        if event == Event::Disconnected {
            println!("left cleanly");
            break;
        }
    }
    Ok(())
}
