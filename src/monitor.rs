// SPDX-License-Identifier: GPL-2.0-only
//! Async monitor task: connects to scheduler, receives messages, sends events.

use crate::model::MonitorEvent;
use icecc_protocol::connection::SchedulerConnection;
use icecc_protocol::connection::TcpSchedulerConnection;
use icecc_protocol::messages::Message;
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::sync::mpsc;
use tokio::time::{Duration, sleep};

/// Parse statmsg "Key:Value\n..." into HashMap.
fn parse_stats(statmsg: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in statmsg.lines() {
        if let Some((k, v)) = line.split_once(':') {
            map.insert(k.to_string(), v.to_string());
        }
    }
    map
}

/// Run the monitor loop. Reconnects on failure.
pub async fn run_monitor(
    scheduler_addr: Option<SocketAddr>,
    netname: &str,
    tx: mpsc::UnboundedSender<MonitorEvent>,
) {
    loop {
        let result = connect_and_monitor(scheduler_addr, netname, &tx).await;
        if let Err(e) = result {
            log::error!("monitor error: {}", e);
        }
        let _ = tx.send(MonitorEvent::Disconnected);
        log::info!("reconnecting in 5s...");
        sleep(Duration::from_secs(5)).await;
    }
}

async fn connect_and_monitor(
    scheduler_addr: Option<SocketAddr>,
    netname: &str,
    tx: &mpsc::UnboundedSender<MonitorEvent>,
) -> anyhow::Result<()> {
    // Discover scheduler
    let info = icecc_protocol::discover_scheduler(netname, scheduler_addr, 5).await?;

    log::info!("connecting to scheduler at {}", info.address);

    let mut conn = TcpSchedulerConnection::connect(info.address).await?;

    tx.send(MonitorEvent::Connected {
        scheduler_name: info.address.to_string(),
        netname: info.netname.clone(),
    })?;

    // Message receive loop
    loop {
        let msg = conn.recv_message().await?;
        match msg {
            Message::MonStats { host_id, statmsg } => {
                let attrs = parse_stats(&statmsg);
                tx.send(MonitorEvent::HostStats { host_id, attrs })?;
            }
            Message::MonGetCs {
                filename,
                job_id,
                client_id,
                ..
            } => {
                tx.send(MonitorEvent::JobPending {
                    job_id,
                    client_id,
                    filename,
                })?;
            }
            Message::MonJobBegin {
                job_id, host_id, ..
            } => {
                tx.send(MonitorEvent::JobBegin { job_id, host_id })?;
            }
            Message::MonJobDone { job_id, .. } => {
                tx.send(MonitorEvent::JobDone { job_id })?;
            }
            Message::MonLocalJobBegin {
                host_id,
                job_id,
                file,
                ..
            } => {
                tx.send(MonitorEvent::LocalJobBegin {
                    job_id,
                    host_id,
                    filename: file,
                })?;
            }
            Message::JobLocalDone { job_id } => {
                tx.send(MonitorEvent::JobDone { job_id })?;
            }
            Message::End => {
                log::info!("received End message");
                return Ok(());
            }
            Message::Ping => {
                // Ignore keepalive
            }
            _ => {
                log::debug!("ignoring message: {:?}", msg);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_stats() {
        let s = "Name:myhost\nMaxJobs:4\nSpeed:100\nIP:10.0.0.1\n";
        let m = parse_stats(s);
        assert_eq!(m["Name"], "myhost");
        assert_eq!(m["MaxJobs"], "4");
        assert_eq!(m["Speed"], "100");
        assert_eq!(m["IP"], "10.0.0.1");
    }

    #[test]
    fn test_parse_stats_empty() {
        let m = parse_stats("");
        assert!(m.is_empty());
    }
}
