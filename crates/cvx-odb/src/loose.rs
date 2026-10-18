use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use cvx_core::{Error, Result};
use cvx_object::{ObjectId, ObjectKind, hash_object};
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;

/// Upper bound for a single decompressed object in the first milestone.
///
/// The limit protects against decompression bombs while remaining well above
/// normal source-code blobs.
const MAX_OBJECT_SIZE: u64 = 1024 * 1024 * 1024;

/// A decoded object read from storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredObject {
    /// Object type.
    pub kind: ObjectKind,
    /// Uncompressed object payload, without the `<type> <size>\0` header.
    pub data: Vec<u8>,
}

/// Loose-object storage under `objects/`.
#[derive(Debug, Clone)]
pub struct LooseStore {
    objects_dir: PathBuf,
}

impl LooseStore {
    /// Creates a store rooted at an `objects/` directory.
    pub fn new(objects_dir: impl Into<PathBuf>) -> Self {
        Self {
            objects_dir: objects_dir.into(),
        }
    }

    /// Returns the `objects/` directory.
    pub fn objects_dir(&self) -> &Path {
        &self.objects_dir
    }

    /// Returns the filesystem path for an object.
    pub fn path_for(&self, oid: ObjectId) -> PathBuf {
        let hex = oid.to_hex();
        self.objects_dir.join(&hex[..2]).join(&hex[2..])
    }

    /// Returns whether a loose object exists.
    pub fn contains(&self, oid: ObjectId) -> bool {
        self.path_for(oid).is_file()
    }

    /// Writes an object, returning its identifier.
    ///
    /// Existing objects are never overwritten. The write is atomic and the
    /// resulting loose object is read-only, matching Git's permissions.
    pub fn write(&self, kind: ObjectKind, data: &[u8]) -> Result<ObjectId> {
        let oid = hash_object(kind, data);
        let path = self.path_for(oid);
        if path.is_file() {
            return Ok(oid);
        }

        let mut header = Vec::with_capacity(32);
        header.extend_from_slice(kind.as_str().as_bytes());
        header.push(b' ');
        header.extend_from_slice(data.len().to_string().as_bytes());
        header.push(0);

        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(&header)
            .map_err(|error| Error::io("failed to compress object header", error))?;
        encoder
            .write_all(data)
            .map_err(|error| Error::io("failed to compress object data", error))?;
        let compressed = encoder
            .finish()
            .map_err(|error| Error::io("failed to finish object compression", error))?;

        cvx_core::fs::write_atomic(&path, &compressed, Some(0o444))?;
        Ok(oid)
    }

    /// Reads and verifies an object.
    pub fn read(&self, oid: ObjectId) -> Result<StoredObject> {
        let path = self.path_for(oid);
        let compressed = match std::fs::read(&path) {
            Ok(compressed) => compressed,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::object_not_found(oid.to_string()));
            }
            Err(error) => return Err(Error::io(path.display().to_string(), error)),
        };

        let mut decoder = ZlibDecoder::new(compressed.as_slice()).take(MAX_OBJECT_SIZE + 1);
        let mut raw = Vec::new();
        decoder
            .read_to_end(&mut raw)
            .map_err(|error| Error::io(format!("failed to decompress {oid}"), error))?;
        if raw.len() as u64 > MAX_OBJECT_SIZE {
            return Err(Error::corrupt(format!(
                "object {oid} exceeds the decompressed size limit"
            )));
        }

        let Some(nul) = raw.iter().position(|byte| *byte == 0) else {
            return Err(Error::corrupt(format!(
                "{} has no object header terminator",
                path.display()
            )));
        };
        let header = &raw[..nul];
        let data = &raw[nul + 1..];
        let Some((kind, size)) = split_header(header) else {
            return Err(Error::corrupt(format!(
                "{} has an invalid object header",
                path.display()
            )));
        };
        if size != data.len() {
            return Err(Error::corrupt(format!(
                "{} declares {size} bytes but contains {}",
                path.display(),
                data.len()
            )));
        }
        if hash_object(kind, data) != oid {
            return Err(Error::corrupt(format!(
                "{} does not match its object identifier",
                path.display()
            )));
        }

        Ok(StoredObject {
            kind,
            data: data.to_vec(),
        })
    }
}

fn split_header(header: &[u8]) -> Option<(ObjectKind, usize)> {
    let header = std::str::from_utf8(header).ok()?;
    let (kind, size) = header.split_once(' ')?;
    let kind = ObjectKind::parse(kind)?;
    let size = size.parse().ok()?;
    Some((kind, size))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, LooseStore) {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = LooseStore::new(temp.path().join("objects"));
        (temp, store)
    }

    #[allow(clippy::permissions_set_readonly_false)]
    fn remove_readonly(path: &std::path::Path) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644))
                .expect("permissions");
        }
        #[cfg(not(unix))]
        {
            let mut permissions = std::fs::metadata(path).expect("metadata").permissions();
            permissions.set_readonly(false);
            std::fs::set_permissions(path, permissions).expect("permissions");
        }
        std::fs::remove_file(path).expect("remove");
    }

    #[test]
    fn writes_and_reads_blob() {
        let (_temp, store) = store();
        let oid = store.write(ObjectKind::Blob, b"hello\n").expect("write");
        assert_eq!(oid.to_hex(), "ce013625030ba8dba906f756967f9e9ca394464a");
        assert!(store.contains(oid));

        let object = store.read(oid).expect("read");
        assert_eq!(object.kind, ObjectKind::Blob);
        assert_eq!(object.data, b"hello\n");
    }

    #[test]
    fn deduplicates_objects() {
        let (_temp, store) = store();
        let first = store.write(ObjectKind::Blob, b"same").expect("write");
        let second = store.write(ObjectKind::Blob, b"same").expect("write");
        assert_eq!(first, second);
    }

    #[test]
    fn reports_missing_objects() {
        let (_temp, store) = store();
        let oid = ObjectId::from_hex("1111111111111111111111111111111111111111").unwrap();
        assert!(matches!(store.read(oid), Err(Error::ObjectNotFound { .. })));
    }

    #[test]
    fn rejects_corrupt_payload() {
        let (_temp, store) = store();
        let oid = store.write(ObjectKind::Blob, b"original").expect("write");
        let path = store.path_for(oid);
        remove_readonly(&path);
        std::fs::write(&path, b"not zlib").expect("corrupt");
        assert!(store.read(oid).is_err());
    }

    #[test]
    fn rejects_hash_mismatch() {
        let (_temp, store) = store();
        let oid = store.write(ObjectKind::Blob, b"original").expect("write");

        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(b"blob 5\0other").expect("encode");
        let compressed = encoder.finish().expect("finish");
        let path = store.path_for(oid);
        remove_readonly(&path);
        std::fs::write(&path, compressed).expect("replace");

        assert!(matches!(store.read(oid), Err(Error::Corrupt { .. })));
    }

    #[cfg(unix)]
    #[test]
    fn loose_objects_are_read_only() {
        use std::os::unix::fs::PermissionsExt;

        let (_temp, store) = store();
        let oid = store.write(ObjectKind::Blob, b"readonly").expect("write");
        let mode = std::fs::metadata(store.path_for(oid))
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o444);
    }
}
