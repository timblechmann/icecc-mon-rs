// SPDX-License-Identifier: GPL-2.0-only
pub mod codec;
pub mod connection;
pub mod discovery;
pub mod messages;

pub use connection::SchedulerConnection;
pub use discovery::discover_scheduler;
pub use messages::{Language, Message, MessageType};

/// icecc protocol version we support
pub const PROTOCOL_VERSION: u32 = 44;
/// Minimum protocol version we accept
pub const MIN_PROTOCOL_VERSION: u32 = 21;
/// Default scheduler port (UDP discovery + TCP)
pub const DEFAULT_PORT: u16 = 8765;
/// Default network name
pub const DEFAULT_NETNAME: &str = "ICECREAM";
/// UDP broadcast buffer length
pub const BROAD_BUFLEN: usize = 268;
/// Maximum message size (1 MB)
pub const MAX_MSG_SIZE: u32 = 1_048_576;
