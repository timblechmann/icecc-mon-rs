// SPDX-License-Identifier: GPL-2.0-only
//! icecc monitor protocol message types and serialization.

use crate::codec::*;
use anyhow::{Result, bail};
use std::io::Cursor;

/// Message type IDs (ASCII-based enum starting at 'A').
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MessageType {
    Unknown = 0x41,
    Ping = 0x42,
    End = 0x43,
    GetCs = 0x47,
    JobBegin = 0x4C,
    JobDone = 0x4D,
    JobLocalDone = 0x4F,
    MonLogin = 0x52,
    MonGetCs = 0x53,
    MonJobBegin = 0x54,
    MonJobDone = 0x55,
    MonLocalJobBegin = 0x56,
    MonStats = 0x57,
}

impl MessageType {
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            0x41 => Some(Self::Unknown),
            0x42 => Some(Self::Ping),
            0x43 => Some(Self::End),
            0x47 => Some(Self::GetCs),
            0x4C => Some(Self::JobBegin),
            0x4D => Some(Self::JobDone),
            0x4F => Some(Self::JobLocalDone),
            0x52 => Some(Self::MonLogin),
            0x53 => Some(Self::MonGetCs),
            0x54 => Some(Self::MonJobBegin),
            0x55 => Some(Self::MonJobDone),
            0x56 => Some(Self::MonLocalJobBegin),
            0x57 => Some(Self::MonStats),
            _ => None,
        }
    }
}

/// Compile job language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Language {
    C = 0,
    Cxx = 1,
    ObjC = 2,
    ObjCxx = 3,
    Custom = 4,
}

impl Language {
    pub fn from_u32(v: u32) -> Self {
        match v {
            0 => Self::C,
            1 => Self::Cxx,
            2 => Self::ObjC,
            3 => Self::ObjCxx,
            _ => Self::Custom,
        }
    }
}

/// All monitor-relevant messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// Empty login message sent by monitor to scheduler.
    MonLogin,
    /// A client requested a compile server.
    MonGetCs {
        filename: String,
        lang: Language,
        job_id: u32,
        client_id: u32,
    },
    /// Remote compile started.
    MonJobBegin {
        job_id: u32,
        start_time: u32,
        host_id: u32,
    },
    /// Remote compile finished.
    MonJobDone {
        job_id: u32,
        exitcode: u32,
        real_msec: u32,
        user_msec: u32,
        sys_msec: u32,
        pfaults: u32,
        in_compressed: u32,
        in_uncompressed: u32,
        out_compressed: u32,
        out_uncompressed: u32,
        flags: u32,
    },
    /// Local compile started.
    MonLocalJobBegin {
        host_id: u32,
        job_id: u32,
        start_time: u32,
        file: String,
    },
    /// Local compile finished.
    JobLocalDone { job_id: u32 },
    /// Host stats update.
    MonStats { host_id: u32, statmsg: String },
    /// Ping (keepalive).
    Ping,
    /// Connection end.
    End,
}

impl Message {
    /// Encode message to wire format (payload only, without length prefix).
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        match self {
            Message::MonLogin => {
                encode_u32(&mut buf, MessageType::MonLogin as u32);
            }
            Message::MonGetCs {
                filename,
                lang,
                job_id,
                client_id,
            } => {
                encode_u32(&mut buf, MessageType::MonGetCs as u32);
                encode_string(&mut buf, filename);
                encode_u32(&mut buf, *lang as u32);
                encode_u32(&mut buf, *job_id);
                encode_u32(&mut buf, *client_id);
            }
            Message::MonJobBegin {
                job_id,
                start_time,
                host_id,
            } => {
                encode_u32(&mut buf, MessageType::MonJobBegin as u32);
                encode_u32(&mut buf, *job_id);
                encode_u32(&mut buf, *start_time);
                encode_u32(&mut buf, *host_id);
            }
            Message::MonJobDone {
                job_id,
                exitcode,
                real_msec,
                user_msec,
                sys_msec,
                pfaults,
                in_compressed,
                in_uncompressed,
                out_compressed,
                out_uncompressed,
                flags,
            } => {
                encode_u32(&mut buf, MessageType::MonJobDone as u32);
                encode_u32(&mut buf, *job_id);
                encode_u32(&mut buf, *exitcode);
                encode_u32(&mut buf, *real_msec);
                encode_u32(&mut buf, *user_msec);
                encode_u32(&mut buf, *sys_msec);
                encode_u32(&mut buf, *pfaults);
                encode_u32(&mut buf, *in_compressed);
                encode_u32(&mut buf, *in_uncompressed);
                encode_u32(&mut buf, *out_compressed);
                encode_u32(&mut buf, *out_uncompressed);
                encode_u32(&mut buf, *flags);
            }
            Message::MonLocalJobBegin {
                host_id,
                job_id,
                start_time,
                file,
            } => {
                encode_u32(&mut buf, MessageType::MonLocalJobBegin as u32);
                encode_u32(&mut buf, *host_id);
                encode_u32(&mut buf, *job_id);
                encode_u32(&mut buf, *start_time);
                encode_string(&mut buf, file);
            }
            Message::MonStats { host_id, statmsg } => {
                encode_u32(&mut buf, MessageType::MonStats as u32);
                encode_u32(&mut buf, *host_id);
                encode_string(&mut buf, statmsg);
            }
            Message::JobLocalDone { job_id } => {
                encode_u32(&mut buf, MessageType::JobLocalDone as u32);
                encode_u32(&mut buf, *job_id);
            }
            Message::Ping => {
                encode_u32(&mut buf, MessageType::Ping as u32);
            }
            Message::End => {
                encode_u32(&mut buf, MessageType::End as u32);
            }
        }
        buf
    }

    /// Decode message from payload bytes (without length prefix).
    pub fn decode(data: &[u8]) -> Result<Self> {
        let mut cursor = Cursor::new(data);
        let type_id = decode_u32(&mut cursor)?;
        let msg_type = MessageType::from_u32(type_id);

        match msg_type {
            Some(MessageType::MonLogin) => Ok(Message::MonLogin),
            Some(MessageType::MonGetCs) => {
                let filename = decode_string(&mut cursor)?;
                let lang = Language::from_u32(decode_u32(&mut cursor)?);
                let job_id = decode_u32(&mut cursor)?;
                let client_id = decode_u32(&mut cursor)?;
                Ok(Message::MonGetCs {
                    filename,
                    lang,
                    job_id,
                    client_id,
                })
            }
            Some(MessageType::MonJobBegin) => {
                let job_id = decode_u32(&mut cursor)?;
                let start_time = decode_u32(&mut cursor)?;
                let host_id = decode_u32(&mut cursor)?;
                Ok(Message::MonJobBegin {
                    job_id,
                    start_time,
                    host_id,
                })
            }
            Some(MessageType::MonJobDone) => {
                let job_id = decode_u32(&mut cursor)?;
                let exitcode = decode_u32(&mut cursor)?;
                let real_msec = decode_u32(&mut cursor)?;
                let user_msec = decode_u32(&mut cursor)?;
                let sys_msec = decode_u32(&mut cursor)?;
                let pfaults = decode_u32(&mut cursor)?;
                let in_compressed = decode_u32(&mut cursor)?;
                let in_uncompressed = decode_u32(&mut cursor)?;
                let out_compressed = decode_u32(&mut cursor)?;
                let out_uncompressed = decode_u32(&mut cursor)?;
                let flags = decode_u32(&mut cursor)?;
                Ok(Message::MonJobDone {
                    job_id,
                    exitcode,
                    real_msec,
                    user_msec,
                    sys_msec,
                    pfaults,
                    in_compressed,
                    in_uncompressed,
                    out_compressed,
                    out_uncompressed,
                    flags,
                })
            }
            Some(MessageType::MonLocalJobBegin) => {
                let host_id = decode_u32(&mut cursor)?;
                let job_id = decode_u32(&mut cursor)?;
                let start_time = decode_u32(&mut cursor)?;
                let file = decode_string(&mut cursor)?;
                Ok(Message::MonLocalJobBegin {
                    host_id,
                    job_id,
                    start_time,
                    file,
                })
            }
            Some(MessageType::MonStats) => {
                let host_id = decode_u32(&mut cursor)?;
                let statmsg = decode_string(&mut cursor)?;
                Ok(Message::MonStats { host_id, statmsg })
            }
            Some(MessageType::JobLocalDone) => {
                let job_id = decode_u32(&mut cursor)?;
                Ok(Message::JobLocalDone { job_id })
            }
            Some(MessageType::Ping) => Ok(Message::Ping),
            Some(MessageType::End) => Ok(Message::End),
            _ => bail!("unknown or unsupported message type: 0x{:02X}", type_id),
        }
    }

    /// Encode a full framed message (length prefix + payload).
    pub fn encode_framed(&self) -> Vec<u8> {
        let payload = self.encode();
        let mut framed = Vec::with_capacity(4 + payload.len());
        encode_u32(&mut framed, payload.len() as u32);
        framed.extend_from_slice(&payload);
        framed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_mon_login() {
        let msg = Message::MonLogin;
        let encoded = msg.encode();
        let decoded = Message::decode(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_mon_get_cs() {
        let msg = Message::MonGetCs {
            filename: "test.cpp".into(),
            lang: Language::Cxx,
            job_id: 42,
            client_id: 7,
        };
        let decoded = Message::decode(&msg.encode()).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_mon_job_begin() {
        let msg = Message::MonJobBegin {
            job_id: 1,
            start_time: 1000,
            host_id: 5,
        };
        let decoded = Message::decode(&msg.encode()).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_mon_job_done() {
        let msg = Message::MonJobDone {
            job_id: 1,
            exitcode: 0,
            real_msec: 100,
            user_msec: 80,
            sys_msec: 10,
            pfaults: 0,
            in_compressed: 500,
            in_uncompressed: 1000,
            out_compressed: 200,
            out_uncompressed: 400,
            flags: 0,
        };
        let decoded = Message::decode(&msg.encode()).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_mon_local_job_begin() {
        let msg = Message::MonLocalJobBegin {
            host_id: 3,
            job_id: 10,
            start_time: 2000,
            file: "main.c".into(),
        };
        let decoded = Message::decode(&msg.encode()).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_mon_stats() {
        let msg = Message::MonStats {
            host_id: 1,
            statmsg: "Name:myhost\nMaxJobs:4\nSpeed:100\n".into(),
        };
        let decoded = Message::decode(&msg.encode()).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_ping() {
        let decoded = Message::decode(&Message::Ping.encode()).unwrap();
        assert_eq!(Message::Ping, decoded);
    }

    #[test]
    fn roundtrip_end() {
        let decoded = Message::decode(&Message::End.encode()).unwrap();
        assert_eq!(Message::End, decoded);
    }

    #[test]
    fn roundtrip_job_local_done() {
        let msg = Message::JobLocalDone { job_id: 42 };
        let decoded = Message::decode(&msg.encode()).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn framed_encoding() {
        let msg = Message::Ping;
        let framed = msg.encode_framed();
        // 4 bytes length + 4 bytes type
        assert_eq!(framed.len(), 8);
        // length prefix = 4 (just the type u32)
        assert_eq!(&framed[0..4], &[0, 0, 0, 4]);
    }

    #[test]
    fn unknown_type_errors() {
        let mut buf = Vec::new();
        encode_u32(&mut buf, 0xFF);
        assert!(Message::decode(&buf).is_err());
    }
}
