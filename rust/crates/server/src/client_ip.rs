//! Who is asking: the address rate limits count against, honouring `TRUST_PROXY`.

use std::net::IpAddr;

use axum::http::HeaderMap;

/// Parsed `TRUST_PROXY`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TrustProxy {
    /// Forwarded headers are ignored.
    #[default]
    None,
    /// One hop, only when the connecting peer has a private address.
    PrivatePeer,
    List(Vec<ProxyRule>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyRule {
    Loopback,
    LinkLocal,
    UniqueLocal,
    Network(IpAddr, u8),
}

const NONE_WORDS: [&str; 4] = ["false", "none", "off", "no"];

/// `true` and hop counts are refused on purpose: they trust whatever sits at that position and
/// let a direct client choose its own rate-limit key.
pub fn parse_trust_proxy(raw: Option<&str>) -> Result<TrustProxy, String> {
    let tokens: Vec<String> = raw.unwrap_or_default().split(',').map(|token| token.trim().to_owned()).filter(|token| !token.is_empty()).collect();
    if tokens.is_empty() {
        return Ok(TrustProxy::None);
    }
    let lower: Vec<String> = tokens.iter().map(|token| token.to_lowercase()).collect();
    let is_none = |token: &String| NONE_WORDS.contains(&token.as_str());
    if lower.iter().all(is_none) {
        return Ok(TrustProxy::None);
    }
    if lower.iter().any(is_none) {
        return Err("TRUST_PROXY mixes a \"none\" value (false/none/off/no) with proxy addresses; use one or the other.".to_owned());
    }
    if lower.iter().any(|token| token == "private-peer") {
        if lower.iter().any(|token| token != "private-peer") {
            return Err("TRUST_PROXY mixes \"private-peer\" with proxy addresses; list every proxy address or range explicitly instead.".to_owned());
        }
        return Ok(TrustProxy::PrivatePeer);
    }
    let mut rules = Vec::with_capacity(tokens.len());
    for (token, lower) in tokens.iter().zip(&lower) {
        if lower == "true" || token.bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!(
                "TRUST_PROXY={token} is not supported: boolean and hop-count values trust any client at that position. Use proxy IPs/CIDRs or loopback, linklocal, uniquelocal, private-peer, or \"none\"."
            ));
        }
        rules.push(match lower.as_str() {
            "loopback" => ProxyRule::Loopback,
            "linklocal" => ProxyRule::LinkLocal,
            "uniquelocal" => ProxyRule::UniqueLocal,
            _ => parse_network(token).ok_or_else(|| format!("TRUST_PROXY value {token} is not an address or range."))?,
        });
    }
    Ok(TrustProxy::List(rules))
}

fn parse_network(token: &str) -> Option<ProxyRule> {
    match token.split_once('/') {
        Some((address, bits)) => {
            let address: IpAddr = address.parse().ok()?;
            let bits: u8 = bits.parse().ok()?;
            let max = if address.is_ipv4() { 32 } else { 128 };
            (bits <= max).then_some(ProxyRule::Network(address, bits))
        }
        None => {
            let address: IpAddr = token.parse().ok()?;
            Some(ProxyRule::Network(address, if address.is_ipv4() { 32 } else { 128 }))
        }
    }
}

fn in_network(ip: IpAddr, network: IpAddr, bits: u8) -> bool {
    match (ip, network) {
        (IpAddr::V4(ip), IpAddr::V4(network)) => {
            let mask = if bits == 0 { 0 } else { u32::MAX << (32 - u32::from(bits)) };
            u32::from(ip) & mask == u32::from(network) & mask
        }
        (IpAddr::V6(ip), IpAddr::V6(network)) => {
            let mask = if bits == 0 { 0 } else { u128::MAX << (128 - u32::from(bits)) };
            u128::from(ip) & mask == u128::from(network) & mask
        }
        _ => false,
    }
}

fn is_unique_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_private(),
        IpAddr::V6(ip) => (ip.segments()[0] & 0xfe00) == 0xfc00,
    }
}

fn is_link_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_link_local(),
        IpAddr::V6(ip) => (ip.segments()[0] & 0xffc0) == 0xfe80,
    }
}

impl ProxyRule {
    fn matches(self, ip: IpAddr) -> bool {
        match self {
            Self::Loopback => ip.is_loopback(),
            Self::LinkLocal => is_link_local(ip),
            Self::UniqueLocal => is_unique_local(ip),
            Self::Network(network, bits) => in_network(ip, network, bits),
        }
    }
}

/// The client: the first address from the right that is not a trusted proxy.
pub fn client_ip(setting: &TrustProxy, peer: Option<IpAddr>, headers: &HeaderMap) -> Option<IpAddr> {
    let peer = peer.map(|ip| ip.to_canonical());
    let forwarded = || -> Vec<IpAddr> {
        headers
            .get_all("x-forwarded-for")
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|value| value.split(','))
            .filter_map(|part| part.trim().parse::<IpAddr>().ok())
            .map(|ip| ip.to_canonical())
            .collect()
    };
    match setting {
        TrustProxy::None => peer,
        TrustProxy::PrivatePeer => {
            let address = peer?;
            if !is_unique_local(address) {
                return Some(address);
            }
            // Only what the proxy itself observed: nothing further left is believed.
            Some(forwarded().last().copied().unwrap_or(address))
        }
        TrustProxy::List(rules) => {
            let mut current = peer?;
            let chain = forwarded();
            for address in chain.iter().rev() {
                if !rules.iter().any(|rule| rule.matches(current)) {
                    return Some(current);
                }
                current = *address;
            }
            Some(current)
        }
    }
}

/// The rate-limit key: an IPv4 address, or an IPv6 /64 since one host usually controls one.
pub fn address_rate_key(ip: Option<IpAddr>) -> String {
    match ip.map(|ip| ip.to_canonical()) {
        Some(IpAddr::V4(ip)) => format!("ipv4:{ip}"),
        Some(IpAddr::V6(ip)) => {
            let hex: String = ip.octets()[..8].iter().map(|byte| format!("{byte:02x}")).collect();
            format!("ipv6:{hex}/64")
        }
        None => "unknown".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(text: &str) -> Option<IpAddr> {
        Some(text.parse().unwrap())
    }

    fn forwarded(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", value.parse().unwrap());
        headers
    }

    #[test]
    fn settings_are_parsed_and_dangerous_ones_refused() {
        assert_eq!(parse_trust_proxy(None), Ok(TrustProxy::None));
        assert_eq!(parse_trust_proxy(Some(" ")), Ok(TrustProxy::None));
        assert_eq!(parse_trust_proxy(Some("false,off")), Ok(TrustProxy::None));
        assert_eq!(parse_trust_proxy(Some("private-peer")), Ok(TrustProxy::PrivatePeer));
        assert!(matches!(parse_trust_proxy(Some("loopback,10.0.0.0/8")), Ok(TrustProxy::List(rules)) if rules.len() == 2));
        for bad in ["true", "1", "none,10.0.0.1", "private-peer,10.0.0.1", "nonsense", "10.0.0.0/99"] {
            assert!(parse_trust_proxy(Some(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn forwarded_headers_are_ignored_unless_a_proxy_is_trusted() {
        assert_eq!(client_ip(&TrustProxy::None, ip("8.8.8.8"), &forwarded("1.1.1.1")), ip("8.8.8.8"));
    }

    #[test]
    fn a_trusted_list_walks_from_the_right() {
        let setting = parse_trust_proxy(Some("10.0.0.0/8")).unwrap();
        assert_eq!(client_ip(&setting, ip("10.0.0.1"), &forwarded("6.6.6.6, 1.1.1.1, 10.0.0.2")), ip("1.1.1.1"), "a client cannot pick its own key");
        assert_eq!(client_ip(&setting, ip("8.8.8.8"), &forwarded("1.1.1.1")), ip("8.8.8.8"), "an untrusted peer is the client");
        assert_eq!(client_ip(&setting, ip("10.0.0.1"), &HeaderMap::new()), ip("10.0.0.1"));
    }

    #[test]
    fn private_peer_trusts_one_hop_only() {
        let setting = TrustProxy::PrivatePeer;
        assert_eq!(client_ip(&setting, ip("172.18.0.2"), &forwarded("6.6.6.6, 1.1.1.1")), ip("1.1.1.1"));
        assert_eq!(client_ip(&setting, ip("8.8.8.8"), &forwarded("1.1.1.1")), ip("8.8.8.8"));
        assert_eq!(client_ip(&setting, ip("172.18.0.2"), &forwarded("not an address")), ip("172.18.0.2"));
    }

    #[test]
    fn keys_treat_one_address_one_way() {
        assert_eq!(address_rate_key(ip("1.2.3.4")), "ipv4:1.2.3.4");
        assert_eq!(address_rate_key(ip("::ffff:1.2.3.4")), "ipv4:1.2.3.4");
        assert_eq!(address_rate_key(ip("2001:db8:1:2:3:4:5:6")), "ipv6:20010db800010002/64");
        assert_eq!(address_rate_key(ip("2001:db8:1:2:ffff::")), "ipv6:20010db800010002/64");
        assert_eq!(address_rate_key(None), "unknown");
    }
}
