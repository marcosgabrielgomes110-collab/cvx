use std::fmt;
use std::str::FromStr;

/// Number of bytes in a SHA-1 object identifier.
pub const SHA1_BYTES: usize = 20;
/// Number of hexadecimal characters in a SHA-1 object identifier.
pub const SHA1_HEX_LEN: usize = SHA1_BYTES * 2;
/// Number of bytes in a SHA-256 object identifier.
pub const SHA256_BYTES: usize = 32;
/// Number of hexadecimal characters in a SHA-256 object identifier.
pub const SHA256_HEX_LEN: usize = SHA256_BYTES * 2;

use crate::hash::HashAlgorithm;

/// A Git object identifier.
///
/// The storage is fixed-width so values remain `Copy`; `len` distinguishes
/// SHA-1 (20 bytes) from SHA-256 (32 bytes). A repository must never mix the
/// two widths, and the repository layer enforces that invariant.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectId {
    bytes: [u8; SHA256_BYTES],
    len: u8,
}

impl ObjectId {
    /// The all-zero SHA-1 identifier used to represent an unborn reference.
    pub const ZERO: Self = Self::from_bytes([0; SHA1_BYTES]);

    /// Builds a SHA-1 identifier from raw bytes.
    pub const fn from_bytes(bytes: [u8; SHA1_BYTES]) -> Self {
        let mut storage = [0u8; SHA256_BYTES];
        let mut index = 0;
        while index < SHA1_BYTES {
            storage[index] = bytes[index];
            index += 1;
        }
        Self {
            bytes: storage,
            len: SHA1_BYTES as u8,
        }
    }

    /// Builds a SHA-256 identifier from raw bytes.
    pub const fn from_sha256_bytes(bytes: [u8; SHA256_BYTES]) -> Self {
        Self {
            bytes,
            len: SHA256_BYTES as u8,
        }
    }

    /// Builds an identifier from a byte slice with a supported width.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, ObjectIdError> {
        match bytes.len() {
            SHA1_BYTES => Ok(Self::from_bytes(bytes.try_into().expect("length checked"))),
            SHA256_BYTES => Ok(Self::from_sha256_bytes(
                bytes.try_into().expect("length checked"),
            )),
            actual => Err(ObjectIdError::InvalidLength { actual }),
        }
    }

    /// Returns the raw bytes without padding.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }

    /// Returns the identifier width in bytes.
    pub const fn len(&self) -> usize {
        self.len as usize
    }

    /// Object identifiers are never empty.
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// Returns whether this is a SHA-1 identifier.
    pub const fn is_sha1(&self) -> bool {
        self.len as usize == SHA1_BYTES
    }

    /// Returns whether this is a SHA-256 identifier.
    pub const fn is_sha256(&self) -> bool {
        self.len as usize == SHA256_BYTES
    }

    /// Returns the hash algorithm for this identifier.
    pub const fn algorithm(&self) -> HashAlgorithm {
        if self.len as usize == SHA256_BYTES {
            HashAlgorithm::Sha256
        } else {
            HashAlgorithm::Sha1
        }
    }

    /// Returns the zero identifier for an algorithm.
    pub const fn zero(algorithm: HashAlgorithm) -> Self {
        match algorithm {
            HashAlgorithm::Sha1 => Self::from_bytes([0; SHA1_BYTES]),
            HashAlgorithm::Sha256 => Self::from_sha256_bytes([0; SHA256_BYTES]),
        }
    }

    /// Parses a hexadecimal identifier of either supported width.
    pub fn from_hex(hex: &str) -> Result<Self, ObjectIdError> {
        if hex.len() != SHA1_HEX_LEN && hex.len() != SHA256_HEX_LEN {
            return Err(ObjectIdError::InvalidLength { actual: hex.len() });
        }

        let mut bytes = [0u8; SHA256_BYTES];
        for (index, chunk) in hex.as_bytes().chunks_exact(2).enumerate() {
            let high = hex_value(chunk[0]).ok_or(ObjectIdError::InvalidHex { index: index * 2 })?;
            let low = hex_value(chunk[1]).ok_or(ObjectIdError::InvalidHex {
                index: index * 2 + 1,
            })?;
            bytes[index] = (high << 4) | low;
        }

        if hex.len() == SHA1_HEX_LEN {
            Ok(Self {
                bytes,
                len: SHA1_BYTES as u8,
            })
        } else {
            Ok(Self {
                bytes,
                len: SHA256_BYTES as u8,
            })
        }
    }

    /// Formats the identifier as lowercase hexadecimal.
    pub fn to_hex(self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut output = String::with_capacity(usize::from(self.len) * 2);
        for byte in self.as_bytes() {
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
        output
    }

    /// Returns whether every byte is zero.
    pub fn is_zero(self) -> bool {
        self.as_bytes().iter().all(|byte| *byte == 0)
    }

    /// Returns a shortened hexadecimal form, clamped to the identifier width.
    pub fn short(self, length: usize) -> String {
        let maximum = usize::from(self.len) * 2;
        let length = length.clamp(4, maximum);
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
    /// The input had an unsupported number of hexadecimal characters.
    #[error("expected 40 or 64 hexadecimal characters, got {actual}")]
    InvalidLength {
        /// Actual number of hexadecimal characters.
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
    const SHA256_ID: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

    #[test]
    fn roundtrips_hex() {
        let id = ObjectId::from_hex(EMPTY_BLOB).expect("parse");
        assert_eq!(id.to_hex(), EMPTY_BLOB);
        assert_eq!(id.to_string(), EMPTY_BLOB);
        assert!(!id.is_zero());
    }

    #[test]
    fn roundtrips_sha256_hex() {
        let id = ObjectId::from_hex(SHA256_ID).expect("parse");
        assert!(id.is_sha256());
        assert_eq!(id.algorithm(), HashAlgorithm::Sha256);
        assert_eq!(id.to_hex(), SHA256_ID);
        assert_eq!(id.as_bytes().len(), SHA256_BYTES);
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
        assert!(ObjectId::zero(HashAlgorithm::Sha256).is_zero());
        assert_eq!(
            ObjectId::zero(HashAlgorithm::Sha256).to_hex().len(),
            SHA256_HEX_LEN
        );
    }

    #[test]
    fn shortens_identifiers() {
        let id = ObjectId::from_hex(EMPTY_BLOB).expect("parse");
        assert_eq!(id.short(7), "e69de29");
        assert_eq!(id.short(1).len(), 4);
        assert_eq!(id.short(100).len(), 40);

        let sha256 = ObjectId::from_hex(SHA256_ID).expect("parse");
        assert_eq!(sha256.short(100).len(), 64);
    }

    #[test]
    fn builds_from_slice() {
        let bytes = [0x42; SHA1_BYTES];
        let id = ObjectId::from_slice(&bytes).expect("slice");
        assert_eq!(id.as_bytes(), &bytes);
        assert!(ObjectId::from_slice(&bytes[..10]).is_err());

        let bytes = [0x42; SHA256_BYTES];
        let id = ObjectId::from_slice(&bytes).expect("slice");
        assert_eq!(id.as_bytes(), &bytes);
    }
}
