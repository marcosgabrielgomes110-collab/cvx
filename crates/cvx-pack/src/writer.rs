use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use cvx_core::{Error, Result};
use cvx_object::{HashAlgorithm, ObjectId, ObjectKind, hash_object_with};
use flate2::Compression;
use flate2::write::ZlibEncoder;

const PACK_VERSION: u32 = 2;
const IDX_MAGIC: &[u8; 4] = b"\xfftOc";
const IDX_VERSION: u32 = 2;
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
/// Objects are written with offset deltas when that is smaller than a complete
/// zlib-compressed entry. Duplicate object identifiers are removed and the
/// output is deterministic.
pub fn write_pack(
    directory: impl AsRef<Path>,
    inputs: &[PackInput<'_>],
) -> Result<PackWriteOutcome> {
    write_pack_with_algorithm(directory, inputs, HashAlgorithm::Sha1)
}

/// Writes a pack using a specific repository object format.
pub fn write_pack_with_algorithm(
    directory: impl AsRef<Path>,
    inputs: &[PackInput<'_>],
    algorithm: HashAlgorithm,
) -> Result<PackWriteOutcome> {
    let directory = directory.as_ref();
    cvx_core::fs::ensure_dir(directory)?;

    let built = build_pack(inputs, algorithm)?;
    let index = build_index(&built.index_entries, &built.checksum_bytes, algorithm);
    let stem = format!("pack-{}", built.checksum.to_hex());
    let pack_path = directory.join(format!("{stem}.pack"));
    let index_path = directory.join(format!("{stem}.idx"));
    // Replacing an existing pack on Windows fails while another process or
    // reader has it open. The checksum names the content, so an existing pair
    // is already the exact pack we would write.
    if !(pack_path.is_file() && index_path.is_file()) {
        cvx_core::fs::write_atomic(&pack_path, &built.bytes, Some(0o444))?;
        cvx_core::failpoint::hit("pack.before-index")?;
        cvx_core::fs::write_atomic(&index_path, &index, Some(0o444))?;
    }

    Ok(PackWriteOutcome {
        pack_path,
        index_path,
        checksum: built.checksum,
        objects: built.objects,
    })
}

/// Builds a version-2 packfile in memory, without writing an index.
pub fn encode_pack(inputs: &[PackInput<'_>]) -> Result<Vec<u8>> {
    encode_pack_with_algorithm(inputs, HashAlgorithm::Sha1)
}

/// Builds a pack in memory using a specific repository object format.
pub fn encode_pack_with_algorithm(
    inputs: &[PackInput<'_>],
    algorithm: HashAlgorithm,
) -> Result<Vec<u8>> {
    Ok(build_pack(inputs, algorithm)?.bytes)
}

struct BuiltPack {
    bytes: Vec<u8>,
    checksum: ObjectId,
    checksum_bytes: Vec<u8>,
    objects: Vec<PackedObject>,
    index_entries: Vec<(ObjectId, u32, u64)>,
}

fn build_pack(inputs: &[PackInput<'_>], algorithm: HashAlgorithm) -> Result<BuiltPack> {
    let mut objects: BTreeMap<ObjectId, (ObjectKind, &[u8])> = BTreeMap::new();
    for input in inputs {
        let oid = hash_object_with(algorithm, input.kind, input.data);
        objects.entry(oid).or_insert((input.kind, input.data));
    }

    let mut ordered: Vec<(ObjectId, ObjectKind, &[u8])> = objects
        .into_iter()
        .map(|(oid, (kind, data))| (oid, kind, data))
        .collect();
    ordered.sort_by_key(|(oid, kind, data)| (kind_rank(*kind), data.len(), *oid));

    let mut pack = Vec::new();
    pack.extend_from_slice(b"PACK");
    pack.extend_from_slice(&PACK_VERSION.to_be_bytes());
    pack.extend_from_slice(&(ordered.len() as u32).to_be_bytes());

    let mut packed = Vec::with_capacity(ordered.len());
    let mut index_entries = Vec::with_capacity(ordered.len());
    let mut recent: Vec<Candidate<'_>> = Vec::new();

    for (oid, kind, data) in ordered {
        let offset = pack.len() as u64;
        let start = pack.len();
        let full_header = object_header(kind, data.len() as u64)?;
        let full_compressed = compress(data)?;
        let mut best_size = full_header.len() + full_compressed.len();
        let mut chosen: Option<(Vec<u8>, usize)> = None;

        for base in recent
            .iter()
            .rev()
            .filter(|base| base.kind == kind)
            .take(MAX_WRITE_DELTA_CANDIDATES)
        {
            if data.len() > MAX_WRITE_DELTA_INPUT || base.data.len() > MAX_WRITE_DELTA_INPUT {
                continue;
            }
            if base.depth >= MAX_WRITE_DELTA_DEPTH {
                continue;
            }
            if base.data.len() == data.len() && base.data == data {
                continue;
            }
            if !worth_delta(base.data, data) {
                continue;
            }
            let delta = crate::delta::create_delta(base.data, data);
            if delta.len() >= data.len() {
                continue;
            }
            let delta_header = object_header_code(6, delta.len() as u64)?;
            let distance = offset - base.offset;
            let ofs = encode_ofs_delta(distance)?;
            let delta_compressed = compress(&delta)?;
            let total = delta_header.len() + ofs.len() + delta_compressed.len();
            if total < best_size {
                best_size = total;
                chosen = Some((
                    [delta_header, ofs, delta_compressed].concat(),
                    base.depth + 1,
                ));
                if total * 2 < data.len() {
                    break;
                }
            }
        }

        let depth = match chosen {
            Some((bytes, depth)) => {
                pack.extend_from_slice(&bytes);
                depth
            }
            None => {
                pack.extend_from_slice(&full_header);
                pack.extend_from_slice(&full_compressed);
                0
            }
        };

        let mut crc = crc32fast::Hasher::new();
        crc.update(&pack[start..]);
        let crc = crc.finalize();

        packed.push(PackedObject {
            oid,
            offset,
            size: data.len() as u64,
        });
        index_entries.push((oid, crc, offset));
        recent.push(Candidate {
            kind,
            offset,
            data,
            depth,
        });
        if recent.len() > 64 {
            recent.remove(0);
        }
    }

    index_entries.sort_by_key(|(oid, _, _)| *oid);

    let checksum_bytes = crate::checksum::checksum(algorithm, &pack);
    let checksum = ObjectId::from_slice(&checksum_bytes)
        .map_err(|error| Error::corrupt(format!("invalid pack checksum: {error}")))?;
    pack.extend_from_slice(&checksum_bytes);

    Ok(BuiltPack {
        bytes: pack,
        checksum,
        checksum_bytes,
        objects: packed,
        index_entries,
    })
}

fn worth_delta(base: &[u8], target: &[u8]) -> bool {
    const SAMPLE: usize = 64;
    const MIN_SHARED: usize = 16;
    let prefix = base
        .iter()
        .zip(target)
        .take(SAMPLE)
        .take_while(|(left, right)| left == right)
        .count();
    if prefix >= MIN_SHARED {
        return true;
    }
    let base_tail = &base[prefix..];
    let target_tail = &target[prefix..];
    let suffix = base_tail
        .iter()
        .rev()
        .zip(target_tail.iter().rev())
        .take(SAMPLE)
        .take_while(|(left, right)| left == right)
        .count();
    prefix + suffix >= MIN_SHARED
}

struct Candidate<'a> {
    kind: ObjectKind,
    offset: u64,
    data: &'a [u8],
    depth: usize,
}

const MAX_WRITE_DELTA_DEPTH: usize = 32;
const MAX_WRITE_DELTA_CANDIDATES: usize = 8;
const MAX_WRITE_DELTA_INPUT: usize = 8 * 1024 * 1024;

fn kind_rank(kind: ObjectKind) -> u8 {
    match kind {
        ObjectKind::Commit => 0,
        ObjectKind::Tree => 1,
        ObjectKind::Tag => 2,
        ObjectKind::Blob => 3,
    }
}

fn compress(data: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(data)
        .map_err(|error| Error::io("failed to compress pack object data", error))?;
    encoder
        .finish()
        .map_err(|error| Error::io("failed to finish pack object compression", error))
}

fn encode_ofs_delta(mut distance: u64) -> Result<Vec<u8>> {
    if distance == 0 {
        return Err(Error::corrupt("pack delta distance cannot be zero"));
    }
    let mut bytes = vec![(distance & 0x7f) as u8];
    distance >>= 7;
    while distance > 0 {
        distance -= 1;
        bytes.push(((distance & 0x7f) as u8) | 0x80);
        distance >>= 7;
    }
    bytes.reverse();
    Ok(bytes)
}

fn object_header(kind: ObjectKind, size: u64) -> Result<Vec<u8>> {
    let type_code: u8 = match kind {
        ObjectKind::Commit => 1,
        ObjectKind::Tree => 2,
        ObjectKind::Blob => 3,
        ObjectKind::Tag => 4,
    };
    object_header_code(type_code, size)
}

fn object_header_code(type_code: u8, size: u64) -> Result<Vec<u8>> {
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

fn build_index(
    entries: &[(ObjectId, u32, u64)],
    pack_checksum: &[u8],
    algorithm: HashAlgorithm,
) -> Vec<u8> {
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

    output.extend_from_slice(pack_checksum);
    let index_checksum = crate::checksum::checksum(algorithm, &output);
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
    fn writes_and_reads_sha256_pack() {
        let temp = tempfile::tempdir().expect("tempdir");
        let inputs = [
            PackInput {
                kind: ObjectKind::Blob,
                data: b"hello\n",
            },
            PackInput {
                kind: ObjectKind::Blob,
                data: b"world\n",
            },
        ];
        let outcome =
            write_pack_with_algorithm(temp.path(), &inputs, cvx_object::HashAlgorithm::Sha256)
                .expect("write");
        assert!(outcome.objects.iter().all(|object| object.oid.is_sha256()));
        let pack =
            Pack::open_with_algorithm(&outcome.index_path, cvx_object::HashAlgorithm::Sha256)
                .expect("open");
        for object in &outcome.objects {
            assert_eq!(
                pack.read(object.oid).expect("read").data,
                inputs
                    .iter()
                    .find(|input| {
                        cvx_object::hash_object_with(
                            cvx_object::HashAlgorithm::Sha256,
                            input.kind,
                            input.data,
                        ) == object.oid
                    })
                    .expect("input")
                    .data
            );
        }
    }

    #[test]
    fn writes_offset_deltas_for_similar_objects() {
        let temp = tempfile::tempdir().expect("tempdir");
        let base = "shared line\n".repeat(2_000).into_bytes();
        let mut target = base.clone();
        target.extend_from_slice(b"one more line\n");
        let inputs = [
            PackInput {
                kind: ObjectKind::Blob,
                data: &base,
            },
            PackInput {
                kind: ObjectKind::Blob,
                data: &target,
            },
        ];
        let outcome = write_pack(temp.path(), &inputs).expect("write");
        let bytes = std::fs::read(&outcome.pack_path).expect("pack");
        assert!(
            outcome
                .objects
                .iter()
                .any(|object| (bytes[object.offset as usize] >> 4) & 7 == 6),
            "expected at least one offset delta"
        );
        let pack = Pack::open(&outcome.index_path).expect("open");
        assert_eq!(pack.read(outcome.objects[0].oid).unwrap().data, base);
        assert_eq!(pack.read(outcome.objects[1].oid).unwrap().data, target);

        let verify = std::process::Command::new("git")
            .args(["verify-pack", "-v"])
            .arg(&outcome.index_path)
            .output()
            .expect("run git verify-pack");
        assert!(
            verify.status.success(),
            "git verify-pack failed: {}",
            String::from_utf8_lossy(&verify.stderr)
        );
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
