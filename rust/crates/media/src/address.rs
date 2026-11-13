//! Which addresses media may be fetched from: the checks that stop a URL typed in chat from
//! reaching this machine, its network or a cloud metadata service.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use tokio::net::lookup_host;
use url::{Host, Url};

use crate::{err, Result};

const BLOCKED_HOSTNAMES: [&str; 3] = ["localhost", "metadata.google.internal", "metadata.internal"];

fn in_v4(ip: Ipv4Addr, network: [u8; 4], prefix: u32) -> bool {
    let mask = if prefix == 0 { 0 } else { u32::MAX << (32 - prefix) };
    u32::from(ip) & mask == u32::from(Ipv4Addr::from(network)) & mask
}

fn in_v6(ip: Ipv6Addr, network: &str, prefix: u32) -> bool {
    let network: Ipv6Addr = network.parse().expect("fixed network");
    let mask = if prefix == 0 { 0 } else { u128::MAX << (128 - prefix) };
    u128::from(ip) & mask == u128::from(network) & mask
}

/// Metadata services, loopback, link-local, unspecified and multicast: denied for every source.
pub fn is_never_allowed(ip: IpAddr) -> bool {
    match ip.to_canonical() {
        IpAddr::V4(ip) => {
            ip == Ipv4Addr::new(100, 100, 100, 200)
                || [([0, 0, 0, 0], 8), ([127, 0, 0, 0], 8), ([169, 254, 0, 0], 16), ([224, 0, 0, 0], 4)]
                    .into_iter()
                    .any(|(network, prefix)| in_v4(ip, network, prefix))
        }
        IpAddr::V6(ip) => {
            ip.is_unspecified()
                || ip.is_loopback()
                || ip == "fd00:ec2::254".parse::<Ipv6Addr>().expect("fixed address")
                || in_v6(ip, "fe80::", 10)
                || in_v6(ip, "ff00::", 8)
        }
    }
}

/// Never-allowed addresses plus private and shared ranges.
pub fn is_private(ip: IpAddr) -> bool {
    if is_never_allowed(ip) {
        return true;
    }
    match ip.to_canonical() {
        IpAddr::V4(ip) => [([10, 0, 0, 0], 8), ([172, 16, 0, 0], 12), ([192, 168, 0, 0], 16), ([100, 64, 0, 0], 10)]
            .into_iter()
            .any(|(network, prefix)| in_v4(ip, network, prefix)),
        IpAddr::V6(ip) => in_v6(ip, "fc00::", 7),
    }
}

/// The IPv4 address an IPv6 address stands for: mapped, translated, compatible, NAT64 or 6to4.
pub fn embedded_ipv4(ip: Ipv6Addr) -> Option<Ipv4Addr> {
    let b = ip.octets();
    let zeros = |from: usize, to: usize| b[from..to].iter().all(|x| *x == 0);
    let tail = Ipv4Addr::new(b[12], b[13], b[14], b[15]);
    if zeros(0, 10) && b[10] == 0xff && b[11] == 0xff {
        return Some(tail);
    }
    if zeros(0, 8) && b[8] == 0xff && b[9] == 0xff && zeros(10, 12) {
        return Some(tail);
    }
    if zeros(0, 12) && !zeros(12, 16) {
        return Some(tail);
    }
    if b[..4] == [0x00, 0x64, 0xff, 0x9b] && zeros(4, 12) {
        return Some(tail);
    }
    if b[..2] == [0x20, 0x02] {
        return Some(Ipv4Addr::new(b[2], b[3], b[4], b[5]));
    }
    None
}

/// Whether media downloads may use this address. Stricter than [`validate_url`]: no LAN
/// allowance, more reserved ranges, and embedded IPv4 addresses are judged as IPv4.
pub fn media_egress_allowed(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let reserved = [
                ([192, 0, 0, 0], 24),
                ([192, 0, 2, 0], 24),
                ([192, 88, 99, 0], 24),
                ([198, 18, 0, 0], 15),
                ([198, 51, 100, 0], 24),
                ([203, 0, 113, 0], 24),
                ([240, 0, 0, 0], 4),
            ];
            !reserved.into_iter().any(|(network, prefix)| in_v4(ip, network, prefix)) && !is_private(IpAddr::V4(ip))
        }
        IpAddr::V6(ip) => {
            let reserved =
                [("100::", 64), ("2001::", 23), ("2001:db8::", 32), ("3fff::", 20), ("64:ff9b:1::", 48), ("fec0::", 10)];
            if reserved.into_iter().any(|(network, prefix)| in_v6(ip, network, prefix)) {
                return false;
            }
            match embedded_ipv4(ip) {
                Some(v4) => media_egress_allowed(IpAddr::V4(v4)),
                None => !is_private(IpAddr::V6(ip)),
            }
        }
    }
}

/// The checks that need no DNS. Returns the hostname still to be resolved, if any.
fn check_literal(url: &str) -> Result<Option<(String, u16)>> {
    let Ok(parsed) = Url::parse(url) else {
        return err("Invalid URL format");
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        return err(format!("Protocol \"{}:\" is not allowed. Use: http:, https:", parsed.scheme()));
    }
    let port = parsed.port_or_known_default().unwrap_or(80);
    let ip = match parsed.host() {
        Some(Host::Domain(name)) => {
            let name = name.trim_end_matches('.').to_ascii_lowercase();
            if BLOCKED_HOSTNAMES.contains(&name.as_str()) {
                return err(format!("Hostname \"{name}\" is blocked"));
            }
            return Ok(Some((name, port)));
        }
        Some(Host::Ipv4(ip)) => IpAddr::V4(ip),
        Some(Host::Ipv6(ip)) => IpAddr::V6(ip),
        None => return err("Invalid URL format"),
    };
    if is_never_allowed(ip) {
        return err("Reserved or cloud metadata IP addresses are blocked");
    }
    if is_private(ip) {
        return err("Private/reserved IP addresses are blocked");
    }
    Ok(None)
}

/// Check a URL before anything connects to it. An unresolved name is refused.
pub async fn validate_url(url: &str) -> Result<()> {
    let Some((host, port)) = check_literal(url)? else {
        return Ok(());
    };
    let Ok(addresses) = lookup_host((host.as_str(), port)).await else {
        return err(format!("Hostname \"{host}\" could not be resolved"));
    };
    let addresses: Vec<_> = addresses.map(|address| address.ip()).collect();
    if addresses.is_empty() {
        return err(format!("Hostname \"{host}\" could not be resolved"));
    }
    for ip in addresses {
        if is_never_allowed(ip) {
            return err(format!("Hostname \"{host}\" resolves to a reserved or cloud metadata IP"));
        }
        if is_private(ip) {
            return err(format!("Hostname \"{host}\" resolves to a private IP ({ip})"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    #[test]
    fn never_allowed_covers_every_spelling() {
        for text in ["127.0.0.1", "169.254.169.254", "100.100.100.200", "0.0.0.0", "224.0.0.1", "::1", "::", "fe80::1", "ff02::1", "fd00:ec2::254", "::ffff:127.0.0.1"] {
            assert!(is_never_allowed(ip(text)), "{text}");
        }
        for text in ["8.8.8.8", "10.0.0.1", "2606:4700::1111"] {
            assert!(!is_never_allowed(ip(text)), "{text}");
        }
    }

    #[test]
    fn private_ranges_are_found() {
        for text in ["10.1.2.3", "172.16.0.1", "172.31.255.255", "192.168.1.1", "100.64.0.1", "fc00::1", "fd12::1", "::ffff:192.168.0.1"] {
            assert!(is_private(ip(text)), "{text}");
        }
        for text in ["172.32.0.1", "100.128.0.1", "1.1.1.1"] {
            assert!(!is_private(ip(text)), "{text}");
        }
    }

    #[test]
    fn embedded_addresses_are_judged_as_ipv4() {
        for (text, inner) in [
            ("::ffff:10.0.0.1", "10.0.0.1"),
            ("::ffff:0:10.0.0.1", "10.0.0.1"),
            ("::10.0.0.1", "10.0.0.1"),
            ("64:ff9b::a00:1", "10.0.0.1"),
            ("2002:a00:1::", "10.0.0.1"),
        ] {
            let IpAddr::V6(v6) = ip(text) else { panic!() };
            assert_eq!(embedded_ipv4(v6), Some(inner.parse().unwrap()), "{text}");
            assert!(!media_egress_allowed(ip(text)), "{text}");
        }
        assert!(media_egress_allowed(ip("::ffff:8.8.8.8")));
    }

    #[test]
    fn media_egress_refuses_reserved_ranges() {
        for text in ["192.0.2.1", "198.18.0.1", "198.51.100.1", "203.0.113.1", "240.0.0.1", "255.255.255.255", "192.168.0.1", "2001:db8::1", "3fff::1", "fec0::1", "64:ff9b:1::1", "100::1"] {
            assert!(!media_egress_allowed(ip(text)), "{text}");
        }
        for text in ["8.8.8.8", "2606:4700:4700::1111"] {
            assert!(media_egress_allowed(ip(text)), "{text}");
        }
    }

    #[test]
    fn urls_are_refused_before_any_lookup() {
        for url in [
            "file:///etc/passwd",
            "ftp://example.com/a",
            "not a url",
            "http://localhost/",
            "http://LOCALHOST./",
            "http://metadata.google.internal/",
            "http://127.0.0.1/",
            "http://2130706433/",
            "http://0x7f.0.0.1/",
            "http://[::1]/",
            "http://[::ffff:169.254.169.254]/",
            "http://192.168.1.10:8000/stream",
        ] {
            assert!(check_literal(url).is_err(), "{url}");
        }
        assert_eq!(check_literal("https://8.8.8.8/x").unwrap(), None);
        assert_eq!(check_literal("https://radio.example.com/x").unwrap(), Some(("radio.example.com".to_owned(), 443)));
    }

    #[tokio::test]
    async fn a_name_that_does_not_resolve_is_refused() {
        assert!(validate_url("http://this-name-does-not-exist.invalid/").await.is_err());
    }
}
