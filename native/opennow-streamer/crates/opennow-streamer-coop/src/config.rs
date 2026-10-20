//! Parsing of the ICE server and relay configuration handed over by the shell.
//!
//! Relay hosts are configuration from the social backend; nothing here names a site.

use std::fmt;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};

use serde::Deserialize;
use serde_json::Value;

pub const MAX_ICE_SERVERS: usize = 8;
pub const MAX_URLS_PER_SERVER: usize = 6;
pub const MAX_RELAYS: usize = 4;
const MAX_TEXT_LEN: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TurnTransport {
    Udp,
    Tcp,
    Tls,
}

impl TurnTransport {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Udp => "udp",
            Self::Tcp => "tcp",
            Self::Tls => "tls",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StunServer {
    pub host: String,
    pub port: u16,
}

#[derive(Clone, PartialEq, Eq)]
pub struct TurnServer {
    pub host: String,
    pub port: u16,
    pub transport: TurnTransport,
    pub username: String,
    pub credential: String,
}

impl fmt::Debug for TurnServer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TurnServer")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("transport", &self.transport)
            .field("username", &"[redacted]")
            .field("credential", &"[redacted]")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelaySite {
    pub id: String,
    pub name: String,
    pub hosts: Vec<String>,
}

/// Validated ICE configuration for one co-op link.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IceConfig {
    pub stun: Vec<StunServer>,
    pub turn: Vec<TurnServer>,
    pub relays: Vec<RelaySite>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawIceServer {
    urls: Vec<String>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    credential: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRelay {
    id: String,
    name: String,
    #[serde(default)]
    stun_url: Option<String>,
    #[serde(default)]
    turn_urls: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError(pub String);

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

fn bounded_text(value: &str, field: &str) -> Result<(), ConfigError> {
    if value.is_empty() || value.len() > MAX_TEXT_LEN || value.contains('\0') {
        return Err(ConfigError(format!(
            "{field} must be 1 to {MAX_TEXT_LEN} bytes"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum IceUrl {
    Stun(StunServer),
    Turn {
        host: String,
        port: u16,
        transport: TurnTransport,
    },
}

/// Parses an RFC 7064 / RFC 7065 URL: stun:host[:port], turn:host[:port][?transport=udp|tcp],
/// turns:host[:port][?transport=tcp]. IPv6 literals use brackets.
fn parse_url(url: &str) -> Result<IceUrl, ConfigError> {
    bounded_text(url, "ICE server URL")?;
    let (scheme, rest) = url
        .split_once(':')
        .ok_or_else(|| ConfigError(format!("ICE server URL has no scheme: {url}")))?;
    let (authority, query) = match rest.split_once('?') {
        Some((authority, query)) => (authority, Some(query)),
        None => (rest, None),
    };
    let (host, port) = if let Some(stripped) = authority.strip_prefix('[') {
        let (host, tail) = stripped
            .split_once(']')
            .ok_or_else(|| ConfigError(format!("unterminated IPv6 literal in {url}")))?;
        let port = match tail.strip_prefix(':') {
            Some(port) => Some(port),
            None if tail.is_empty() => None,
            None => return Err(ConfigError(format!("invalid ICE authority in {url}"))),
        };
        (host, port)
    } else {
        match authority.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        }
    };
    if host.is_empty() {
        return Err(ConfigError(format!("ICE server URL has no host: {url}")));
    }
    let parse_port = |default: u16| -> Result<u16, ConfigError> {
        match port {
            None => Ok(default),
            Some(port) => port
                .parse::<u16>()
                .ok()
                .filter(|port| *port != 0)
                .ok_or_else(|| ConfigError(format!("invalid port in {url}"))),
        }
    };
    let transport_param = query
        .into_iter()
        .flat_map(|query| query.split('&'))
        .find_map(|pair| pair.strip_prefix("transport="));
    match scheme.to_ascii_lowercase().as_str() {
        "stun" => Ok(IceUrl::Stun(StunServer {
            host: host.to_owned(),
            port: parse_port(3478)?,
        })),
        "turn" => {
            let transport = match transport_param.map(str::to_ascii_lowercase).as_deref() {
                None | Some("udp") => TurnTransport::Udp,
                Some("tcp") => TurnTransport::Tcp,
                Some(other) => {
                    return Err(ConfigError(format!("unsupported TURN transport {other}")));
                }
            };
            Ok(IceUrl::Turn {
                host: host.to_owned(),
                port: parse_port(3478)?,
                transport,
            })
        }
        "turns" => match transport_param.map(str::to_ascii_lowercase).as_deref() {
            None | Some("tcp") => Ok(IceUrl::Turn {
                host: host.to_owned(),
                port: parse_port(5349)?,
                transport: TurnTransport::Tls,
            }),
            Some(other) => Err(ConfigError(format!("unsupported TURNS transport {other}"))),
        },
        other => Err(ConfigError(format!("unsupported ICE URL scheme {other}"))),
    }
}

fn url_host(url: &str) -> Option<String> {
    match parse_url(url).ok()? {
        IceUrl::Stun(server) => Some(server.host),
        IceUrl::Turn { host, .. } => Some(host),
    }
}

impl IceConfig {
    /// Parses the contract's 'iceServers' and 'relays' arrays.
    pub fn from_json(ice_servers: &Value, relays: Option<&Value>) -> Result<Self, ConfigError> {
        let servers: Vec<RawIceServer> = serde_json::from_value(ice_servers.clone())
            .map_err(|error| ConfigError(format!("iceServers is invalid: {error}")))?;
        if servers.len() > MAX_ICE_SERVERS {
            return Err(ConfigError(format!(
                "iceServers allows at most {MAX_ICE_SERVERS} entries"
            )));
        }
        let mut config = Self::default();
        for server in servers {
            if server.urls.is_empty() || server.urls.len() > MAX_URLS_PER_SERVER {
                return Err(ConfigError(format!(
                    "each ICE server needs 1 to {MAX_URLS_PER_SERVER} urls"
                )));
            }
            for url in &server.urls {
                match parse_url(url)? {
                    IceUrl::Stun(stun) => {
                        if !config.stun.contains(&stun) {
                            config.stun.push(stun);
                        }
                    }
                    IceUrl::Turn {
                        host,
                        port,
                        transport,
                    } => {
                        let (Some(username), Some(credential)) =
                            (server.username.as_ref(), server.credential.as_ref())
                        else {
                            return Err(ConfigError(
                                "TURN servers require a username and credential".to_owned(),
                            ));
                        };
                        bounded_text(username, "TURN username")?;
                        bounded_text(credential, "TURN credential")?;
                        config.turn.push(TurnServer {
                            host,
                            port,
                            transport,
                            username: username.clone(),
                            credential: credential.clone(),
                        });
                    }
                }
            }
        }
        if let Some(relays) = relays.filter(|value| !value.is_null()) {
            let relays: Vec<RawRelay> = serde_json::from_value(relays.clone())
                .map_err(|error| ConfigError(format!("relays is invalid: {error}")))?;
            if relays.len() > MAX_RELAYS {
                return Err(ConfigError(format!(
                    "relays allows at most {MAX_RELAYS} entries"
                )));
            }
            for relay in relays {
                bounded_text(&relay.id, "relay id")?;
                bounded_text(&relay.name, "relay name")?;
                if relay.turn_urls.len() > MAX_URLS_PER_SERVER {
                    return Err(ConfigError("relay lists too many TURN urls".to_owned()));
                }
                let mut hosts = Vec::new();
                for url in relay.stun_url.iter().chain(relay.turn_urls.iter()) {
                    let host = url_host(url)
                        .ok_or_else(|| ConfigError(format!("relay URL is invalid: {url}")))?;
                    if !hosts.contains(&host) {
                        hosts.push(host);
                    }
                }
                config.relays.push(RelaySite {
                    id: relay.id,
                    name: relay.name,
                    hosts,
                });
            }
        }
        Ok(config)
    }

    /// The relay site whose configured host is 'host'.
    pub fn relay_for_host(&self, host: &str) -> Option<&RelaySite> {
        self.relays.iter().find(|relay| {
            relay
                .hosts
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(host))
        })
    }

    /// Resolves every relay host once so a remote relayed candidate can be attributed to a site.
    pub fn resolve_relay_addresses(&self) -> Vec<(IpAddr, String)> {
        let mut resolved = Vec::new();
        for relay in &self.relays {
            for host in &relay.hosts {
                for addr in resolve(host, 3478) {
                    if !resolved.iter().any(|(ip, _)| *ip == addr.ip()) {
                        resolved.push((addr.ip(), relay.id.clone()));
                    }
                }
            }
        }
        resolved
    }
}

pub(crate) fn resolve(host: &str, port: u16) -> Vec<SocketAddr> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return vec![SocketAddr::new(ip, port)];
    }
    (host, port)
        .to_socket_addrs()
        .map(|addrs| addrs.take(8).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_contract_ice_servers_and_relay_sites() {
        let config = IceConfig::from_json(
            &json!([
                {"urls":["stun:nl.example.test:3478"]},
                {"urls":["turn:nl.example.test:3478?transport=udp","turn:nl.example.test:3478?transport=tcp","turns:nl.example.test:5349?transport=tcp"],
                 "username":"1700000000:usr_1","credential":"c2VjcmV0"}
            ]),
            Some(&json!([{"id":"nl","name":"Netherlands","stunUrl":"stun:nl.example.test:3478",
                "turnUrls":["turn:nl.example.test:3478?transport=udp"]}])),
        )
        .unwrap();
        assert_eq!(
            config.stun,
            [StunServer {
                host: "nl.example.test".into(),
                port: 3478
            }]
        );
        let transports: Vec<_> = config
            .turn
            .iter()
            .map(|turn| (turn.transport, turn.port))
            .collect();
        assert_eq!(
            transports,
            [
                (TurnTransport::Udp, 3478),
                (TurnTransport::Tcp, 3478),
                (TurnTransport::Tls, 5349)
            ]
        );
        assert_eq!(
            config.relay_for_host("NL.example.test").unwrap().name,
            "Netherlands"
        );
        assert!(!format!("{:?}", config.turn[0]).contains("c2VjcmV0"));
    }

    #[test]
    fn rejects_unbounded_or_malformed_configuration() {
        assert!(IceConfig::from_json(&json!([{"urls":["turn:host"]}]), None).is_err());
        assert!(IceConfig::from_json(&json!([{"urls":["http://host"]}]), None).is_err());
        assert!(IceConfig::from_json(&json!([{"urls":["stun:host:0"]}]), None).is_err());
        let many: Vec<_> = (0..9).map(|_| json!({"urls":["stun:h"]})).collect();
        assert!(IceConfig::from_json(&json!(many), None).is_err());
        let ipv6 = IceConfig::from_json(&json!([{"urls":["stun:[::1]:3479"]}]), None).unwrap();
        assert_eq!(
            ipv6.stun[0],
            StunServer {
                host: "::1".into(),
                port: 3479
            }
        );
    }
}
