use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use cvx_core::{Error, Result};
use cvx_object::{ObjectId, ObjectKind, hash_object};
use flate2::Compression;
use flate2::write::ZlibEncoder;
use sha1::{Digest, Sha1};

const PACK_VERSION: u32 = 2;
const IDX_MAGIC: &[u8; 4] = b"\xfftOc";
const IDX_VERSION: u32 = 2;
const CHECKSUM_LEN: usize = 20;
const LARGE_OFFSET_FLAG: u32 = 0x8000_0000;

/// One object to include in a packfile.
#[derive(Debug, Clone, Copy)]
pub struct PackInput<'a> {
    /// Object type.
    pub kind: ObjectKind,
    /// Uncompressed object payload.
    pub data: &'a [u8],
}

/// Information about an object written to a packfile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackedObject {
    /// Object identifier.
    pub oid: ObjectId,
    /// Offset of the object header inside the packfile.
    pub offset: u64,
    /// Uncompressed object size.
    pub size: u64,
}

/// Result of writing a packfile and its index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackWriteOutcome {
    /// Path to the `.pack` file.
    pub pack_path: PathBuf,
    /// Path to the `.idx` file.
    pub index_path: PathBuf,
    /// Pack checksum, also used in the file names.
    pub checksum: ObjectId,
    /// Objects in index order.
    pub objects: Vec<PackedObject>,
}

/// Writes a version-2 packfile and its version-2 index.
///
/// Objects are written as complete zlib-compressed entries without deltas.
/// Duplicate object identifiers are removed, and objects are written in
/// identifier order so the output is deterministic.
pub fn write_pack(
    directory: impl AsRef<Path>,
    inputs: &[PackInput<'_>],
) -> Result<PackWriteOutcome> {
    let directory = directory.as_ref();
    cvx_core::fs::ensure_dir(directory)?;

    let built = build_pack(inputs)?;
    let index = build_index(&built.index_entries, built.checksum_bytes);
    let stem = format!("pack-{}", built.checksum.to_hex());
    let pack_path = directory.join(format!("{stem}.pack"));
    let index_path = directory.join(format!("{stem}.idx"));
    cvx_core::fs::write_atomic(&pack_path, &built.bytes, Some(0o444))?;
    cvx_core::fs::write_atomic(&index_path, &index, Some(0o444))?;

    Ok(PackWriteOutcome {
        pack_path,
        index_path,
        checksum: built.checksum,
        objects: built.objects,
    })
}

/// Builds a version-2 packfile in memory, without writing an index.
pub fn encode_pack(inputs: &[PackInput<'_>]) -> Result<Vec<u8>> {
    Ok(build_pack(inputs)?.bytes)
}

struct BuiltPack {
    bytes: Vec<u8>,
    checksum: ObjectId,
    checksum_bytes: [u8; CHECKSUM_LEN],
    objects: Vec<PackedObject>,
    index_entries: Vec<(ObjectId, u32, u64)>,
}

fn build_pack(inputs: &[PackInput<'_>]) -> Result<BuiltPack> {
    let mut objects: BTreeMap<ObjectId, (ObjectKind, &[u8])> = BTreeMap::new();
    for input in inputs {
        let oid = hash_object(input.kind, input.data);
        objects.entry(oid).or_insert((input.kind, input.data));
    }

    let mut pack = Vec::new();
    pack.extend_from_slice(b"PACK");
    pack.extend_from_slice(&PACK_VERSION.to_be_bytes());
    pack.extend_from_slice(&(objects.len() as u32).to_be_bytes());

    let mut packed = Vec::with_capacity(objects.len());
    let mut index_entries = Vec::with_capacity(objects.len());

    for (oid, (kind, data)) in &objects {
        let offset = pack.len() as u64;
        let header = object_header(*kind, data.len() as u64)?;

        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(data)
            .map_err(|error| Error::io("failed to compress pack object data", error))?;
        let compressed = encoder
            .finish()
            .map_err(|error| Error::io("failed to finish pack object compression", error))?;

        let mut crc = crc32fast::Hasher::new();
        crc.update(&header);
        crc.update(&compressed);
        let crc = crc.finalize();

        pack.extend_from_slice(&header);
        pack.extend_from_slice(&compressed);
        packed.push(PackedObject {
            oid: *oid,
            offset,
            size: data.len() as u64,
        });
        index_entries.push((*oid, crc, offset));
    }

    let checksum_bytes: [u8; CHECKSUM_LEN] = Sha1::digest(&pack).into();
    let checksum = ObjectId::from_bytes(checksum_bytes);
    pack.extend_from_slice(&checksum_bytes);

    Ok(BuiltPack {
        bytes: pack,
        checksum,
        checksum_bytes,
        objects: packed,
        index_entries,
    })
}

fn object_header(kind: ObjectKind, size: u64) -> Result<Vec<u8>> {
    let type_code: u8 = match kind {
        ObjectKind::Commit => 1,
        ObjectKind::Tree => 2,
        ObjectKind::Blob => 3,
        ObjectKind::Tag => 4,
    };

    let mut output = Vec::new();
    let mut first = (type_code << 4) | (size & 0x0f) as u8;
    let mut remaining = size >> 4;
    if remaining > 0 {
        first |= 0x80;
    }
    output.push(first);

    while remaining > 0 {
        let mut byte = (remaining & 0x7f) as u8;
        remaining >>= 7;
        if remaining > 0 {
            byte |= 0x80;
        }
        output.push(byte);
    }

    Ok(output)
}

fn build_index(entries: &[(ObjectId, u32, u64)], pack_checksum: [u8; 20]) -> Vec<u8> {
    let mut output = Vec::new();
    output.extend_from_slice(IDX_MAGIC);
    output.extend_from_slice(&IDX_VERSION.to_be_bytes());

    let mut fanout = [0u32; 256];
    for (oid, _, _) in entries {
        fanout[usize::from(oid.as_bytes()[0])] += 1;
    }
    let mut cumulative = 0u32;
    for count in fanout {
        cumulative += count;
        output.extend_from_slice(&cumulative.to_be_bytes());
    }

    for (oid, _, _) in entries {
        output.extend_from_slice(oid.as_bytes());
    }
    for (_, crc, _) in entries {
        output.extend_from_slice(&crc.to_be_bytes());
    }

    let mut large_offsets = Vec::new();
    for (_, _, offset) in entries {
        if *offset < u64::from(LARGE_OFFSET_FLAG) {
            output.extend_from_slice(&(*offset as u32).to_be_bytes());
        } else {
            let index = large_offsets.len() as u32;
            output.extend_from_slice(&(LARGE_OFFSET_FLAG | index).to_be_bytes());
            large_offsets.push(*offset);
        }
    }
    for offset in large_offsets {
        output.extend_from_slice(&offset.to_be_bytes());
    }

    output.extend_from_slice(&pack_checksum);
    let index_checksum: [u8; CHECKSUM_LEN] = Sha1::digest(&output).into();
    output.extend_from_slice(&index_checksum);
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Pack;
    use std::process::Command;

    fn has_git() -> bool {
        Command::new("git").arg("--version").output().is_ok()
    }

    #[test]
    fn writes_a_pack_readable_by_cvx_and_git() {
        if !has_git() {
            return;
        }

        let temp = tempfile::tempdir().expect("tempdir");
        let pack_dir = temp.path().join("objects/pack");
        let inputs = [
            PackInput {
                kind: ObjectKind::Blob,
                data: b"hello world\n",
            },
            PackInput {
                kind: ObjectKind::Blob,
                data: b"hello world\n",
            },
            PackInput {
                kind: ObjectKind::Blob,
                data: b"second object\n",
            },
        ];
        let outcome = write_pack(&pack_dir, &inputs).expect("write pack");
        assert_eq!(outcome.objects.len(), 2);
        assert!(outcome.pack_path.is_file());
        assert!(outcome.index_path.is_file());

        let pack = Pack::open(&outcome.index_path).expect("open written pack");
        assert_eq!(pack.object_count(), 2);
        for object in &outcome.objects {
            assert_eq!(pack.read(object.oid).expect("read").kind, ObjectKind::Blob);
        }

        let repo = temp.path().join("repo");
        std::fs::create_dir(&repo).expect("repo");
        let git = |args: &[&str]| {
            let output = Command::new("git")
                .args(args)
                .current_dir(&repo)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", temp.path().join("gitconfig"))
                .output()
                .expect("git");
            assert!(
                output.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            );
            output
        };
        git(&["init", "-q", "-b", "main"]);
        let objects_dir = repo.join(".git/objects/pack");
        std::fs::create_dir_all(&objects_dir).expect("objects");
        std::fs::copy(&outcome.pack_path, objects_dir.join("pack.pack")).expect("copy pack");
        std::fs::copy(&outcome.index_path, objects_dir.join("pack.idx")).expect("copy idx");

        git(&[
            "verify-pack",
            "-v",
            objects_dir.join("pack.idx").to_str().unwrap(),
        ]);
        for object in &outcome.objects {
            let output = git(&["cat-file", "-p", &object.oid.to_hex()]);
            assert!(!output.stdout.is_empty());
        }
    }

    #[test]
    fn encodes_object_headers() {
        assert_eq!(object_header(ObjectKind::Blob, 3).unwrap(), vec![0x33]);
        assert_eq!(
            object_header(ObjectKind::Commit, 16).unwrap(),
            vec![0x90, 0x01]
        );
        assert_eq!(
            object_header(ObjectKind::Tree, 1000).unwrap(),
            vec![0xa8, 0x3e]
        );
    }
}
