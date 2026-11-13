//! The current title of an ICY/Shoutcast/Icecast stream.
//!
//! Uses a raw socket because such servers answer "ICY 200 OK", which HTTP clients refuse.

use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::rustls::{ClientConfig, RootCertStore};
use tokio_rustls::TlsConnector;
use url::Url;

use crate::address::validate_url;

const MAX_REDIRECTS: usize = 5;
const TIMEOUT: Duration = Duration::from_secs(10);
/// Largest response read while looking for the title; servers use up to 64 KiB between titles.
const MAX_BUFFER: usize = 1024 * 1024;
/// ffmpeg's name, so a station that lets the audio through also gives the title. SomaFM
/// answers 403 to the old program's own name, while ffmpeg plays its audio.
const USER_AGENT: &str = "Lavf/60.16.100";

#[derive(Debug, PartialEq, Eq)]
enum Step {
    NeedMore,
    Title(Option<String>),
    Redirect(String),
}

/// Decide what the response read so far says.
fn parse_response(buffer: &[u8]) -> Step {
    let Some(header_end) = buffer.windows(4).position(|w| w == b"\r\n\r\n") else {
        return Step::NeedMore;
    };
    // Latin-1, like the headers themselves.
    let headers: String = buffer[..header_end].iter().map(|b| char::from(*b)).collect();
    let mut lines = headers.split("\r\n");
    let status = lines.next().unwrap_or_default();
    let is_redirect = status.split_whitespace().any(|word| ["301", "302", "303", "307", "308"].contains(&word));
    let mut meta_interval = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        let name = name.trim().to_ascii_lowercase();
        if is_redirect && name == "location" {
            return Step::Redirect(value.trim().to_owned());
        }
        if name == "icy-metaint" {
            meta_interval = value.trim().parse::<usize>().ok();
        }
    }
    let Some(interval) = meta_interval.filter(|interval| *interval > 0) else {
        return Step::Title(None);
    };

    let body = &buffer[header_end + 4..];
    if body.len() <= interval {
        return Step::NeedMore;
    }
    let length = usize::from(body[interval]) * 16;
    if length == 0 {
        return Step::Title(None);
    }
    if body.len() < interval + 1 + length {
        return Step::NeedMore;
    }
    let metadata = String::from_utf8_lossy(&body[interval + 1..interval + 1 + length]);
    Step::Title(stream_title(&metadata))
}

fn stream_title(metadata: &str) -> Option<String> {
    let start = metadata.find("StreamTitle='")? + "StreamTitle='".len();
    let rest = &metadata[start..];
    // The value ends at "';": a title may hold an apostrophe ("Guns N' Roses").
    let end = rest.find("';").or_else(|| rest.rfind('\''))?;
    let title = rest[..end].trim();
    (!title.is_empty()).then(|| title.to_owned())
}

fn tls_connector() -> TlsConnector {
    let roots = RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
    let config = ClientConfig::builder_with_provider(Arc::new(tokio_rustls::rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .expect("default protocol versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
    TlsConnector::from(Arc::new(config))
}

async fn read_step(mut stream: impl AsyncRead + AsyncWrite + Unpin, request: &str) -> Option<Step> {
    stream.write_all(request.as_bytes()).await.ok()?;
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
        match parse_response(&buffer) {
            Step::NeedMore if buffer.len() > MAX_BUFFER => return None,
            Step::NeedMore => {}
            step => return Some(step),
        }
    }
}

async fn fetch_one(url: &Url) -> Option<Step> {
    let host = url.host_str()?;
    let port = url.port_or_known_default()?;
    let mut path = url.path().to_owned();
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }
    let request =
        format!("GET {path} HTTP/1.0\r\nHost: {host}\r\nUser-Agent: {USER_AGENT}\r\nIcy-MetaData: 1\r\nConnection: close\r\n\r\n");
    let bare_host = host.trim_start_matches('[').trim_end_matches(']');
    let socket = TcpStream::connect((bare_host, port)).await.ok()?;
    if url.scheme() == "https" {
        let name = ServerName::try_from(bare_host.to_owned()).ok()?;
        read_step(tls_connector().connect(name, socket).await.ok()?, &request).await
    } else {
        read_step(socket, &request).await
    }
}

/// The current title, or `None` when the stream has none or anything fails. Every address,
/// including each redirect target, passes [`validate_url`] first.
pub async fn fetch_title(stream_url: &str) -> Option<String> {
    let mut current = Url::parse(stream_url).ok()?;
    for _ in 0..=MAX_REDIRECTS {
        validate_url(current.as_str()).await.ok()?;
        match tokio::time::timeout(TIMEOUT, fetch_one(&current)).await.ok()?? {
            Step::Title(title) => return title,
            Step::Redirect(location) => current = current.join(&location).ok()?,
            Step::NeedMore => return None,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(headers: &str, body: &[u8]) -> Vec<u8> {
        let mut out = format!("{headers}\r\n\r\n").into_bytes();
        out.extend_from_slice(body);
        out
    }

    fn with_metadata(text: &str) -> Vec<u8> {
        let blocks = text.len().div_ceil(16);
        let mut body = vec![0u8; 4];
        body.push(blocks as u8);
        body.extend_from_slice(text.as_bytes());
        body.resize(5 + blocks * 16, 0);
        response("ICY 200 OK\r\nicy-metaint: 4", &body)
    }

    #[test]
    fn the_title_comes_from_the_first_block() {
        let data = with_metadata("StreamTitle='Tarkan - Kuzu Kuzu';StreamUrl='';");
        assert_eq!(parse_response(&data), Step::Title(Some("Tarkan - Kuzu Kuzu".to_owned())));
    }

    #[test]
    fn apostrophes_stay_in_the_title() {
        assert_eq!(stream_title("StreamTitle='Guns N' Roses - Patience';"), Some("Guns N' Roses - Patience".to_owned()));
        assert_eq!(stream_title("StreamTitle='Son'"), Some("Son".to_owned()));
    }

    #[test]
    fn empty_titles_are_none() {
        assert_eq!(stream_title("StreamTitle='  ';"), None);
        assert_eq!(stream_title("Other='x';"), None);
    }

    #[test]
    fn every_partial_read_asks_for_more() {
        let data = with_metadata("StreamTitle='Abc';");
        for length in 0..data.len() {
            assert_eq!(parse_response(&data[..length]), Step::NeedMore, "at {length}");
        }
    }

    #[test]
    fn a_stream_without_metadata_has_no_title() {
        assert_eq!(parse_response(&response("HTTP/1.0 200 OK\r\nContent-Type: audio/mpeg", b"xx")), Step::Title(None));
        assert_eq!(parse_response(&response("ICY 200 OK\r\nicy-metaint: 0", b"xx")), Step::Title(None));
        assert_eq!(parse_response(&response("ICY 200 OK\r\nICY-METAINT: 2", &[1, 2, 0])), Step::Title(None));
    }

    #[test]
    fn redirects_are_followed() {
        assert_eq!(
            parse_response(&response("HTTP/1.1 302 Found\r\nlocation: https://b.example/live", b"")),
            Step::Redirect("https://b.example/live".to_owned())
        );
    }

    #[tokio::test]
    async fn local_addresses_are_never_contacted() {
        assert_eq!(fetch_title("http://127.0.0.1:1/x").await, None);
    }
}
