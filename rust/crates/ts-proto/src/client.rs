//! Runs a [`Session`] over a real UDP socket on its own task.

use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use tokio::net::{lookup_host, UdpSocket};
use tokio::sync::mpsc;

use crate::commands::build_send_text_commands;
use crate::session::{ClientOptions, Event, Session};
use crate::view::{ChannelInfo, View};
use crate::{err, Result};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

enum Request {
    Command(String),
    Voice(Vec<u8>),
    VoiceStop,
    Move(u64),
    Disconnect,
}

/// A connected TeamSpeak client. Dropping it closes the connection.
pub struct TsClient {
    requests: mpsc::UnboundedSender<Request>,
    view: Arc<RwLock<View>>,
}

impl TsClient {
    /// Connect and wait for the server to accept. Later events arrive on the returned receiver.
    pub async fn connect(opts: ClientOptions) -> Result<(Self, mpsc::UnboundedReceiver<Event>)> {
        // Resolve once: hostname lookups must not run for every voice packet.
        let target = lookup_host((opts.host.trim(), opts.port))
            .await
            .or_else(|e| err(format!("Could not resolve {}: {e}", opts.host)))?
            .find(std::net::SocketAddr::is_ipv4)
            .ok_or(crate::Error(format!("No IPv4 address for {}", opts.host)))?;
        let socket = UdpSocket::bind(("0.0.0.0", 0)).await.or_else(|e| err(format!("UDP bind failed: {e}")))?;
        socket.connect(target).await.or_else(|e| err(format!("UDP connect failed: {e}")))?;

        let view = Arc::new(RwLock::new(View::default()));
        let session = Session::new(opts, view.clone(), Instant::now())?;
        let (request_tx, request_rx) = mpsc::unbounded_channel();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        tokio::spawn(run(session, socket, request_rx, event_tx));

        let client = Self { requests: request_tx, view };
        let wait = async {
            while let Some(event) = event_rx.recv().await {
                match event {
                    Event::Connected => return Ok(()),
                    Event::Error(message) => return err(message),
                    Event::Disconnected => return err("Connection closed"),
                    _ => {}
                }
            }
            err("Connection closed")
        };
        match tokio::time::timeout(CONNECT_TIMEOUT, wait).await {
            Ok(Ok(())) => Ok((client, event_rx)),
            Ok(Err(error)) => Err(error),
            Err(_) => err("Connection timeout"),
        }
    }

    pub fn send_command(&self, command: impl Into<String>) {
        let _ = self.requests.send(Request::Command(command.into()));
    }

    /// Chat text, split so each piece fits one voice packet.
    pub fn send_text(&self, message: &str, target_mode: u8, target: Option<u64>) {
        for command in build_send_text_commands(message, target_mode, target) {
            self.send_command(command);
        }
    }

    pub fn send_voice(&self, opus: Vec<u8>) {
        let _ = self.requests.send(Request::Voice(opus));
    }

    pub fn send_voice_stop(&self) {
        let _ = self.requests.send(Request::VoiceStop);
    }

    pub fn move_to_channel(&self, channel: u64) {
        let _ = self.requests.send(Request::Move(channel));
    }

    /// Ask the server to end the connection; `Event::Disconnected` follows.
    pub fn disconnect(&self) {
        let _ = self.requests.send(Request::Disconnect);
    }

    pub fn client_id(&self) -> u64 {
        self.view.read().expect("view lock").client_id
    }

    pub fn current_channel(&self) -> u64 {
        self.view.read().expect("view lock").current_channel
    }

    /// Other non-query clients in this client's channel.
    pub fn channel_members(&self) -> Vec<u64> {
        self.view.read().expect("view lock").channel_members.iter().copied().collect()
    }

    /// Channel of any client in view.
    pub fn client_channel(&self, clid: u64) -> Option<u64> {
        self.view.read().expect("view lock").client_channels.get(&clid).copied()
    }

    pub fn channels(&self) -> Vec<ChannelInfo> {
        self.view.read().expect("view lock").list_channels()
    }
}

async fn run(
    mut session: Session,
    socket: UdpSocket,
    mut requests: mpsc::UnboundedReceiver<Request>,
    events: mpsc::UnboundedSender<Event>,
) {
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    let mut buffer = [0u8; 2048];
    let mut requests_open = true;
    loop {
        tokio::select! {
            received = socket.recv(&mut buffer) => match received {
                Ok(len) => session.handle_datagram(&buffer[..len], Instant::now()),
                // Windows reports an unreachable port as a read error; the timers decide.
                Err(error) => tracing::debug!(%error, "UDP receive failed"),
            },
            request = requests.recv(), if requests_open => {
                let now = Instant::now();
                match request {
                    Some(Request::Command(command)) => session.send_command(&command, now),
                    Some(Request::Voice(opus)) => session.send_voice(&opus, now),
                    Some(Request::VoiceStop) => session.send_voice_stop(now),
                    Some(Request::Move(channel)) => session.move_to_channel(channel, now),
                    Some(Request::Disconnect) => session.disconnect(now),
                    None => {
                        requests_open = false;
                        session.disconnect(now);
                    }
                }
            },
            _ = tick.tick() => session.tick(Instant::now()),
        }

        while let Some(datagram) = session.next_datagram() {
            if let Err(error) = socket.send(&datagram).await {
                tracing::debug!(%error, "UDP send failed");
            }
        }
        let mut closed = false;
        while let Some(event) = session.next_event() {
            closed |= event == Event::Disconnected;
            let _ = events.send(event);
        }
        if closed {
            return;
        }
    }
}
