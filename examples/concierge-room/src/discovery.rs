//! UDP discovery of an `aice-backend` instance using the same
//! `FIND` / `HERE:<port>` exchange as `aice-macos`.

use std::net::{IpAddr, Ipv4Addr, UdpSocket};
use std::time::{Duration, Instant};
use thiserror::Error;
use tracing::{debug, info, warn};

const DEFAULT_DISCOVERY_UDP_PORT: u16 = 9999;
const DEFAULT_DISCOVERY_TIMEOUT_MS: u64 = 3000;
const FIND_PAYLOAD: &[u8] = b"FIND";

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("udp bind failed: {0}")]
    Bind(std::io::Error),
    #[error("udp send_to failed for all targets")]
    NoSends,
    #[error("no HERE replies within {0:?}")]
    Timeout(Duration),
}

/// Configuration controlling the discovery loop.
#[derive(Clone, Debug)]
pub struct DiscoveryConfig {
    pub udp_port: u16,
    pub timeout: Duration,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            udp_port: env_port().unwrap_or(DEFAULT_DISCOVERY_UDP_PORT),
            timeout: env_timeout().unwrap_or(Duration::from_millis(DEFAULT_DISCOVERY_TIMEOUT_MS)),
        }
    }
}

fn env_port() -> Option<u16> {
    std::env::var("CONCIERGE_DISCOVERY_UDP_PORT")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .or_else(|| {
            std::env::var("AICE_BACKEND_DISCOVERY_UDP_PORT")
                .ok()
                .and_then(|v| v.trim().parse().ok())
        })
}

fn env_timeout() -> Option<Duration> {
    std::env::var("CONCIERGE_DISCOVERY_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v > 0)
        .map(Duration::from_millis)
}

/// A discovered backend, expressed as the HTTP base URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredBackend {
    pub http_base_url: String,
    pub ws_base_url: String,
}

/// Discover one backend by broadcasting `FIND` and waiting for the first
/// `HERE:<port>` reply.
pub async fn discover_one(config: DiscoveryConfig) -> Result<DiscoveredBackend, DiscoveryError> {
    tokio::task::spawn_blocking(move || discover_one_blocking(config))
        .await
        .map_err(|error| DiscoveryError::Bind(std::io::Error::other(error.to_string())))?
}

fn discover_one_blocking(config: DiscoveryConfig) -> Result<DiscoveredBackend, DiscoveryError> {
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(DiscoveryError::Bind)?;
    socket.set_broadcast(true).map_err(DiscoveryError::Bind)?;

    let targets = build_targets(config.udp_port);
    let mut any_sent = false;
    for target in &targets {
        match socket.send_to(FIND_PAYLOAD, target) {
            Ok(_) => any_sent = true,
            Err(error) => warn!(%error, %target, "send FIND failed"),
        }
    }
    if !any_sent {
        return Err(DiscoveryError::NoSends);
    }
    debug!(?targets, "FIND probes sent");

    socket
        .set_read_timeout(Some(Duration::from_millis(50)))
        .map_err(DiscoveryError::Bind)?;

    let deadline = Instant::now() + config.timeout;
    let mut buf = [0u8; 128];
    while Instant::now() < deadline {
        match socket.recv_from(&mut buf) {
            Ok((n, src)) => {
                if let Some(port) = parse_here_port(&buf[..n]) {
                    let http = format_url(&src.ip().to_string(), port, "http");
                    let ws = format_url(&src.ip().to_string(), port, "ws");
                    info!(%src, http, "backend discovered");
                    return Ok(DiscoveredBackend {
                        http_base_url: http,
                        ws_base_url: ws,
                    });
                }
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut => {}
            Err(error) => {
                warn!(%error, "recv_from failed");
                break;
            }
        }
    }
    Err(DiscoveryError::Timeout(config.timeout))
}

fn build_targets(port: u16) -> Vec<String> {
    let mut targets = Vec::with_capacity(3);
    targets.push(format!("127.0.0.1:{port}"));
    if let Some(local) = resolve_local_ipv4() {
        if let Some(bc) = subnet_broadcast(local) {
            targets.push(format!("{bc}:{port}"));
        }
    }
    targets.push(format!("255.255.255.255:{port}"));
    targets
}

fn resolve_local_ipv4() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("1.1.1.1:80").ok()?;
    let IpAddr::V4(v4) = socket.local_addr().ok()?.ip() else {
        return None;
    };
    if v4.is_loopback() || v4.is_unspecified() || v4.is_link_local() {
        return None;
    }
    Some(v4)
}

fn subnet_broadcast(local: Ipv4Addr) -> Option<Ipv4Addr> {
    if local.is_loopback() || local.is_unspecified() || local.is_multicast() {
        return None;
    }
    let o = local.octets();
    Some(Ipv4Addr::new(o[0], o[1], o[2], 255))
}

pub fn parse_here_port(payload: &[u8]) -> Option<u16> {
    let text = std::str::from_utf8(payload).ok()?.trim();
    let rest = text.strip_prefix("HERE:")?;
    let token = rest.split_whitespace().next()?;
    token.parse().ok()
}

fn format_url(host: &str, port: u16, scheme: &str) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("{scheme}://[{host}]:{port}")
    } else {
        format!("{scheme}://{host}:{port}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_here_port() {
        assert_eq!(parse_here_port(b"HERE:8781"), Some(8781));
        assert_eq!(parse_here_port(b"  HERE:9999\n"), Some(9999));
        assert_eq!(parse_here_port(b"HERE:not_a_port"), None);
        assert_eq!(parse_here_port(b"FIND"), None);
    }

    #[test]
    fn assumes_slash_24_for_subnet_broadcast() {
        assert_eq!(
            subnet_broadcast(Ipv4Addr::new(192, 168, 2, 136)),
            Some(Ipv4Addr::new(192, 168, 2, 255))
        );
        assert_eq!(subnet_broadcast(Ipv4Addr::LOCALHOST), None);
    }

    #[test]
    fn formats_urls() {
        assert_eq!(
            format_url("127.0.0.1", 8781, "http"),
            "http://127.0.0.1:8781"
        );
        assert_eq!(format_url("127.0.0.1", 8781, "ws"), "ws://127.0.0.1:8781");
    }
}
