//! Checking a TeamSpeak address before any connection to it: no URL tricks, no cloud metadata
//! service, and on request no private addresses. Unlike media addresses, a TeamSpeak server on the
//! same network is normal, so private addresses are allowed by default.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::{Error, Result};

const BLOCKED_HOSTNAMES: [&str; 4] = ["metadata", "metadata.google", "metadata.google.internal", "kubernetes.default.svc"];
const BLOCKED_IPV4: [Ipv4Addr; 4] =
    [Ipv4Addr::new(169, 254, 169, 254), Ipv4Addr::new(169, 254, 170, 2), Ipv4Addr::new(100, 100, 100, 200), Ipv4Addr::UNSPECIFIED];

fn bad(message: &str) -> Error {
    Error::Invalid(message.to_owned())
}

/// `TS_ALLOW_PRIVATE_HOSTS`: on unless set to `false` or `0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostPolicy {
    pub allow_private: bool,
}

impl HostPolicy {
    pub fn from_env() -> Self {
        Self { allow_private: !matches!(std::env::var("TS_ALLOW_PRIVATE_HOSTS").as_deref(), Ok("false" | "0")) }
    }
}

impl Default for HostPolicy {
    fn default() -> Self {
        Self { allow_private: true }
    }
}

fn is_private_or_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, ..] = ip.octets();
            a == 127 || a == 10 || (a == 172 && (16..=31).contains(&b)) || (a == 192 && b == 168) || (a == 169 && b == 254)
        }
        IpAddr::V6(ip) => ip == Ipv6Addr::LOCALHOST || (ip.segments()[0] & 0xffc0) == 0xfe80 || (ip.segments()[0] & 0xfe00) == 0xfc00,
    }
}

fn is_hostname(host: &str) -> bool {
    if host.is_empty() || host.len() > 253 {
        return false;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    })
}

/// A hostname or address in its plain form, or why it cannot be used.
pub fn sanitize_host(host: &str, policy: HostPolicy) -> Result<String> {
    let trimmed = host.trim();
    if trimmed.is_empty() {
        return Err(bad("Host is required"));
    }
    if trimmed.contains("://") || trimmed.chars().any(|c| c.is_whitespace() || matches!(c, '/' | '\\' | '?' | '#' | '@')) {
        return Err(bad("Host must be a hostname or IP address without URL path or credentials"));
    }
    let bare = trimmed.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')).unwrap_or(trimmed);
    let normalized = bare.strip_suffix('.').unwrap_or(bare).to_ascii_lowercase();

    if BLOCKED_HOSTNAMES.contains(&normalized.as_str()) {
        return Err(bad("Host is not allowed"));
    }
    let private_refused = "Private or loopback hosts are disabled (set TS_ALLOW_PRIVATE_HOSTS=true to allow)";
    match normalized.parse::<IpAddr>() {
        Ok(ip) => {
            if matches!(ip, IpAddr::V4(v4) if BLOCKED_IPV4.contains(&v4)) {
                return Err(bad("Host is not allowed"));
            }
            if !policy.allow_private && is_private_or_loopback(ip) {
                return Err(bad(private_refused));
            }
        }
        Err(_) if !is_hostname(&normalized) => return Err(bad("Invalid hostname")),
        Err(_) => {}
    }
    Ok(normalized)
}

/// A port, or the default when none was given.
pub fn validate_port(port: Option<i64>, default: u16) -> Result<u16> {
    let value = port.unwrap_or(i64::from(default));
    u16::try_from(value).ok().filter(|port| *port >= 1).ok_or_else(|| bad("Port must be an integer between 1 and 65535"))
}

/// `/sid/command`, or `/command` for instance commands (sid 0). The command cannot change the
/// host or path: no slashes, schemes or `//`.
pub fn web_query_path(sid: u32, command: &str) -> Result<String> {
    if sid > 65_535 {
        return Err(bad("Server id must be an integer between 0 and 65535"));
    }
    let command = command.trim();
    let mut bytes = command.bytes();
    let starts_with_letter = bytes.next().is_some_and(|b| b.is_ascii_alphabetic());
    if !starts_with_letter || command.len() > 64 || !bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
        return Err(bad("Invalid WebQuery command"));
    }
    Ok(if sid > 0 { format!("/{sid}/{command}") } else { format!("/{command}") })
}

/// A checked address to connect to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub origin: String,
    pub host: String,
    pub port: u16,
    pub use_https: bool,
}

impl Endpoint {
    pub fn new(host: &str, port: Option<i64>, use_https: bool, default_port: u16, policy: HostPolicy) -> Result<Self> {
        let host = sanitize_host(host, policy)?;
        let port = validate_port(port, default_port)?;
        let scheme = if use_https { "https" } else { "http" };
        let origin = if host.parse::<Ipv6Addr>().is_ok() { format!("{scheme}://[{host}]:{port}") } else { format!("{scheme}://{host}:{port}") };
        Ok(Self { origin, host, port, use_https })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPEN: HostPolicy = HostPolicy { allow_private: true };
    const STRICT: HostPolicy = HostPolicy { allow_private: false };

    #[test]
    fn hosts_are_normalized() {
        assert_eq!(sanitize_host(" TS.Example.COM. ", OPEN).unwrap(), "ts.example.com");
        assert_eq!(sanitize_host("[::1]", OPEN).unwrap(), "::1");
        assert_eq!(sanitize_host("203.0.113.5", OPEN).unwrap(), "203.0.113.5");
        assert_eq!(sanitize_host("teamspeak", OPEN).unwrap(), "teamspeak");
    }

    #[test]
    fn url_tricks_and_metadata_are_refused() {
        for host in [
            "",
            "http://x",
            "x/y",
            "user@x",
            "x y",
            "x?a",
            "x#a",
            "x\\y",
            "metadata.google.internal",
            "169.254.169.254",
            "100.100.100.200",
            "0.0.0.0",
            "-bad.example",
            "bad_host",
            &"a".repeat(64),
        ] {
            assert!(sanitize_host(host, OPEN).is_err(), "{host:?}");
        }
    }

    #[test]
    fn private_addresses_depend_on_the_setting() {
        for host in ["127.0.0.1", "10.0.0.1", "172.16.0.1", "192.168.1.1", "::1", "fe80::1", "fd00::1"] {
            assert!(sanitize_host(host, OPEN).is_ok(), "{host}");
            assert!(sanitize_host(host, STRICT).is_err(), "{host}");
        }
        assert!(sanitize_host("172.32.0.1", STRICT).is_ok());
    }

    #[test]
    fn ports_are_checked() {
        assert_eq!(validate_port(None, 10080).unwrap(), 10080);
        assert_eq!(validate_port(Some(1), 10080).unwrap(), 1);
        for port in [0, -1, 65_536] {
            assert!(validate_port(Some(port), 10080).is_err(), "{port}");
        }
    }

    #[test]
    fn query_paths_stay_on_the_origin() {
        assert_eq!(web_query_path(0, "version").unwrap(), "/version");
        assert_eq!(web_query_path(3, "clientlist").unwrap(), "/3/clientlist");
        for command in ["", "/x", "a/b", "//evil.example", "http://x", "1a", "a b", "a?b=1", &"a".repeat(65)] {
            assert!(web_query_path(1, command).is_err(), "{command:?}");
        }
        assert!(web_query_path(65_536, "x").is_err());
    }

    #[test]
    fn origins_are_built_from_checked_parts() {
        assert_eq!(Endpoint::new("Ts.Example", Some(10443), true, 10080, OPEN).unwrap().origin, "https://ts.example:10443");
        assert_eq!(Endpoint::new("::1", None, false, 10080, OPEN).unwrap().origin, "http://[::1]:10080");
    }
}
