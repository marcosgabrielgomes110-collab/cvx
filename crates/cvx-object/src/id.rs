use std::fmt;
use std::str::FromStr;

/// Number of bytes in a SHA-1 object identifier.
pub const SHA1_BYTES: usize = 20;

/// Number of hexadecimal characters in a SHA-1 object identifier.
pub const SHA1_HEX_LEN: usize = SHA1_BYTES * 2;

/// A Git object identifier.
///
/// CVX currently supports only SHA-1 repositories. The type is deliberately
/// fixed-size so that a future SHA-256 implementation cannot silently mix
/// identifier widths.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectId([u8; SHA1_BYTES]);

impl ObjectId {
    /// The all-zero identifier used to represent an unborn reference.
    pub const ZERO: Self = Self([0; SHA1_BYTES]);

    /// Builds an identifier from raw bytes.
    pub const fn from_bytes(bytes: [u8; SHA1_BYTES]) -> Self {
        Self(bytes)
    }

    /// Builds an identifier from a byte slice, checking its length.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, ObjectIdError> {
        let bytes: [u8; SHA1_BYTES] =
            bytes.try_into().map_err(|_| ObjectIdError::InvalidLength {
                expected: SHA1_HEX_LEN,
                actual: bytes.len() * 2,
            })?;
        Ok(Self(bytes))
    }

    /// Returns the raw bytes.
    pub const fn as_bytes(&self) -> &[u8; SHA1_BYTES] {
        &self.0
    }

    /// Parses a 40-character hexadecimal identifier.
    pub fn from_hex(hex: &str) -> Result<Self, ObjectIdError> {
        if hex.len() != SHA1_HEX_LEN {
            return Err(ObjectIdError::InvalidLength {
                expected: SHA1_HEX_LEN,
                actual: hex.len(),
            });
        }

        let mut bytes = [0u8; SHA1_BYTES];
        for (index, chunk) in hex.as_bytes().chunks_exact(2).enumerate() {
            let high = hex_value(chunk[0]).ok_or(ObjectIdError::InvalidHex { index: index * 2 })?;
            let low = hex_value(chunk[1]).ok_or(ObjectIdError::InvalidHex {
                index: index * 2 + 1,
            })?;
            bytes[index] = (high << 4) | low;
        }

        Ok(Self(bytes))
    }

    /// Formats the identifier as lowercase hexadecimal.
    pub fn to_hex(self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut output = String::with_capacity(SHA1_HEX_LEN);
        for byte in self.0 {
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
        output
    }

    /// Returns whether every byte is zero.
    pub fn is_zero(self) -> bool {
        self.0 == [0; SHA1_BYTES]
    }

    /// Returns a shortened hexadecimal form, clamped to 4..=40 characters.
    pub fn short(self, length: usize) -> String {
        let length = length.clamp(4, SHA1_HEX_LEN);
        self.to_hex()[..length].to_owned()
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

impl fmt::Debug for ObjectId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ObjectId({self})")
    }
}

impl FromStr for ObjectId {
    type Err = ObjectIdError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::from_hex(input)
    }
}

/// Errors produced while parsing or constructing an object identifier.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ObjectIdError {
    /// The input had the wrong number of hexadecimal characters.
    #[error("expected {expected} hexadecimal characters, got {actual}")]
    InvalidLength {
        /// Expected number of characters.
        expected: usize,
        /// Actual number of characters.
        actual: usize,
    },

    /// The input contained a non-hexadecimal character.
    #[error("invalid hexadecimal character at position {index}")]
    InvalidHex {
        /// Zero-based position of the offending character.
        index: usize,
    },
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMPTY_BLOB: &str = "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391";

    #[test]
    fn roundtrips_hex() {
        let id = ObjectId::from_hex(EMPTY_BLOB).expect("parse");
        assert_eq!(id.to_hex(), EMPTY_BLOB);
        assert_eq!(id.to_string(), EMPTY_BLOB);
        assert!(!id.is_zero());
    }

    #[test]
    fn accepts_uppercase_hex() {
        let id = ObjectId::from_hex(&EMPTY_BLOB.to_ascii_uppercase()).expect("parse");
        assert_eq!(id.to_hex(), EMPTY_BLOB);
    }

    #[test]
    fn rejects_bad_hex() {
        assert!(matches!(
            ObjectId::from_hex("abc"),
            Err(ObjectIdError::InvalidLength { .. })
        ));
        assert!(matches!(
            ObjectId::from_hex("z000000000000000000000000000000000000000"),
            Err(ObjectIdError::InvalidHex { index: 0 })
        ));
    }

    #[test]
    fn zero_identifier_is_detected() {
        assert!(ObjectId::ZERO.is_zero());
        assert_eq!(
            ObjectId::ZERO.to_hex(),
            "0000000000000000000000000000000000000000"
        );
    }

    #[test]
    fn shortens_identifiers() {
        let id = ObjectId::from_hex(EMPTY_BLOB).expect("parse");
        assert_eq!(id.short(7), "e69de29");
        assert_eq!(id.short(1).len(), 4);
        assert_eq!(id.short(100).len(), 40);
    }

    #[test]
    fn builds_from_slice() {
        let bytes = [0x42; SHA1_BYTES];
        let id = ObjectId::from_slice(&bytes).expect("slice");
        assert_eq!(id.as_bytes(), &bytes);
        assert!(ObjectId::from_slice(&bytes[..10]).is_err());
    }
}
