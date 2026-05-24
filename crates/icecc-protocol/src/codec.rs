// SPDX-License-Identifier: GPL-2.0-only
//! Binary codec for icecc wire format primitives.
//!
//! All integers are big-endian (network byte order).
//! Strings are length-prefixed (u32 len including NUL terminator), followed by bytes + NUL.

use anyhow::{Result, bail};
use std::io::Cursor;

/// Write a u32 in big-endian to a buffer.
pub fn encode_u32(buf: &mut Vec<u8>, val: u32) {
    buf.extend_from_slice(&val.to_be_bytes());
}

/// Read a u32 in big-endian from a cursor.
pub fn decode_u32(cursor: &mut Cursor<&[u8]>) -> Result<u32> {
    let pos = cursor.position() as usize;
    let data = cursor.get_ref();
    if pos + 4 > data.len() {
        bail!("unexpected EOF reading u32");
    }
    let val = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
    cursor.set_position((pos + 4) as u64);
    Ok(val)
}

/// Write a length-prefixed string (len includes NUL terminator).
pub fn encode_string(buf: &mut Vec<u8>, s: &str) {
    let len = (s.len() + 1) as u32; // +1 for NUL
    encode_u32(buf, len);
    buf.extend_from_slice(s.as_bytes());
    buf.push(0); // NUL terminator
}

/// Read a length-prefixed string.
pub fn decode_string(cursor: &mut Cursor<&[u8]>) -> Result<String> {
    let len = decode_u32(cursor)? as usize;
    if len == 0 {
        return Ok(String::new());
    }
    let pos = cursor.position() as usize;
    let data = cursor.get_ref();
    if pos + len > data.len() {
        bail!("unexpected EOF reading string of len {}", len);
    }
    let s = std::str::from_utf8(&data[pos..pos + len - 1])?.to_string();
    cursor.set_position((pos + len) as u64);
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_u32() {
        let mut buf = Vec::new();
        encode_u32(&mut buf, 0x12345678);
        let mut cursor = Cursor::new(buf.as_slice());
        assert_eq!(decode_u32(&mut cursor).unwrap(), 0x12345678);
    }

    #[test]
    fn roundtrip_string() {
        let mut buf = Vec::new();
        encode_string(&mut buf, "hello");
        let mut cursor = Cursor::new(buf.as_slice());
        assert_eq!(decode_string(&mut cursor).unwrap(), "hello");
    }

    #[test]
    fn roundtrip_empty_string() {
        let mut buf = Vec::new();
        encode_string(&mut buf, "");
        let mut cursor = Cursor::new(buf.as_slice());
        assert_eq!(decode_string(&mut cursor).unwrap(), "");
    }

    #[test]
    fn string_wire_format() {
        let mut buf = Vec::new();
        encode_string(&mut buf, "hi");
        assert_eq!(buf, vec![0, 0, 0, 3, b'h', b'i', 0]);
    }
}
