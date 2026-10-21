use sha1::Sha1;
use sha2::Sha256;

use crate::{ObjectId, ObjectKind};

/// Hash algorithms recognized by this build of CVX.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HashAlgorithm {
    /// SHA-1 repositories.
    #[default]
    Sha1,
    /// SHA-256 repositories.
    Sha256,
}

impl HashAlgorithm {
    /// Returns the canonical name used in `extensions.objectformat`.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sha1 => "sha1",
            Self::Sha256 => "sha256",
        }
    }

    /// Returns the identifier length in bytes.
    pub const fn oid_len(self) -> usize {
        match self {
            Self::Sha1 => 20,
            Self::Sha256 => 32,
        }
    }

    /// Returns the identifier length in hexadecimal characters.
    pub const fn hex_len(self) -> usize {
        self.oid_len() * 2
    }

    /// Parses an object format name.
    pub fn parse(name: &str) -> Result<Self, UnsupportedHashAlgorithm> {
        match name.to_ascii_lowercase().as_str() {
            "sha1" => Ok(Self::Sha1),
            "sha256" => Ok(Self::Sha256),
            other => Err(UnsupportedHashAlgorithm(other.to_owned())),
        }
    }
}

/// Returned when a repository declares an object format CVX cannot handle.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("object format '{0}' is not supported; expected sha1 or sha256")]
pub struct UnsupportedHashAlgorithm(pub String);

/// Computes a SHA-1 object identifier for `content`.
pub fn hash_object(kind: ObjectKind, content: &[u8]) -> ObjectId {
    hash_object_with(HashAlgorithm::Sha1, kind, content)
}

/// Computes an object identifier using the requested algorithm.
pub fn hash_object_with(algorithm: HashAlgorithm, kind: ObjectKind, content: &[u8]) -> ObjectId {
    let header = format!("{} {}\0", kind.as_str(), content.len());
    match algorithm {
        HashAlgorithm::Sha1 => {
            use sha1::Digest;
            let mut hasher = Sha1::new();
            hasher.update(header.as_bytes());
            hasher.update(content);
            let digest: [u8; 20] = hasher.finalize().into();
            ObjectId::from_bytes(digest)
        }
        HashAlgorithm::Sha256 => {
            use sha2::Digest;
            let mut hasher = Sha256::new();
            hasher.update(header.as_bytes());
            hasher.update(content);
            let digest: [u8; 32] = hasher.finalize().into();
            ObjectId::from_sha256_bytes(digest)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_empty_blob() {
        assert_eq!(
            hash_object(ObjectKind::Blob, b"").to_hex(),
            "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391"
        );
    }

    #[test]
    fn hashes_hello_blob() {
        assert_eq!(
            hash_object(ObjectKind::Blob, b"hello\n").to_hex(),
            "ce013625030ba8dba906f756967f9e9ca394464a"
        );
    }

    #[test]
    fn hashes_empty_tree() {
        assert_eq!(
            hash_object(ObjectKind::Tree, b"").to_hex(),
            "4b825dc642cb6eb9a060e54bf8d69288fbee4904"
        );
    }

    #[test]
    fn supports_both_formats() {
        assert_eq!(HashAlgorithm::parse("sha1").unwrap(), HashAlgorithm::Sha1);
        assert_eq!(
            HashAlgorithm::parse("sha256").unwrap(),
            HashAlgorithm::Sha256
        );
        assert!(HashAlgorithm::parse("md5").is_err());

        let sha256 = hash_object_with(HashAlgorithm::Sha256, ObjectKind::Blob, b"hello\n");
        assert!(sha256.is_sha256());
        assert_eq!(sha256.to_hex().len(), 64);
        assert_eq!(
            sha256.to_hex(),
            "2cf8d83d9ee29543b34a87727421fdecb7e3f3a183d337639025de576db9ebb4"
        );
    }
}
