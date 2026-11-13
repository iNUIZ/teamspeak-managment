//! The protocol state machine, free of sockets and timers: datagrams go in and out as
//! bytes and time is passed in, so every path can be tested. Ported from `tslib/client.ts`.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use num_bigint::BigUint;

use crate::commands::{build_command, parse_command, Params, ParsedCommand, MAX_TS_COMMAND_BYTES};
use crate::crypto::{
    derive_key_nonce, eax_decrypt, eax_encrypt, hash_password, sha1, DUMMY_KEY, DUMMY_NONCE, INIT_MAC, INIT_VERSION,
    MAC_LEN,
};
use crate::identity::Identity;
use crate::license::{derive_license_key, generate_temporary_key, parse_license, shared_secret2};
use crate::quicklz;
use crate::view::{number, parse_enter_view, EnterViewEntry, View};
use crate::{err, Result};

pub const VOICE: u8 = 0;
pub const VOICE_WHISPER: u8 = 1;
pub const COMMAND: u8 = 2;
pub const COMMAND_LOW: u8 = 3;
pub const PING: u8 = 4;
pub const PONG: u8 = 5;
pub const ACK: u8 = 6;
pub const ACK_LOW: u8 = 7;
pub const INIT1: u8 = 8;

const FLAG_FRAGMENTED: u8 = 0x10;
const FLAG_NEWPROTOCOL: u8 = 0x20;
const FLAG_COMPRESSED: u8 = 0x40;
const FLAG_UNENCRYPTED: u8 = 0x80;

const C2S_HEADER_LEN: usize = 5; // id(2) + client id(2) + type(1)
const S2C_HEADER_LEN: usize = 3; // id(2) + type(1)
const OPUS_MUSIC: u8 = 5;

const RESEND_AFTER: Duration = Duration::from_secs(1);
const GIVE_UP_AFTER: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const PING_EVERY: Duration = Duration::from_secs(1);
const DISCONNECT_GRACE: Duration = Duration::from_millis(500);

// TS3AudioBot's default version sign (Linux, far-future build).
const VERSION_PLATFORM: &str = "Linux";
const VERSION_NAME: &str = "3.?.? [Build: 5680278000]";
const VERSION_SIGN: &str = "Hjd+N58Gv3ENhoKmGYy2bNRBsNNgm5kpiaQWxOj5HN2DXttG6REjymSwJtpJ8muC2gSwRuZi0R+8Laan5ts5CQ==";

/// Errors that mean the server refused the connection itself, so retrying cannot help:
/// 1027 server full, 1028 wrong password, 3329 banned; 1796 for older servers.
/// 2568 (insufficient permissions) belongs to a single command and is deliberately not here.
const CONNECTION_REFUSED_ERRORS: [u64; 4] = [1027, 1028, 1796, 3329];

/// Besides the refusals above, any error answering the handshake ends the attempt: the
/// server then never sends `initserver` (1541 bad nickname, 519 low security level).
pub fn is_connection_refusal(error_id: u64, handshaking: bool) -> bool {
    CONNECTION_REFUSED_ERRORS.contains(&error_id) || (handshaking && error_id != 0)
}

#[derive(Debug, Clone)]
pub struct ClientOptions {
    pub host: String,
    pub port: u16,
    pub identity: Identity,
    pub nickname: String,
    pub server_password: Option<String>,
    pub default_channel: Option<String>,
    pub channel_password: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Connected,
    Disconnected,
    Command(ParsedCommand),
    TextMessage(Params),
    ServerError(Params),
    Voice(Vec<u8>),
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Disconnected,
    Init,
    Handshake,
    Connected,
    Disconnecting,
}

struct Resend {
    raw: Vec<u8>,
    first_send: Instant,
    last_send: Instant,
}

pub struct Session {
    opts: ClientOptions,
    state: State,
    outgoing: VecDeque<Vec<u8>>,
    events: VecDeque<Event>,
    view: Arc<RwLock<View>>,

    packet_counter: [u16; 9],
    generation: [u32; 9],
    in_generation: [u32; 9],
    in_last_id: [u16; 9],

    resend: HashMap<u16, Resend>,
    init_resend: Option<Resend>,
    last_message: Instant,
    connect_deadline: Option<Instant>,
    disconnect_at: Option<Instant>,
    last_ping: Instant,

    crypto_ready: bool,
    iv: Option<Vec<u8>>,
    fake_signature: [u8; MAC_LEN],
    alpha: Option<[u8; 10]>,

    fragments: Vec<u8>,
    fragmenting: bool,
    fragment_flags: u8,
    pending_enter_views: Vec<EnterViewEntry>,
}

impl Session {
    pub fn new(opts: ClientOptions, view: Arc<RwLock<View>>, now: Instant) -> Result<Self> {
        *view.write().expect("view lock") = View::default();
        let mut session = Self {
            opts,
            state: State::Init,
            outgoing: VecDeque::new(),
            events: VecDeque::new(),
            view,
            packet_counter: [0; 9],
            generation: [0; 9],
            in_generation: [0; 9],
            in_last_id: [0; 9],
            resend: HashMap::new(),
            init_resend: None,
            last_message: now,
            connect_deadline: Some(now + CONNECT_TIMEOUT),
            disconnect_at: None,
            last_ping: now,
            crypto_ready: false,
            iv: None,
            fake_signature: [0; MAC_LEN],
            alpha: None,
            fragments: Vec::new(),
            fragmenting: false,
            fragment_flags: 0,
            pending_enter_views: Vec::new(),
        };
        // Command id 0 belongs to clientinitiv, which travels inside the init packets.
        session.inc_counter(COMMAND);
        let init0 = session.build_init0(now)?;
        session.send_init(init0, now);
        Ok(session)
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn is_handshaking(&self) -> bool {
        matches!(self.state, State::Init | State::Handshake)
    }

    pub fn next_datagram(&mut self) -> Option<Vec<u8>> {
        self.outgoing.pop_front()
    }

    pub fn next_event(&mut self) -> Option<Event> {
        self.events.pop_front()
    }

    fn view(&self) -> std::sync::RwLockWriteGuard<'_, View> {
        self.view.write().expect("view lock")
    }

    fn fail(&mut self, message: impl Into<String>) {
        self.events.push_back(Event::Error(message.into()));
    }

    // ====== Sending ======

    pub fn send_command(&mut self, cmd: &str, now: Instant) {
        let data = cmd.as_bytes();
        if data.len() <= MAX_TS_COMMAND_BYTES {
            self.send_outgoing(data.to_vec(), COMMAND, 0, now);
            return;
        }
        let chunks: Vec<&[u8]> = data.chunks(MAX_TS_COMMAND_BYTES).collect();
        let last = chunks.len() - 1;
        for (i, chunk) in chunks.into_iter().enumerate() {
            // The flag marks the first and the last fragment.
            let flags = if i == 0 || i == last { FLAG_FRAGMENTED } else { 0 };
            self.send_outgoing(chunk.to_vec(), COMMAND, flags, now);
        }
    }

    /// Already Opus-encoded audio.
    pub fn send_voice(&mut self, opus: &[u8], now: Instant) {
        if self.state != State::Connected {
            return;
        }
        let mut data = vec![0, 0, OPUS_MUSIC];
        data.extend_from_slice(opus);
        self.send_outgoing(data, VOICE, 0, now);
    }

    pub fn send_voice_stop(&mut self, now: Instant) {
        self.send_voice(&[], now);
    }

    /// Move this client into a channel (no-op when already there).
    pub fn move_to_channel(&mut self, channel: u64, now: Instant) {
        {
            let mut view = self.view();
            if view.client_id == 0 || channel == 0 || view.current_channel == channel {
                return;
            }
            view.current_channel = channel;
            view.clear_members();
        }
        let clid = self.view().client_id.to_string();
        let cid = channel.to_string();
        let cpw = self.opts.channel_password.clone().unwrap_or_default();
        self.send_command(&build_command("clientmove", &[("cid", &cid), ("clid", &clid), ("cpw", &cpw)]), now);
        // Seed the occupants: enter-view events alone race with the moved notification.
        self.send_command("clientlist", now);
    }

    pub fn disconnect(&mut self, now: Instant) {
        if self.state == State::Connected {
            self.state = State::Disconnecting;
            self.send_command(&build_command("clientdisconnect", &[("reasonid", "8"), ("reasonmsg", "leaving")]), now);
        }
        self.disconnect_at.get_or_insert(now + DISCONNECT_GRACE);
    }

    fn cleanup(&mut self) {
        if self.state == State::Disconnected {
            return;
        }
        self.state = State::Disconnected;
        {
            // Drop the assigned id so a recycled id is never treated as this bot.
            let mut view = self.view();
            view.client_id = 0;
            view.current_channel = 0;
            view.clear_members();
        }
        self.pending_enter_views.clear();
        self.events.push_back(Event::Disconnected);
    }

    fn build_init0(&self, now: Instant) -> Result<Vec<u8>> {
        let _ = now;
        let mut buf = vec![0u8; 4 + 1 + 4 + 4 + 8];
        buf[..4].copy_from_slice(&INIT_VERSION.to_be_bytes());
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as u32);
        buf[5..9].copy_from_slice(&seconds.to_be_bytes());
        getrandom::fill(&mut buf[9..13]).or_else(|e| err(format!("No random data: {e}")))?;
        Ok(buf)
    }

    fn send_init(&mut self, data: Vec<u8>, now: Instant) {
        let mut raw = self.build_raw(&data, INIT1, 101, FLAG_UNENCRYPTED);
        raw[..MAC_LEN].copy_from_slice(INIT_MAC);
        self.init_resend = Some(Resend { raw: raw.clone(), first_send: now, last_send: now });
        self.outgoing.push_back(raw);
    }

    fn build_raw(&self, data: &[u8], packet_type: u8, id: u16, flags: u8) -> Vec<u8> {
        let client_id = self.view.read().expect("view lock").client_id as u16;
        let mut raw = vec![0u8; MAC_LEN + C2S_HEADER_LEN + data.len()];
        raw[MAC_LEN..MAC_LEN + 2].copy_from_slice(&id.to_be_bytes());
        raw[MAC_LEN + 2..MAC_LEN + 4].copy_from_slice(&client_id.to_be_bytes());
        raw[MAC_LEN + 4] = (flags & 0xf0) | (packet_type & 0x0f);
        raw[MAC_LEN + C2S_HEADER_LEN..].copy_from_slice(data);
        raw
    }

    fn inc_counter(&mut self, packet_type: u8) {
        let index = usize::from(packet_type);
        self.packet_counter[index] = self.packet_counter[index].wrapping_add(1);
        if self.packet_counter[index] == 0 {
            self.generation[index] = self.generation[index].wrapping_add(1);
        }
    }

    fn keys(&self, from_server: bool, id: u16, generation: u32, packet_type: u8, force_dummy: bool) -> ([u8; 16], [u8; 16]) {
        match &self.iv {
            Some(iv) if self.crypto_ready && !force_dummy => derive_key_nonce(from_server, id, generation, packet_type, iv),
            _ => (*DUMMY_KEY, *DUMMY_NONCE),
        }
    }

    fn send_outgoing(&mut self, mut data: Vec<u8>, packet_type: u8, extra_flags: u8, now: Instant) {
        let index = usize::from(packet_type);
        let (id, generation) = (self.packet_counter[index], self.generation[index]);
        self.inc_counter(packet_type);

        let mut flags = extra_flags;
        match packet_type {
            VOICE | VOICE_WHISPER => data[..2].copy_from_slice(&id.to_be_bytes()),
            COMMAND | COMMAND_LOW => flags |= FLAG_NEWPROTOCOL,
            PING | PONG => flags |= FLAG_UNENCRYPTED,
            _ => {}
        }

        let mut raw = self.build_raw(&data, packet_type, id, flags);
        if flags & FLAG_UNENCRYPTED != 0 {
            raw[..MAC_LEN].copy_from_slice(&self.fake_signature);
        } else {
            let (key, nonce) = self.keys(false, id, generation, packet_type, false);
            let (ciphertext, mac) = eax_encrypt(&key, &nonce, &raw[MAC_LEN..MAC_LEN + C2S_HEADER_LEN], &data);
            raw[..MAC_LEN].copy_from_slice(&mac);
            raw[MAC_LEN + C2S_HEADER_LEN..].copy_from_slice(&ciphertext);
        }

        if matches!(packet_type, COMMAND | COMMAND_LOW) {
            self.resend.insert(id, Resend { raw: raw.clone(), first_send: now, last_send: now });
        }
        self.outgoing.push_back(raw);
    }

    // ====== Receiving ======

    pub fn handle_datagram(&mut self, raw: &[u8], now: Instant) {
        if raw.len() < MAC_LEN + S2C_HEADER_LEN || self.state == State::Disconnected {
            return;
        }
        let id = u16::from_be_bytes([raw[MAC_LEN], raw[MAC_LEN + 1]]);
        let type_byte = raw[MAC_LEN + 2];
        let packet_type = type_byte & 0x0f;
        let flags = type_byte & 0xf0;
        if packet_type > INIT1 {
            return;
        }
        self.last_message = now;

        let Some(data) = self.decrypt(raw, packet_type, id, flags) else {
            tracing::debug!(packet_type, id, "failed to decrypt packet");
            return;
        };

        match packet_type {
            INIT1 => self.handle_init(&data, now),
            COMMAND | COMMAND_LOW => {
                let ack = if packet_type == COMMAND { ACK } else { ACK_LOW };
                self.send_outgoing(id.to_be_bytes().to_vec(), ack, 0, now);
                self.handle_command_data(data, flags, now);
            }
            ACK => {
                if data.len() >= 2 {
                    self.resend.remove(&u16::from_be_bytes([data[0], data[1]]));
                }
            }
            PING => self.send_outgoing(id.to_be_bytes().to_vec(), PONG, 0, now),
            VOICE | VOICE_WHISPER => self.events.push_back(Event::Voice(data)),
            _ => {}
        }
    }

    fn try_decrypt(&self, raw: &[u8], packet_type: u8, id: u16, generation: u32, force_dummy: bool) -> Option<Vec<u8>> {
        let (key, nonce) = self.keys(true, id, generation, packet_type, force_dummy);
        let header = &raw[MAC_LEN..MAC_LEN + S2C_HEADER_LEN];
        eax_decrypt(&key, &nonce, header, &raw[MAC_LEN + S2C_HEADER_LEN..], &raw[..MAC_LEN])
    }

    fn decrypt(&mut self, raw: &[u8], packet_type: u8, id: u16, flags: u8) -> Option<Vec<u8>> {
        let body = &raw[MAC_LEN + S2C_HEADER_LEN..];
        if packet_type == INIT1 {
            return (&raw[..MAC_LEN] == INIT_MAC).then(|| body.to_vec());
        }
        if flags & FLAG_UNENCRYPTED != 0 {
            let valid = !self.crypto_ready || raw[..MAC_LEN] == self.fake_signature;
            return valid.then(|| body.to_vec());
        }
        let index = usize::from(packet_type);
        let generation = self.in_generation[index];
        // The id wrapped around: the server moved on to the next generation of keys.
        let wrapped = id < 0x4000 && self.in_last_id[index] > 0xc000;
        if wrapped {
            if let Some(data) = self.try_decrypt(raw, packet_type, id, generation.wrapping_add(1), false) {
                self.in_generation[index] = generation.wrapping_add(1);
                self.in_last_id[index] = id;
                return Some(data);
            }
        }
        let data = self.try_decrypt(raw, packet_type, id, generation, false).or_else(|| {
            // Packets from before or during the key change still use the dummy keys.
            self.try_decrypt(raw, packet_type, id, generation, true)
        })?;
        if !wrapped {
            self.in_last_id[index] = self.in_last_id[index].max(id);
        }
        Some(data)
    }

    fn handle_init(&mut self, data: &[u8], now: Instant) {
        match data.first() {
            Some(1) => {
                if data.len() < 21 {
                    return;
                }
                let mut init2 = vec![0u8; 25];
                init2[..4].copy_from_slice(&INIT_VERSION.to_be_bytes());
                init2[4] = 2;
                init2[5..25].copy_from_slice(&data[1..21]);
                self.send_init(init2, now);
            }
            Some(3) => {
                if let Err(error) = self.answer_puzzle(data, now) {
                    self.fail(error.0);
                }
            }
            Some(0x7f) => match self.build_init0(now) {
                Ok(init0) => self.send_init(init0, now),
                Err(error) => self.fail(error.0),
            },
            _ => {}
        }
    }

    fn answer_puzzle(&mut self, data: &[u8], now: Instant) -> Result<()> {
        const TEXT_START: usize = 301;
        if data.len() < 1 + 64 + 64 + 4 + 100 {
            return Ok(());
        }
        self.state = State::Handshake;
        self.init_resend = None;

        let level = i32::from_be_bytes(data[129..133].try_into().expect("4 bytes"));
        let (x, n) = (BigUint::from_bytes_be(&data[1..65]), BigUint::from_bytes_be(&data[65..129]));
        if !(0..=1_000_000).contains(&level) || n.bits() == 0 {
            return err(format!("Invalid server puzzle (level {level})"));
        }
        // y = x^(2^level) mod n
        let mut y = x % &n;
        for _ in 0..level {
            y = &y * &y % &n;
        }
        let y = y.to_bytes_be();

        let mut alpha = [0u8; 10];
        getrandom::fill(&mut alpha).or_else(|e| err(format!("No random data: {e}")))?;
        self.alpha = Some(alpha);
        let alpha_b64 = B64.encode(alpha);
        let omega = self.opts.identity.public_key_string.clone();
        let text = build_command("clientinitiv", &[("alpha", &alpha_b64), ("omega", &omega), ("ot", "1"), ("ip", "")]);

        let mut init4 = vec![0u8; TEXT_START + text.len()];
        init4[..4].copy_from_slice(&INIT_VERSION.to_be_bytes());
        init4[4] = 4;
        init4[5..69].copy_from_slice(&data[1..65]);
        init4[69..133].copy_from_slice(&data[65..129]);
        init4[133..137].copy_from_slice(&level.to_be_bytes());
        init4[137..237].copy_from_slice(&data[133..233]);
        init4[237 + 64 - y.len()..TEXT_START].copy_from_slice(&y);
        init4[TEXT_START..].copy_from_slice(text.as_bytes());
        self.send_init(init4, now);
        Ok(())
    }

    fn handle_command_data(&mut self, data: Vec<u8>, flags: u8, now: Instant) {
        if flags & FLAG_FRAGMENTED != 0 {
            if self.fragmenting {
                self.fragments.extend(data);
                let merged = std::mem::take(&mut self.fragments);
                let saved = std::mem::take(&mut self.fragment_flags);
                self.fragmenting = false;
                self.process_command(merged, saved, now);
            } else {
                self.fragmenting = true;
                self.fragment_flags = flags;
                self.fragments = data;
            }
            return;
        }
        if self.fragmenting {
            self.fragments.extend(data);
            return;
        }
        self.process_command(data, flags, now);
    }

    fn process_command(&mut self, data: Vec<u8>, flags: u8, now: Instant) {
        let data = if flags & FLAG_COMPRESSED != 0 {
            match quicklz::decompress(&data) {
                Ok(data) => data,
                Err(error) => {
                    tracing::debug!(%error, "decompression failed");
                    return;
                }
            }
        } else {
            data
        };
        let text = String::from_utf8_lossy(&data);
        let parsed = parse_command(&text);
        self.events.push_back(Event::Command(parsed.clone()));

        match parsed.name.as_str() {
            "initivexpand" => {
                if let Err(error) = self.handle_iv_expand(&parsed.params, now) {
                    self.fail(error.0);
                }
            }
            "initivexpand2" => {
                if !self.crypto_ready && self.alpha.is_some() {
                    if let Err(error) = self.handle_iv_expand2(&parsed.params, now) {
                        self.fail(error.0);
                    }
                }
            }
            "initserver" => self.handle_init_server(&parsed.params, now),
            "channellist" | "notifychannelcreated" => self.view().apply_channel_list(&parsed),
            "channellistfinished" => self.handle_channel_list_finished(now),
            "notifycliententerview" => {
                let entries = parse_enter_view(&parsed);
                if self.view().client_id == 0 {
                    self.pending_enter_views.extend(entries);
                } else if self.view().apply_enter_view(&entries) {
                    self.send_command("clientlist", now);
                }
            }
            "notifyclientleftview" => {
                if self.view().apply_left_view(&parsed.params) {
                    self.cleanup();
                }
            }
            "notifyclientmoved" => {
                if self.view().apply_moved(&parsed.params) {
                    self.send_command("clientlist", now);
                }
            }
            "notifychanneledited" => self.view().apply_channel_edited(&parsed.params),
            "notifychanneldeleted" => self.view().apply_channel_deleted(&parsed.params),
            "notifytextmessage" => self.events.push_back(Event::TextMessage(parsed.params.clone())),
            "clientlist" => {
                if self.view().apply_client_list(&parsed) {
                    self.send_command("clientlist", now);
                }
            }
            "error" => {
                self.events.push_back(Event::ServerError(parsed.params.clone()));
                let id = number(parsed.params.get("id"));
                if is_connection_refusal(id, self.is_handshaking()) {
                    let message = parsed.params.get("msg").map_or("unknown error", String::as_str);
                    self.fail(format!("TS3 error {id}: {message}"));
                    self.disconnect(now);
                }
            }
            _ => {}
        }
    }

    fn finish_crypto(&mut self, iv: Vec<u8>, now: Instant) {
        self.fake_signature.copy_from_slice(&sha1(&iv)[..MAC_LEN]);
        self.iv = Some(iv);
        self.crypto_ready = true;
        self.alpha = None;
        self.send_client_init(now);
    }

    fn handle_iv_expand(&mut self, params: &Params, now: Instant) -> Result<()> {
        self.init_resend = None;
        let (Some(beta), Some(omega), Some(alpha)) = (params.get("beta"), params.get("omega"), self.alpha) else {
            return err("Missing initivexpand parameters");
        };
        let beta = B64.decode(beta).or_else(|_| err("Invalid initivexpand beta"))?;
        let omega = B64.decode(omega).or_else(|_| err("Invalid initivexpand omega"))?;
        let shared = self.opts.identity.shared_secret(&omega)?;

        let mut iv = vec![0u8; 10 + beta.len()];
        for i in 0..10 {
            iv[i] = shared[i] ^ alpha[i];
        }
        for (i, byte) in beta.iter().enumerate() {
            iv[10 + i] = shared.get(10 + i).copied().unwrap_or(0) ^ byte;
        }
        self.finish_crypto(iv, now);
        Ok(())
    }

    fn handle_iv_expand2(&mut self, params: &Params, now: Instant) -> Result<()> {
        self.init_resend = None;
        let (Some(license), Some(beta), Some(_), Some(alpha)) =
            (params.get("l"), params.get("beta"), params.get("omega"), self.alpha)
        else {
            return err("Missing initivexpand2 parameters");
        };
        let beta = B64.decode(beta).or_else(|_| err("Invalid initivexpand2 beta"))?;
        let license = B64.decode(license).or_else(|_| err("Invalid initivexpand2 license"))?;
        let server_key = derive_license_key(&parse_license(&license)?)?;
        let (temp_public, temp_private) = generate_temporary_key()?;

        let mut to_sign = temp_public.to_vec();
        to_sign.extend_from_slice(&beta);
        let proof = B64.encode(self.opts.identity.sign(&to_sign));
        let ek = B64.encode(temp_public);
        // Goes out under the dummy keys: the real ones only apply after this.
        self.send_command(&build_command("clientek", &[("ek", &ek), ("proof", &proof)]), now);

        let mut iv = shared_secret2(&server_key, &temp_private)?.to_vec();
        for (byte, a) in iv.iter_mut().zip(alpha) {
            *byte ^= a;
        }
        for (byte, b) in iv[10..].iter_mut().zip(&beta) {
            *byte ^= b;
        }
        self.finish_crypto(iv, now);
        Ok(())
    }

    fn send_client_init(&mut self, now: Instant) {
        let opts = &self.opts;
        let channel_password = opts.channel_password.as_deref().map(hash_password).unwrap_or_default();
        let server_password = opts.server_password.as_deref().map(hash_password).unwrap_or_default();
        let key_offset = opts.identity.key_offset.to_string();
        let cmd = build_command(
            "clientinit",
            &[
                ("client_nickname", &opts.nickname),
                ("client_version", VERSION_NAME),
                ("client_platform", VERSION_PLATFORM),
                ("client_input_hardware", "1"),
                ("client_output_hardware", "1"),
                ("client_default_channel", opts.default_channel.as_deref().unwrap_or("")),
                ("client_default_channel_password", &channel_password),
                ("client_server_password", &server_password),
                ("client_meta_data", ""),
                ("client_version_sign", VERSION_SIGN),
                ("client_key_offset", &key_offset),
                ("client_nickname_phonetic", ""),
                ("client_default_token", ""),
                ("hwid", ""),
            ],
        );
        self.send_command(&cmd, now);
    }

    fn handle_init_server(&mut self, params: &Params, now: Instant) {
        let client_id = number(params.get("aclid"));
        self.view().client_id = client_id;
        self.state = State::Connected;
        self.connect_deadline = None;
        self.last_ping = now;

        // Enter-view that raced ahead of the id, then find home if it is still unknown.
        let mut learned = false;
        if client_id != 0 && !self.pending_enter_views.is_empty() {
            let pending = std::mem::take(&mut self.pending_enter_views);
            learned = self.view().apply_enter_view(&pending);
        }
        let unknown = self.view().current_channel == 0;
        if client_id != 0 && (unknown || learned) {
            self.send_command("clientlist", now);
        }
        self.events.push_back(Event::Connected);
    }

    fn handle_channel_list_finished(&mut self, now: Instant) {
        let (client_id, unknown) = {
            let view = self.view();
            (view.client_id, view.current_channel == 0)
        };
        if client_id == 0 {
            return;
        }
        let target = self.opts.default_channel.clone().filter(|t| !t.is_empty());
        let cid = target.as_deref().and_then(|target| {
            // A plain number is a channel id; anything else is a channel name.
            match target.parse::<u64>() {
                Ok(id) if id.to_string() == target => Some(id),
                _ => self.view().channel_id_by_name(target),
            }
        });
        match cid {
            Some(cid) => self.move_to_channel(cid, now),
            None if unknown => self.send_command("clientlist", now),
            None => {}
        }
    }

    // ====== Timers ======

    /// Call about every 100 ms.
    pub fn tick(&mut self, now: Instant) {
        if self.state == State::Disconnected {
            return;
        }
        if self.disconnect_at.is_some_and(|at| now >= at) {
            self.cleanup();
            return;
        }
        if self.connect_deadline.is_some_and(|at| now >= at) {
            self.connect_deadline = None;
            self.fail("Connection timeout");
            self.disconnect(now);
            return;
        }

        if let Some(init) = &mut self.init_resend {
            if now - init.first_send > GIVE_UP_AFTER {
                self.fail("Init timeout");
                self.cleanup();
                return;
            }
            if now - init.last_send > RESEND_AFTER {
                init.last_send = now;
                self.outgoing.push_back(init.raw.clone());
            }
        }

        let timed_out = self.resend.iter().find(|(_, p)| now - p.first_send > GIVE_UP_AFTER).map(|(id, _)| *id);
        if let Some(id) = timed_out {
            self.fail(format!("Packet {id} timeout"));
            self.cleanup();
            return;
        }
        for packet in self.resend.values_mut() {
            if now - packet.last_send > RESEND_AFTER {
                packet.last_send = now;
                self.outgoing.push_back(packet.raw.clone());
            }
        }

        if now - self.last_message > GIVE_UP_AFTER {
            self.fail("Connection timeout - no response");
            self.cleanup();
            return;
        }

        if self.state == State::Connected && now - self.last_ping >= PING_EVERY {
            self.last_ping = now;
            self.send_outgoing(Vec::new(), PING, 0, now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(identity: Identity) -> ClientOptions {
        ClientOptions {
            host: "127.0.0.1".into(),
            port: 9987,
            identity,
            nickname: "test".into(),
            server_password: None,
            default_channel: None,
            channel_password: None,
        }
    }

    fn session(now: Instant) -> Session {
        Session::new(options(Identity::generate(0).unwrap()), Arc::default(), now).unwrap()
    }

    /// A command packet from the server, under the dummy keys.
    fn server_command(id: u16, flags: u8, text: &[u8]) -> Vec<u8> {
        let header = [(id >> 8) as u8, id as u8, flags | COMMAND];
        let (ciphertext, mac) = eax_encrypt(DUMMY_KEY, DUMMY_NONCE, &header, text);
        let mut raw = mac.to_vec();
        raw.extend(header);
        raw.extend(ciphertext);
        raw
    }

    fn drain_events(session: &mut Session) -> Vec<Event> {
        std::iter::from_fn(|| session.next_event()).collect()
    }

    fn drain_packets(session: &mut Session) -> Vec<Vec<u8>> {
        std::iter::from_fn(|| session.next_datagram()).collect()
    }

    fn connect(session: &mut Session, now: Instant) {
        session.handle_datagram(&server_command(1, 0, b"initserver aclid=7"), now);
        drain_events(session);
        drain_packets(session);
    }

    #[test]
    fn it_starts_with_the_init_packet() {
        let now = Instant::now();
        let mut s = session(now);
        let packets = drain_packets(&mut s);
        assert_eq!(packets.len(), 1);
        let raw = &packets[0];
        assert_eq!(&raw[..8], INIT_MAC);
        assert_eq!(u16::from_be_bytes([raw[8], raw[9]]), 101);
        assert_eq!(raw[12], FLAG_UNENCRYPTED | INIT1);
        assert_eq!(u32::from_be_bytes(raw[13..17].try_into().unwrap()), INIT_VERSION);
        assert_eq!(raw[17], 0, "init step 0");
    }

    #[test]
    fn it_resends_the_init_packet_every_second_and_gives_up() {
        let now = Instant::now();
        let mut s = session(now);
        drain_packets(&mut s);
        s.tick(now + Duration::from_millis(500));
        assert!(s.next_datagram().is_none());
        s.tick(now + Duration::from_millis(1100));
        assert!(s.next_datagram().is_some());
        s.tick(now + CONNECT_TIMEOUT);
        assert_eq!(drain_events(&mut s), [Event::Error("Connection timeout".into())]);
        s.tick(now + CONNECT_TIMEOUT + DISCONNECT_GRACE);
        assert_eq!(drain_events(&mut s), [Event::Disconnected]);
    }

    #[test]
    fn it_answers_the_server_steps() {
        let now = Instant::now();
        let mut s = session(now);
        drain_packets(&mut s);

        let init = |data: &[u8]| {
            let mut raw = INIT_MAC.to_vec();
            raw.extend([0, 101, INIT1]);
            raw.extend_from_slice(data);
            raw
        };
        let mut step1 = vec![1u8];
        step1.extend(1..=20u8);
        s.handle_datagram(&init(&step1), now);
        let init2 = s.next_datagram().unwrap();
        assert_eq!(init2[17], 2);
        assert_eq!(&init2[18..38], &step1[1..21]);

        // x = 3, n = 7^?: y = 3^(2^2) mod 100 = 81
        let mut step3 = vec![3u8];
        let mut x = [0u8; 64];
        x[63] = 3;
        let mut n = [0u8; 64];
        n[63] = 100;
        step3.extend(x);
        step3.extend(n);
        step3.extend(2i32.to_be_bytes());
        step3.extend([9u8; 100]);
        s.handle_datagram(&init(&step3), now);
        assert_eq!(s.state(), State::Handshake);
        let init4 = s.next_datagram().unwrap();
        let body = &init4[13..];
        assert_eq!(body[4], 4);
        assert_eq!(body[300], 81, "the puzzle answer ends at the last byte");
        let text = String::from_utf8_lossy(&body[301..]);
        assert!(text.starts_with("clientinitiv alpha="), "{text}");
        assert!(text.contains(" ot=1 ip="), "{text}");
    }

    #[test]
    fn commands_are_acknowledged_and_initserver_connects() {
        let now = Instant::now();
        let mut s = session(now);
        drain_packets(&mut s);
        s.handle_datagram(&server_command(5, 0, b"notifycliententerview ctid=3 clid=7|clid=8 client_type=0"), now);
        s.handle_datagram(&server_command(6, 0, b"initserver aclid=7"), now);
        assert_eq!(s.state(), State::Connected);
        assert!(drain_events(&mut s).contains(&Event::Connected));
        let view = s.view.read().unwrap().clone();
        assert_eq!(view.client_id, 7);
        assert_eq!(view.current_channel, 3, "enter-view that raced ahead of the id still counts");
        assert!(view.channel_members.contains(&8));

        let packets = drain_packets(&mut s);
        let ack = &packets[0];
        assert_eq!(ack[12] & 0x0f, ACK);
        let (key, nonce) = (DUMMY_KEY, DUMMY_NONCE);
        let plain = eax_decrypt(key, nonce, &ack[8..13], &ack[13..], &ack[..8]).unwrap();
        assert_eq!(plain, [0, 5]);
    }

    #[test]
    fn a_stray_backslash_does_not_end_the_connection() {
        let now = Instant::now();
        let mut s = session(now);
        connect(&mut s, now);
        s.handle_datagram(&server_command(2, 0, b"notifytextmessage msg=hello\\"), now);
        assert_eq!(s.state(), State::Connected);
        let events = drain_events(&mut s);
        assert!(events.iter().any(|e| matches!(e, Event::TextMessage(p) if p["msg"] == "hello\\")));
    }

    #[test]
    fn invalid_utf8_and_broken_compression_do_not_end_the_connection() {
        let now = Instant::now();
        let mut s = session(now);
        connect(&mut s, now);
        s.handle_datagram(&server_command(2, 0, &[0xff, 0xfe, b' ', 0xfa]), now);
        s.handle_datagram(&server_command(3, FLAG_COMPRESSED, &[0x05, 1]), now);
        assert_eq!(s.state(), State::Connected);
    }

    #[test]
    fn fragments_are_put_back_together() {
        let now = Instant::now();
        let mut s = session(now);
        connect(&mut s, now);
        s.handle_datagram(&server_command(2, FLAG_FRAGMENTED, b"notifytextmessage "), now);
        s.handle_datagram(&server_command(3, 0, b"msg=he"), now);
        assert!(!drain_events(&mut s).iter().any(|e| matches!(e, Event::TextMessage(_))));
        s.handle_datagram(&server_command(4, FLAG_FRAGMENTED, b"llo"), now);
        let events = drain_events(&mut s);
        assert!(events.iter().any(|e| matches!(e, Event::TextMessage(p) if p["msg"] == "hello")));
    }

    #[test]
    fn long_commands_are_split_with_the_flag_on_both_ends() {
        let now = Instant::now();
        let mut s = session(now);
        connect(&mut s, now);
        let long = format!("sendtextmessage msg={}", "a".repeat(MAX_TS_COMMAND_BYTES * 2));
        s.send_command(&long, now);
        let packets = drain_packets(&mut s);
        assert_eq!(packets.len(), 3);
        assert!(packets.iter().all(|p| p.len() <= 500));
        let flags: Vec<_> = packets.iter().map(|p| p[12] & FLAG_FRAGMENTED).collect();
        assert_eq!(flags, [FLAG_FRAGMENTED, 0, FLAG_FRAGMENTED]);
    }

    #[test]
    fn a_refusal_while_connecting_ends_the_attempt_but_a_later_error_does_not() {
        let now = Instant::now();
        let mut s = session(now);
        s.handle_datagram(&server_command(2, 0, b"error id=1541 msg=bad\\snickname"), now);
        assert!(drain_events(&mut s).contains(&Event::Error("TS3 error 1541: bad nickname".into())));

        let mut s = session(now);
        connect(&mut s, now);
        s.handle_datagram(&server_command(2, 0, b"error id=2568 msg=no\\spermission"), now);
        assert_eq!(s.state(), State::Connected);
        assert!(!drain_events(&mut s).iter().any(|e| matches!(e, Event::Error(_))));
        assert!(is_connection_refusal(3329, false));
        assert!(!is_connection_refusal(0, true));
    }

    #[test]
    fn it_pings_every_second_and_times_out_without_answers() {
        let now = Instant::now();
        let mut s = session(now);
        connect(&mut s, now);
        s.tick(now + PING_EVERY);
        let ping = s.next_datagram().unwrap();
        assert_eq!(ping[12], FLAG_UNENCRYPTED | PING);
        s.tick(now + GIVE_UP_AFTER + Duration::from_secs(1));
        let events = drain_events(&mut s);
        assert!(events.contains(&Event::Disconnected));
        assert!(events.iter().any(|e| matches!(e, Event::Error(_))));
    }

    #[test]
    fn acknowledged_commands_stop_being_resent() {
        let now = Instant::now();
        let mut s = session(now);
        connect(&mut s, now);
        s.send_command("clientlist", now);
        let raw = s.next_datagram().unwrap();
        let id = u16::from_be_bytes([raw[8], raw[9]]);
        s.tick(now + Duration::from_millis(1100));
        assert!(drain_packets(&mut s).iter().any(|p| p == &raw), "resent after a second");

        let header = [0, 1, ACK];
        let (ciphertext, mac) = eax_encrypt(DUMMY_KEY, DUMMY_NONCE, &header, &id.to_be_bytes());
        let mut ack = mac.to_vec();
        ack.extend(header);
        ack.extend(ciphertext);
        s.handle_datagram(&ack, now);
        s.tick(now + Duration::from_millis(2300));
        assert!(!drain_packets(&mut s).iter().any(|p| p == &raw));
    }

    #[test]
    fn voice_packets_carry_their_id_and_the_opus_codec() {
        let now = Instant::now();
        let mut s = session(now);
        s.send_voice(b"x", now);
        drain_packets(&mut s);
        assert_eq!(s.packet_counter[usize::from(VOICE)], 0, "no voice before connecting");
        connect(&mut s, now);
        s.send_voice(b"opus", now);
        let raw = s.next_datagram().unwrap();
        let plain = eax_decrypt(DUMMY_KEY, DUMMY_NONCE, &raw[8..13], &raw[13..], &raw[..8]).unwrap();
        assert_eq!(plain, [0, 0, OPUS_MUSIC, b'o', b'p', b'u', b's']);
    }

    #[test]
    fn counters_roll_into_the_next_generation() {
        let now = Instant::now();
        let mut s = session(now);
        let index = usize::from(COMMAND);
        s.packet_counter[index] = u16::MAX;
        s.inc_counter(COMMAND);
        assert_eq!(s.packet_counter[index], 0);
        assert_eq!(s.generation[index], 1);
    }

    #[test]
    fn short_or_foreign_datagrams_are_ignored() {
        let now = Instant::now();
        let mut s = session(now);
        drain_packets(&mut s);
        s.handle_datagram(&[1, 2, 3], now);
        s.handle_datagram(&[0xaa; 40], now);
        assert!(drain_events(&mut s).is_empty());
        assert!(s.next_datagram().is_none());
    }
}
