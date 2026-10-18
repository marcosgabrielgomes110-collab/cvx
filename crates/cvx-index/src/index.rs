use std::path::Path;

use cvx_core::{Error, Result};
use cvx_object::ObjectId;
use sha1::{Digest, Sha1};

use crate::entry::{IndexEntry, validate_path};

const MAGIC: &[u8; 4] = b"DIRC";
const CHECKSUM_LEN: usize = 20;

/// Parsed Git index (staging area).
#[derive(Debug, Clone)]
pub struct Index {
    version: u32,
    entries: Vec<IndexEntry>,
}

impl Default for Index {
    fn default() -> Self {
        Self::new()
    }
}

impl Index {
    /// Creates an empty version-2 index.
    pub fn new() -> Self {
        Self {
            version: 2,
            entries: Vec::new(),
        }
    }

    /// Returns the index format version.
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Returns all entries in index order.
    pub fn entries(&self) -> &[IndexEntry] {
        &self.entries
    }

    /// Returns mutable access to all entries.
    pub fn entries_mut(&mut self) -> &mut Vec<IndexEntry> {
        &mut self.entries
    }

    /// Returns whether the index has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the first entry for a path, regardless of stage.
    pub fn get(&self, path: &[u8]) -> Option<&IndexEntry> {
        self.entries.iter().find(|entry| entry.path == path)
    }

    /// Returns the entry for a path and merge stage.
    pub fn get_stage(&self, path: &[u8], stage: u8) -> Option<&IndexEntry> {
        self.entries
            .iter()
            .find(|entry| entry.path == path && entry.stage == stage)
    }

    /// Inserts or replaces an entry, returning the previous one.
    pub fn insert(&mut self, entry: IndexEntry) -> Result<Option<IndexEntry>> {
        validate_path(&entry.path)?;
        if entry.stage > 3 {
            return Err(Error::corrupt("index entry has an invalid merge stage"));
        }

        let path = entry.path.clone();
        let stage = entry.stage;

        if let Some(index) = self
            .entries
            .iter()
            .position(|current| current.path == path && current.stage == stage)
        {
            Ok(Some(std::mem::replace(&mut self.entries[index], entry)))
        } else {
            self.entries.push(entry);
            Ok(None)
        }
    }

    /// Removes all entries for a path.
    pub fn remove_path(&mut self, path: &[u8]) -> Vec<IndexEntry> {
        let mut removed = Vec::new();
        self.entries.retain(|entry| {
            if entry.path == path {
                removed.push(entry.clone());
                false
            } else {
                true
            }
        });
        removed
    }

    /// Sorts entries by path bytes and merge stage.
    pub fn sort(&mut self) {
        self.entries.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then(left.stage.cmp(&right.stage))
        });
    }

    /// Returns whether any entry has a non-zero merge stage.
    pub fn has_conflicts(&self) -> bool {
        self.entries.iter().any(|entry| entry.stage != 0)
    }

    /// Reads an index file, returning `None` when it does not exist.
    pub fn read_from(path: &Path) -> Result<Option<Self>> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(Error::io(path.display().to_string(), error)),
        };
        Self::parse(&bytes).map(Some)
    }

    /// Writes the index atomically.
    pub fn write_to(&self, path: &Path) -> Result<()> {
        let bytes = self.serialize()?;
        cvx_core::fs::write_atomic(path, &bytes, Some(0o644))
    }

    /// Parses a complete index file.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 12 + CHECKSUM_LEN {
            return Err(Error::corrupt("index file is too short"));
        }

        let payload_len = bytes.len() - CHECKSUM_LEN;
        let expected: [u8; 20] = Sha1::digest(&bytes[..payload_len]).into();
        if expected.as_slice() != &bytes[payload_len..] {
            return Err(Error::corrupt("index checksum does not match"));
        }

        let mut reader = Reader::new(&bytes[..payload_len]);
        if reader.take(4)? != MAGIC {
            return Err(Error::corrupt("index has an invalid signature"));
        }

        let version = reader.u32()?;
        if !(2..=4).contains(&version) {
            return Err(Error::unsupported(format!("index version {version}")));
        }

        let count = reader.u32()? as usize;
        if count > reader.remaining() / 62 + 1 {
            return Err(Error::corrupt("index entry count is implausible"));
        }

        let mut entries = Vec::with_capacity(count);
        let mut previous_path = Vec::new();

        for _ in 0..count {
            let entry_start = reader.position();

            let ctime_sec = reader.u32()?;
            let ctime_nsec = reader.u32()?;
            let mtime_sec = reader.u32()?;
            let mtime_nsec = reader.u32()?;
            let dev = reader.u32()?;
            let ino = reader.u32()?;
            let mode = reader.u32()?;
            let uid = reader.u32()?;
            let gid = reader.u32()?;
            let size = reader.u32()?;
            let oid = ObjectId::from_slice(reader.take(20)?).map_err(|error| {
                Error::corrupt(format!("index contains an invalid object id: {error}"))
            })?;
            let flags = reader.u16()?;

            let extended = flags & 0x4000 != 0;
            if extended && version < 3 {
                return Err(Error::corrupt(
                    "index entry uses extended flags in version 2",
                ));
            }
            let extended_flags = if extended { reader.u16()? } else { 0 };

            let name_len = (flags & 0x0fff) as usize;
            let path = if version == 4 {
                let strip = reader.varint()?;
                let strip = usize::try_from(strip)
                    .map_err(|_| Error::corrupt("index path prefix length is too large"))?;
                if strip > previous_path.len() {
                    return Err(Error::corrupt(
                        "index path prefix is longer than the previous path",
                    ));
                }
                let suffix = reader.cstring()?;
                let mut path = previous_path[..previous_path.len() - strip].to_vec();
                path.extend_from_slice(suffix);
                path
            } else if name_len < 0x0fff {
                let path = reader.take(name_len)?.to_vec();
                if reader.u8()? != 0 {
                    return Err(Error::corrupt("index path is not NUL-terminated"));
                }
                path
            } else {
                reader.cstring()?.to_vec()
            };

            if version != 4 {
                let consumed = reader.position() - entry_start;
                let padding = (8 - (consumed % 8)) % 8;
                for _ in 0..padding {
                    if reader.u8()? != 0 {
                        return Err(Error::corrupt("index entry has non-zero padding"));
                    }
                }
            }

            validate_path(&path)?;
            entries.push(IndexEntry {
                ctime_sec,
                ctime_nsec,
                mtime_sec,
                mtime_nsec,
                dev,
                ino,
                mode,
                uid,
                gid,
                size,
                oid,
                stage: ((flags >> 12) & 0x3) as u8,
                assume_valid: flags & 0x8000 != 0,
                extended_flags,
                path: path.clone(),
            });
            previous_path = path;
        }

        while reader.remaining() > 0 {
            if reader.remaining() < 8 {
                return Err(Error::corrupt("index has a truncated extension"));
            }
            let signature = reader.take(4)?.to_vec();
            let length = reader.u32()? as usize;
            if reader.remaining() < length {
                return Err(Error::corrupt("index has a truncated extension payload"));
            }
            reader.take(length)?;

            if signature[0].is_ascii_lowercase() {
                return Err(Error::unsupported(format!(
                    "required index extension '{}'",
                    String::from_utf8_lossy(&signature)
                )));
            }
        }

        validate_sorted(&entries)?;

        Ok(Self { version, entries })
    }

    /// Serializes the index, writing version 2 or 3 depending on entry flags.
    pub fn serialize(&self) -> Result<Vec<u8>> {
        let mut entries = self.entries.clone();
        entries.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then(left.stage.cmp(&right.stage))
        });
        validate_sorted(&entries)?;

        let version: u32 = if entries.iter().any(|entry| entry.extended_flags != 0) {
            3
        } else {
            2
        };

        let mut output = Vec::new();
        output.extend_from_slice(MAGIC);
        output.extend_from_slice(&version.to_be_bytes());
        output.extend_from_slice(&(entries.len() as u32).to_be_bytes());

        for entry in &entries {
            validate_path(&entry.path)?;
            if entry.stage > 3 {
                return Err(Error::corrupt("index entry has an invalid merge stage"));
            }

            let start = output.len();

            for field in [
                entry.ctime_sec,
                entry.ctime_nsec,
                entry.mtime_sec,
                entry.mtime_nsec,
                entry.dev,
                entry.ino,
                entry.mode,
                entry.uid,
                entry.gid,
                entry.size,
            ] {
                output.extend_from_slice(&field.to_be_bytes());
            }
            output.extend_from_slice(entry.oid.as_bytes());

            let name_len = entry.path.len().min(0x0fff) as u16;
            let mut flags = name_len | ((u16::from(entry.stage)) << 12);
            if entry.assume_valid {
                flags |= 0x8000;
            }
            if entry.extended_flags != 0 {
                flags |= 0x4000;
            }
            output.extend_from_slice(&flags.to_be_bytes());
            if entry.extended_flags != 0 {
                output.extend_from_slice(&entry.extended_flags.to_be_bytes());
            }

            output.extend_from_slice(&entry.path);
            output.push(0);

            let consumed = output.len() - start;
            let padding = (8 - (consumed % 8)) % 8;
            output.resize(output.len() + padding, 0);
        }

        let checksum: [u8; 20] = Sha1::digest(&output).into();
        output.extend_from_slice(&checksum);
        Ok(output)
    }
}

fn validate_sorted(entries: &[IndexEntry]) -> Result<()> {
    for pair in entries.windows(2) {
        let left = (pair[0].path.as_slice(), pair[0].stage);
        let right = (pair[1].path.as_slice(), pair[1].stage);
        if left >= right {
            return Err(Error::corrupt(
                "index entries are not sorted by path and stage",
            ));
        }
    }
    Ok(())
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn position(&self) -> usize {
        self.position
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.position)
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(length)
            .ok_or_else(|| Error::corrupt("index offset overflow"))?;
        if end > self.bytes.len() {
            return Err(Error::corrupt("index is truncated"));
        }
        let slice = &self.bytes[self.position..end];
        self.position = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        let bytes: [u8; 2] = self.take(2)?.try_into().expect("length checked");
        Ok(u16::from_be_bytes(bytes))
    }

    fn u32(&mut self) -> Result<u32> {
        let bytes: [u8; 4] = self.take(4)?.try_into().expect("length checked");
        Ok(u32::from_be_bytes(bytes))
    }

    fn cstring(&mut self) -> Result<&'a [u8]> {
        let remaining = &self.bytes[self.position..];
        let Some(end) = remaining.iter().position(|byte| *byte == 0) else {
            return Err(Error::corrupt("index string is not NUL-terminated"));
        };
        let value = &remaining[..end];
        self.position += end + 1;
        Ok(value)
    }

    /// Git's offset-encoded varint, used by index version 4.
    fn varint(&mut self) -> Result<u64> {
        let mut byte = self.u8()?;
        let mut value = u64::from(byte & 0x7f);

        while byte & 0x80 != 0 {
            value = value
                .checked_add(1)
                .ok_or_else(|| Error::corrupt("index varint overflow"))?;
            byte = self.u8()?;
            value = value
                .checked_shl(7)
                .ok_or_else(|| Error::corrupt("index varint overflow"))?
                + u64::from(byte & 0x7f);
        }

        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{MODE_EXECUTABLE, MODE_FILE};

    fn oid(value: u8) -> ObjectId {
        ObjectId::from_bytes([value; 20])
    }

    #[test]
    fn roundtrips_empty_index() {
        let index = Index::new();
        let bytes = index.serialize().expect("serialize");
        assert_eq!(&bytes[..4], b"DIRC");
        let parsed = Index::parse(&bytes).expect("parse");
        assert!(parsed.is_empty());
    }

    #[test]
    fn roundtrips_entries_and_modes() {
        let mut index = Index::new();
        index
            .insert(IndexEntry::new(
                b"src/main.rs".to_vec(),
                oid(0x11),
                MODE_FILE,
            ))
            .expect("insert");
        index
            .insert(IndexEntry::new(
                b"scripts/run".to_vec(),
                oid(0x22),
                MODE_EXECUTABLE,
            ))
            .expect("insert");

        let bytes = index.serialize().expect("serialize");
        let parsed = Index::parse(&bytes).expect("parse");

        assert_eq!(parsed.version(), 2);
        assert_eq!(parsed.entries().len(), 2);
        assert!(parsed.get(b"src/main.rs").unwrap().is_file());
        assert!(parsed.get(b"scripts/run").unwrap().is_executable());
    }

    #[test]
    fn preserves_stage_and_extended_flags() {
        let mut entry = IndexEntry::new(b"conflicted".to_vec(), oid(0x33), MODE_FILE);
        entry.stage = 2;
        entry.extended_flags = 0x4000;
        entry.assume_valid = true;

        let mut index = Index::new();
        index.insert(entry).expect("insert");
        let bytes = index.serialize().expect("serialize");
        let parsed = Index::parse(&bytes).expect("parse");
        let entry = parsed.get_stage(b"conflicted", 2).expect("entry");

        assert!(entry.skip_worktree());
        assert!(entry.assume_valid);
    }

    #[test]
    fn rejects_bad_checksum() {
        let index = Index::new();
        let mut bytes = index.serialize().expect("serialize");
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        assert!(Index::parse(&bytes).is_err());
    }

    #[test]
    fn rejects_required_extensions() {
        let index = Index::new();
        let mut bytes = index.serialize().expect("serialize");
        bytes.truncate(bytes.len() - CHECKSUM_LEN);

        bytes.extend_from_slice(b"link");
        bytes.extend_from_slice(&0u32.to_be_bytes());
        let digest: [u8; 20] = Sha1::digest(&bytes).into();
        bytes.extend_from_slice(&digest);

        assert!(matches!(
            Index::parse(&bytes),
            Err(Error::Unsupported { .. })
        ));
    }

    #[test]
    fn reads_version_four_prefix_compression() {
        let mut payload = Vec::new();
        payload.extend_from_slice(MAGIC);
        payload.extend_from_slice(&4u32.to_be_bytes());
        payload.extend_from_slice(&2u32.to_be_bytes());

        for (strip, suffix, full_path) in [
            (0u8, b"src/lib.rs".as_slice(), b"src/lib.rs".as_slice()),
            (6u8, b"main.rs".as_slice(), b"src/main.rs".as_slice()),
        ] {
            for field in [0u32; 10] {
                payload.extend_from_slice(&field.to_be_bytes());
            }
            payload.extend_from_slice(oid(0x44).as_bytes());

            let flags = full_path.len().min(0x0fff) as u16;
            payload.extend_from_slice(&flags.to_be_bytes());
            payload.push(strip);
            payload.extend_from_slice(suffix);
            payload.push(0);
        }

        let checksum: [u8; 20] = Sha1::digest(&payload).into();
        payload.extend_from_slice(&checksum);

        let index = Index::parse(&payload).expect("parse v4");
        assert_eq!(index.version(), 4);
        assert_eq!(index.entries()[0].path, b"src/lib.rs");
        assert_eq!(index.entries()[1].path, b"src/main.rs");
    }

    #[test]
    fn rejects_unsorted_entries() {
        let mut index = Index::new();
        index
            .insert(IndexEntry::new(b"b".to_vec(), oid(1), MODE_FILE))
            .expect("insert");
        index
            .insert(IndexEntry::new(b"a".to_vec(), oid(2), MODE_FILE))
            .expect("insert");
        // serialize sorts internally, so it succeeds.
        assert!(index.serialize().is_ok());

        // A manually built unsorted entry list must be rejected on parse.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&2u32.to_be_bytes());
        bytes.extend_from_slice(&2u32.to_be_bytes());
        for path in [b"b".as_slice(), b"a".as_slice()] {
            for field in [0u32; 10] {
                bytes.extend_from_slice(&field.to_be_bytes());
            }
            bytes.extend_from_slice(oid(1).as_bytes());
            let flags = path.len() as u16;
            bytes.extend_from_slice(&flags.to_be_bytes());
            bytes.extend_from_slice(path);
            bytes.push(0);
            let padding = (8 - ((62 + path.len() + 1) % 8)) % 8;
            bytes.resize(bytes.len() + padding, 0);
        }
        let checksum: [u8; 20] = Sha1::digest(&bytes).into();
        bytes.extend_from_slice(&checksum);

        assert!(Index::parse(&bytes).is_err());
    }
}
