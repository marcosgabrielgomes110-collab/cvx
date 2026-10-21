use std::collections::HashMap;
use std::io::Read;

use cvx_core::{Error, Result};
use cvx_object::{HashAlgorithm, ObjectId, ObjectKind, hash_object_with};
use flate2::read::ZlibDecoder;

use crate::delta::apply_delta;

const MAX_OBJECT_SIZE: u64 = 1024 * 1024 * 1024;

/// An object decoded from a received packfile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestedObject {
    /// Object identifier computed from the decoded content.
    pub oid: ObjectId,
    /// Object type.
    pub kind: ObjectKind,
    /// Uncompressed object payload.
    pub data: Vec<u8>,
}

/// Decodes every object in a received packfile.
///
/// The pack trailer is verified. Delta objects are resolved against earlier
/// objects in the same pack; thin packs with external bases are rejected.
pub fn ingest_pack(bytes: &[u8]) -> Result<Vec<IngestedObject>> {
    ingest_pack_with_algorithm(bytes, HashAlgorithm::Sha1)
}

/// Decodes a received pack using a specific repository object format.
pub fn ingest_pack_with_algorithm(
    bytes: &[u8],
    algorithm: HashAlgorithm,
) -> Result<Vec<IngestedObject>> {
    let checksum_len = algorithm.oid_len();
    if bytes.len() < 12 + checksum_len {
        return Err(Error::corrupt("received pack is too short"));
    }
    let payload_end = bytes.len() - checksum_len;
    let expected = crate::checksum::checksum(algorithm, &bytes[..payload_end]);
    if expected.as_slice() != &bytes[payload_end..] {
        return Err(Error::corrupt("received pack checksum does not match"));
    }

    if &bytes[..4] != b"PACK" {
        return Err(Error::corrupt("received pack has an invalid signature"));
    }
    let version = u32::from_be_bytes(bytes[4..8].try_into().expect("length checked"));
    if version != 2 && version != 3 {
        return Err(Error::unsupported(format!("packfile version {version}")));
    }
    let count = u32::from_be_bytes(bytes[8..12].try_into().expect("length checked")) as usize;

    let mut objects: Vec<IngestedObject> = Vec::with_capacity(count);
    let mut by_offset: HashMap<usize, usize> = HashMap::new();
    let mut by_oid: HashMap<ObjectId, usize> = HashMap::new();
    let mut position = 12;

    for _ in 0..count {
        let offset = position;
        let (type_code, declared_size, data_start) = read_header(bytes, position, payload_end)?;
        position = data_start;

        let (kind, data) = match type_code {
            1..=4 => {
                let kind = match type_code {
                    1 => ObjectKind::Commit,
                    2 => ObjectKind::Tree,
                    3 => ObjectKind::Blob,
                    4 => ObjectKind::Tag,
                    _ => unreachable!("range is 1..=4"),
                };
                let (data, consumed) = decompress(&bytes[..payload_end], position)?;
                position += consumed;
                (kind, data)
            }
            6 => {
                let (distance, next) = read_offset_delta(bytes, position, payload_end)?;
                position = next;
                let distance = usize::try_from(distance)
                    .map_err(|_| Error::corrupt("received pack offset delta is too large"))?;
                let base_offset = offset.checked_sub(distance).ok_or_else(|| {
                    Error::corrupt("received pack offset delta points before the pack")
                })?;
                let base_index = *by_offset.get(&base_offset).ok_or_else(|| {
                    Error::corrupt(format!(
                        "received pack offset delta base at {base_offset} is missing"
                    ))
                })?;
                let (delta, consumed) = decompress(&bytes[..payload_end], position)?;
                position += consumed;
                let base = &objects[base_index];
                (base.kind, apply_delta(&base.data, &delta)?)
            }
            7 => {
                let base_end = position
                    .checked_add(algorithm.oid_len())
                    .ok_or_else(|| Error::corrupt("received pack ref delta is truncated"))?;
                if base_end > payload_end {
                    return Err(Error::corrupt("received pack ref delta is truncated"));
                }
                let base_oid = ObjectId::from_slice(&bytes[position..base_end])
                    .map_err(|error| Error::corrupt(format!("invalid delta base: {error}")))?;
                position = base_end;
                let base_index = *by_oid.get(&base_oid).ok_or_else(|| {
                    Error::corrupt(format!(
                        "received thin pack references missing base {base_oid}"
                    ))
                })?;
                let (delta, consumed) = decompress(&bytes[..payload_end], position)?;
                position += consumed;
                let base = &objects[base_index];
                (base.kind, apply_delta(&base.data, &delta)?)
            }
            other => {
                return Err(Error::corrupt(format!(
                    "received pack contains unknown object type {other}"
                )));
            }
        };

        if data.len() != declared_size && type_code <= 4 {
            return Err(Error::corrupt(format!(
                "received object declares {declared_size} bytes but produced {}",
                data.len()
            )));
        }
        if data.len() as u64 > MAX_OBJECT_SIZE {
            return Err(Error::corrupt("received object exceeds the size limit"));
        }

        let oid = hash_object_with(algorithm, kind, &data);
        let index = objects.len();
        objects.push(IngestedObject { oid, kind, data });
        by_offset.insert(offset, index);
        by_oid.insert(oid, index);
    }

    if position != payload_end {
        return Err(Error::corrupt(
            "received pack has trailing bytes after its objects",
        ));
    }

    Ok(objects)
}

fn read_header(bytes: &[u8], start: usize, end: usize) -> Result<(u8, usize, usize)> {
    let mut position = start;
    let first = *bytes
        .get(position)
        .filter(|_| position < end)
        .ok_or_else(|| Error::corrupt("received pack object is truncated"))?;
    position += 1;

    let type_code = (first >> 4) & 0x07;
    let mut size = u64::from(first & 0x0f);
    let mut shift = 4u32;
    let mut byte = first;

    while byte & 0x80 != 0 {
        if position >= end {
            return Err(Error::corrupt("received pack object header is truncated"));
        }
        byte = bytes[position];
        position += 1;
        size |= u64::from(byte & 0x7f) << shift;
        shift += 7;
        if shift >= 64 {
            return Err(Error::corrupt("received pack object size overflows"));
        }
    }

    let size = usize::try_from(size)
        .map_err(|_| Error::corrupt("received pack object size is too large"))?;
    Ok((type_code, size, position))
}

fn read_offset_delta(bytes: &[u8], start: usize, end: usize) -> Result<(u64, usize)> {
    let mut position = start;
    if position >= end {
        return Err(Error::corrupt("received pack offset delta is truncated"));
    }
    let mut byte = bytes[position];
    position += 1;
    let mut distance = u64::from(byte & 0x7f);

    while byte & 0x80 != 0 {
        if position >= end {
            return Err(Error::corrupt("received pack offset delta is truncated"));
        }
        byte = bytes[position];
        position += 1;
        distance = distance
            .checked_add(1)
            .and_then(|value| value.checked_shl(7))
            .and_then(|value| value.checked_add(u64::from(byte & 0x7f)))
            .ok_or_else(|| Error::corrupt("received pack offset delta overflows"))?;
    }

    Ok((distance, position))
}

fn decompress(bytes: &[u8], start: usize) -> Result<(Vec<u8>, usize)> {
    if start >= bytes.len() {
        return Err(Error::corrupt("received pack object data is truncated"));
    }
    let mut decoder = ZlibDecoder::new(&bytes[start..]);
    let mut output = Vec::new();
    (&mut decoder)
        .take(MAX_OBJECT_SIZE + 1)
        .read_to_end(&mut output)
        .map_err(|error| Error::io("failed to decompress received pack object", error))?;
    if output.len() as u64 > MAX_OBJECT_SIZE {
        return Err(Error::corrupt(
            "received object exceeds the decompression limit",
        ));
    }
    let consumed = usize::try_from(decoder.total_in())
        .map_err(|_| Error::corrupt("received pack compressed size is too large"))?;
    if consumed == 0 || start + consumed > bytes.len() {
        return Err(Error::corrupt("received pack compressed object is invalid"));
    }
    Ok((output, consumed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PackInput, write_pack};
    use std::process::Command;

    #[test]
    fn ingests_a_written_pack() {
        let temp = tempfile::tempdir().expect("tempdir");
        let outcome = write_pack(
            temp.path(),
            &[
                PackInput {
                    kind: ObjectKind::Blob,
                    data: b"hello\n",
                },
                PackInput {
                    kind: ObjectKind::Blob,
                    data: b"world\n",
                },
            ],
        )
        .expect("pack");
        let bytes = std::fs::read(&outcome.pack_path).expect("read");
        let objects = ingest_pack(&bytes).expect("ingest");
        assert_eq!(objects.len(), 2);
        assert!(objects.iter().any(|object| object.data == b"hello\n"));
    }

    #[test]
    fn ingests_a_delta_pack_from_git() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let temp = tempfile::tempdir().expect("tempdir");
        let repo = temp.path().join("repo");
        std::fs::create_dir(&repo).expect("repo");
        let git = |args: &[&str]| {
            let output = Command::new("git")
                .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
                .args(args)
                .current_dir(&repo)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", temp.path().join("gitconfig"))
                .output()
                .expect("git");
            assert!(
                output.status.success(),
                "git {:?}: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            );
        };
        git(&["init", "-q", "-b", "main"]);
        let mut content = String::new();
        for line in 0..500 {
            content.push_str(&format!("line {line}\n"));
        }
        for revision in 0..5 {
            std::fs::write(repo.join("large.txt"), format!("{content}rev {revision}\n"))
                .expect("write");
            git(&["add", "."]);
            git(&["commit", "-q", "-m", &format!("rev {revision}")]);
        }
        git(&["gc", "-q", "--aggressive"]);

        let idx = std::fs::read_dir(repo.join(".git/objects/pack"))
            .expect("pack dir")
            .filter_map(|entry| entry.ok())
            .find(|entry| entry.path().extension().is_some_and(|ext| ext == "idx"))
            .expect("idx");
        let bytes = std::fs::read(idx.path().with_extension("pack")).expect("read pack");
        let objects = ingest_pack(&bytes).expect("ingest git pack");
        assert!(!objects.is_empty());
        assert!(
            objects
                .iter()
                .any(|object| object.data.ends_with(b"rev 4\n"))
        );
    }

    #[test]
    fn rejects_bad_checksums_and_truncation() {
        let temp = tempfile::tempdir().expect("tempdir");
        let outcome = write_pack(
            temp.path(),
            &[PackInput {
                kind: ObjectKind::Blob,
                data: b"data\n",
            }],
        )
        .expect("pack");
        let mut bytes = std::fs::read(&outcome.pack_path).expect("read");
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        assert!(ingest_pack(&bytes).is_err());

        let bytes = std::fs::read(&outcome.pack_path).expect("read");
        assert!(ingest_pack(&bytes[..bytes.len() / 2]).is_err());
    }
}
