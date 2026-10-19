use std::io::Read;

use cvx_core::{Error, Result};

/// Maximum length of one pkt-line, including its four-byte length prefix.
pub const MAX_PKT_LINE_LEN: usize = 65_520;

/// A Git pkt-line packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PktLine {
    /// A data packet, without the length prefix.
    Data(Vec<u8>),
    /// Flush packet (`0000`).
    Flush,
    /// Delimiter packet (`0001`), used by protocol v2.
    Delim,
    /// Response-end packet (`0002`).
    ResponseEnd,
}

impl PktLine {
    /// Encodes the packet, including its length prefix.
    pub fn encode(&self) -> Result<Vec<u8>> {
        match self {
            Self::Data(data) => encode_data(data),
            Self::Flush => Ok(b"0000".to_vec()),
            Self::Delim => Ok(b"0001".to_vec()),
            Self::ResponseEnd => Ok(b"0002".to_vec()),
        }
    }

    /// Returns the data payload, if this is a data packet.
    pub fn data(&self) -> Option<&[u8]> {
        match self {
            Self::Data(data) => Some(data),
            _ => None,
        }
    }
}

/// Encodes one data packet.
pub fn encode_data(data: &[u8]) -> Result<Vec<u8>> {
    let length = data
        .len()
        .checked_add(4)
        .ok_or_else(|| Error::Message("pkt-line length overflow".to_owned()))?;
    if length > MAX_PKT_LINE_LEN {
        return Err(Error::Message(format!(
            "pkt-line is too long ({length} bytes, maximum {MAX_PKT_LINE_LEN})"
        )));
    }

    let mut output = Vec::with_capacity(length);
    output.extend_from_slice(format!("{length:04x}").as_bytes());
    output.extend_from_slice(data);
    Ok(output)
}

/// Encodes a UTF-8 text packet.
pub fn encode_text(text: &str) -> Result<Vec<u8>> {
    encode_data(text.as_bytes())
}

/// Encodes a flush packet.
pub fn flush() -> Vec<u8> {
    b"0000".to_vec()
}

/// Encodes a protocol v2 delimiter packet.
pub fn delimiter() -> Vec<u8> {
    b"0001".to_vec()
}

/// Encodes a protocol v2 response-end packet.
pub fn response_end() -> Vec<u8> {
    b"0002".to_vec()
}

/// Reads one packet from a stream.
///
/// Returns `None` on a clean end of stream before the length prefix.
pub fn read_from(reader: &mut impl Read) -> Result<Option<PktLine>> {
    let mut prefix = [0u8; 4];
    let mut read = 0;
    while read < prefix.len() {
        match reader.read(&mut prefix[read..]) {
            Ok(0) => {
                if read == 0 {
                    return Ok(None);
                }
                return Err(Error::corrupt("pkt-line length prefix is truncated"));
            }
            Ok(count) => read += count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(Error::io("failed to read pkt-line prefix", error)),
        }
    }

    let length = parse_length(&prefix)?;
    match length {
        0 => Ok(Some(PktLine::Flush)),
        1 => Ok(Some(PktLine::Delim)),
        2 => Ok(Some(PktLine::ResponseEnd)),
        _ if length < 4 => Err(Error::corrupt(format!("invalid pkt-line length {length}"))),
        _ => {
            let payload_len = length - 4;
            let mut payload = vec![0u8; payload_len];
            reader
                .read_exact(&mut payload)
                .map_err(|error| Error::io("failed to read pkt-line payload", error))?;
            Ok(Some(PktLine::Data(payload)))
        }
    }
}

/// Parses a buffer containing consecutive packets.
pub fn parse_all(bytes: &[u8]) -> Result<Vec<PktLine>> {
    let mut packets = Vec::new();
    let mut position = 0;

    while position < bytes.len() {
        if bytes.len() - position < 4 {
            return Err(Error::corrupt("pkt-line length prefix is truncated"));
        }
        let length = parse_length(&bytes[position..position + 4])?;
        position += 4;
        match length {
            0 => packets.push(PktLine::Flush),
            1 => packets.push(PktLine::Delim),
            2 => packets.push(PktLine::ResponseEnd),
            _ if length < 4 => {
                return Err(Error::corrupt(format!("invalid pkt-line length {length}")));
            }
            _ => {
                let payload_len = length - 4;
                let end = position
                    .checked_add(payload_len)
                    .ok_or_else(|| Error::corrupt("pkt-line length overflow"))?;
                if end > bytes.len() {
                    return Err(Error::corrupt("pkt-line payload is truncated"));
                }
                packets.push(PktLine::Data(bytes[position..end].to_vec()));
                position = end;
            }
        }
    }

    Ok(packets)
}

fn parse_length(prefix: &[u8]) -> Result<usize> {
    let text =
        std::str::from_utf8(prefix).map_err(|_| Error::corrupt("pkt-line length is not ASCII"))?;
    usize::from_str_radix(text, 16)
        .map_err(|_| Error::corrupt(format!("invalid pkt-line length: {text}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_data_and_control_packets() {
        let packets = vec![
            PktLine::Data(b"hello\n".to_vec()),
            PktLine::Delim,
            PktLine::Data(Vec::new()),
            PktLine::Flush,
            PktLine::ResponseEnd,
        ];
        let mut encoded = Vec::new();
        for packet in &packets {
            encoded.extend(packet.encode().expect("encode"));
        }
        assert_eq!(parse_all(&encoded).expect("parse"), packets);
    }

    #[test]
    fn reads_from_a_stream() {
        let bytes = b"0009hello0000";
        let mut cursor = std::io::Cursor::new(bytes);
        assert_eq!(
            read_from(&mut cursor).expect("read"),
            Some(PktLine::Data(b"hello".to_vec()))
        );
        assert_eq!(read_from(&mut cursor).expect("read"), Some(PktLine::Flush));
        assert_eq!(read_from(&mut cursor).expect("read"), None);
    }

    #[test]
    fn rejects_oversized_packets() {
        let data = vec![b'x'; MAX_PKT_LINE_LEN - 3];
        assert!(encode_data(&data).is_err());
    }

    #[test]
    fn rejects_truncated_payloads() {
        assert!(parse_all(b"0009hell").is_err());
        assert!(parse_all(b"00").is_err());
        assert!(parse_all(b"zzzz").is_err());
    }
}
