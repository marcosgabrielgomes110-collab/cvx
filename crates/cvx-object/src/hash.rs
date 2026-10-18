use sha1::{Digest, Sha1};

use crate::{ObjectId, ObjectKind};

/// Hash algorithms recognized by this build of CVX.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HashAlgorithm {
    /// SHA-1, the only format supported in the first milestone.
    #[default]
    Sha1,
}

impl HashAlgorithm {
    /// Returns the canonical name used in `extensions.objectformat`.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sha1 => "sha1",
        }
    }

    /// Returns the identifier length in bytes.
    pub const fn oid_len(self) -> usize {
        match self {
            Self::Sha1 => 20,
        }
    }

    /// Parses an object format name.
    ///
    /// SHA-256 is recognized as a valid Git format, but this build rejects it
    /// explicitly instead of risking a repository with mixed identifiers.
    pub fn parse(name: &str) -> Result<Self, UnsupportedHashAlgorithm> {
        match name.to_ascii_lowercase().as_str() {
            "sha1" => Ok(Self::Sha1),
            other => Err(UnsupportedHashAlgorithm(other.to_owned())),
        }
    }
}

/// Returned when a repository declares an object format CVX cannot handle.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("object format '{0}' is not supported yet; CVX currently supports sha1")]
pub struct UnsupportedHashAlgorithm(pub String);

/// Computes the Git object identifier for `content`.
///
/// The object header (`<kind> <length>\0`) is hashed together with the content,
/// exactly as Git does for loose and packed objects.
pub fn hash_object(kind: ObjectKind, content: &[u8]) -> ObjectId {
    let mut hasher = Sha1::new();
    hasher.update(kind.as_str().as_bytes());
    hasher.update(b" ");
    hasher.update(content.len().to_string().as_bytes());
    hasher.update(b"\0");
    hasher.update(content);

    let digest: [u8; 20] = hasher.finalize().into();
    ObjectId::from_bytes(digest)
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
    fn only_sha1_is_supported() {
        assert_eq!(HashAlgorithm::parse("sha1").unwrap(), HashAlgorithm::Sha1);
        assert_eq!(HashAlgorithm::parse("SHA1").unwrap(), HashAlgorithm::Sha1);
        assert!(HashAlgorithm::parse("sha256").is_err());
    }
}
