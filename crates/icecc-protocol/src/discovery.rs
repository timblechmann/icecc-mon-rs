// SPDX-License-Identifier: GPL-2.0-only
//! UDP scheduler discovery.

use crate::{BROAD_BUFLEN, DEFAULT_PORT, PROTOCOL_VERSION};
use anyhow::{Result, bail};
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio::time::{Duration, timeout};

/// Info about a discovered scheduler.
#[derive(Debug, Clone)]
pub struct SchedulerInfo {
    pub address: SocketAddr,
    pub netname: String,
    pub protocol_version: u32,
}

/// Discover an icecc scheduler via UDP broadcast.
///
/// If `scheduler_addr` is Some, skip discovery and return that address directly.
/// Otherwise broadcast on port 8765 and wait for a reply.
pub async fn discover_scheduler(
    netname: &str,
    scheduler_addr: Option<SocketAddr>,
    timeout_secs: u64,
) -> Result<SchedulerInfo> {
    if let Some(addr) = scheduler_addr {
        return Ok(SchedulerInfo {
            address: addr,
            netname: netname.to_string(),
            protocol_version: PROTOCOL_VERSION,
        });
    }

    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.set_broadcast(true)?;

    let broadcast_addr: SocketAddr = format!("255.255.255.255:{}", DEFAULT_PORT).parse()?;
    let discovery_packet = [PROTOCOL_VERSION as u8];
    socket.send_to(&discovery_packet, broadcast_addr).await?;

    let mut buf = [0u8; BROAD_BUFLEN];
    let (len, src) = timeout(
        Duration::from_secs(timeout_secs),
        socket.recv_from(&mut buf),
    )
    .await
    .map_err(|_| anyhow::anyhow!("scheduler discovery timed out"))??;

    if len < 13 {
        bail!("discovery response too short: {} bytes", len);
    }

    let version_byte = buf[0];
    // For protocol >= 38, first byte = version + 3
    let remote_version = if version_byte >= 3 {
        let v = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]);
        if v < crate::MIN_PROTOCOL_VERSION {
            bail!("scheduler protocol version {} too old", v);
        }
        v
    } else {
        bail!("scheduler protocol too old (pre-v38)");
    };

    let netname_end = buf[13..].iter().position(|&b| b == 0).unwrap_or(255);
    let discovered_netname = std::str::from_utf8(&buf[13..13 + netname_end])
        .unwrap_or("")
        .to_string();

    if !netname.is_empty() && !discovered_netname.is_empty() && netname != discovered_netname {
        bail!(
            "scheduler netname '{}' doesn't match requested '{}'",
            discovered_netname,
            netname
        );
    }

    let scheduler_addr = SocketAddr::new(src.ip(), DEFAULT_PORT);

    Ok(SchedulerInfo {
        address: scheduler_addr,
        netname: if discovered_netname.is_empty() {
            netname.to_string()
        } else {
            discovered_netname
        },
        protocol_version: remote_version,
    })
}
