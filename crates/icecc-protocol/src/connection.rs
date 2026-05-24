// SPDX-License-Identifier: GPL-2.0-only
//! TCP connection to icecc scheduler with monitor protocol.

use crate::messages::Message;
use crate::{MAX_MSG_SIZE, PROTOCOL_VERSION};
use anyhow::{Result, bail};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Trait for receiving messages from a scheduler.
/// Enables mocking in tests.
#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait SchedulerConnection: Send {
    /// Receive the next message from the scheduler.
    async fn recv_message(&mut self) -> Result<Message>;
}

/// Real TCP connection to an icecc scheduler.
pub struct TcpSchedulerConnection {
    stream: TcpStream,
    protocol: u32,
}

impl TcpSchedulerConnection {
    /// Connect to scheduler, perform 2-round handshake, send MonLoginMsg.
    pub async fn connect(addr: std::net::SocketAddr) -> Result<Self> {
        let mut stream = TcpStream::connect(addr).await?;

        let mut buf = [0u8; 4];

        // Round 1: send our version, read remote version
        stream.write_all(&PROTOCOL_VERSION.to_le_bytes()).await?;
        stream.read_exact(&mut buf).await?;
        let remote_version = u32::from_le_bytes(buf);

        // Compute agreed version
        let agreed = remote_version.min(PROTOCOL_VERSION);

        if agreed < crate::MIN_PROTOCOL_VERSION {
            bail!("negotiated protocol version {} too old", agreed);
        }

        // Round 2: write agreed version, read remote confirmation
        stream.write_all(&agreed.to_le_bytes()).await?;
        stream.read_exact(&mut buf).await?;
        let remote_agreed = u32::from_le_bytes(buf);

        if remote_agreed != agreed {
            bail!(
                "protocol version mismatch: us={} remote={}",
                agreed,
                remote_agreed
            );
        }

        log::info!("connected to scheduler, protocol version {}", agreed);

        let login = Message::MonLogin;
        let framed = login.encode_framed();
        stream.write_all(&framed).await?;

        Ok(Self {
            stream,
            protocol: agreed,
        })
    }

    /// Get negotiated protocol version.
    pub fn protocol_version(&self) -> u32 {
        self.protocol
    }
}

#[async_trait::async_trait]
impl SchedulerConnection for TcpSchedulerConnection {
    async fn recv_message(&mut self) -> Result<Message> {
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf);

        if len > MAX_MSG_SIZE {
            bail!("message too large: {} bytes", len);
        }

        let mut payload = vec![0u8; len as usize];
        self.stream.read_exact(&mut payload).await?;

        Message::decode(&payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_connect_and_recv() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4];

            // Round 1: read client version, send our version
            sock.read_exact(&mut buf).await.unwrap();
            let client_ver = u32::from_le_bytes(buf);
            assert_eq!(client_ver, PROTOCOL_VERSION);
            sock.write_all(&PROTOCOL_VERSION.to_le_bytes())
                .await
                .unwrap();

            // Round 2: read client agreed, send back agreed
            sock.read_exact(&mut buf).await.unwrap();
            let agreed = u32::from_le_bytes(buf);
            assert_eq!(agreed, PROTOCOL_VERSION);
            sock.write_all(&agreed.to_le_bytes()).await.unwrap();

            let mut len_buf = [0u8; 4];
            sock.read_exact(&mut len_buf).await.unwrap();
            let len = u32::from_be_bytes(len_buf);
            let mut payload = vec![0u8; len as usize];
            sock.read_exact(&mut payload).await.unwrap();
            let msg = Message::decode(&payload).unwrap();
            assert_eq!(msg, Message::MonLogin);

            let stats = Message::MonStats {
                host_id: 1,
                statmsg: "Name:testhost\nMaxJobs:4\n".into(),
            };
            let framed = stats.encode_framed();
            sock.write_all(&framed).await.unwrap();

            let end = Message::End.encode_framed();
            sock.write_all(&end).await.unwrap();
        });

        let mut conn = TcpSchedulerConnection::connect(addr).await.unwrap();
        assert_eq!(conn.protocol_version(), PROTOCOL_VERSION);

        let msg = conn.recv_message().await.unwrap();
        assert_eq!(
            msg,
            Message::MonStats {
                host_id: 1,
                statmsg: "Name:testhost\nMaxJobs:4\n".into()
            }
        );

        let msg = conn.recv_message().await.unwrap();
        assert_eq!(msg, Message::End);

        server.await.unwrap();
    }
}
