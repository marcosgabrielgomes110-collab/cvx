use cvx_core::{Error, Result};

use crate::{HashAlgorithm, ObjectId};

/// Mode for a directory tree entry.
pub const MODE_TREE: u32 = 0o040_000;
/// Mode for a regular file.
pub const MODE_FILE: u32 = 0o100_644;
/// Mode for an executable file.
pub const MODE_EXECUTABLE: u32 = 0o100_755;
/// Mode for a symbolic link.
pub const MODE_SYMLINK: u32 = 0o120_000;
/// Mode for a submodule.
pub const MODE_GITLINK: u32 = 0o160_000;

/// One entry in a tree object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    /// Entry mode, including file type bits.
    pub mode: u32,
    /// Entry name, without slashes.
    pub name: Vec<u8>,
    /// Referenced object.
    pub oid: ObjectId,
}

impl TreeEntry {
    /// Creates a tree entry.
    pub fn new(mode: u32, name: impl Into<Vec<u8>>, oid: ObjectId) -> Self {
        Self {
            mode,
            name: name.into(),
            oid,
        }
    }

    /// Returns whether this entry is a directory.
    pub fn is_tree(&self) -> bool {
        self.mode & 0o170_000 == MODE_TREE
    }

    /// Returns whether this entry is a symbolic link.
    pub fn is_symlink(&self) -> bool {
        self.mode & 0o170_000 == MODE_SYMLINK
    }

    /// Returns whether this entry is a submodule.
    pub fn is_gitlink(&self) -> bool {
        self.mode & 0o170_000 == MODE_GITLINK
    }
}

/// A parsed tree object.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tree {
    entries: Vec<TreeEntry>,
}

impl Tree {
    /// Creates a tree and sorts its entries into Git order.
    pub fn new(mut entries: Vec<TreeEntry>) -> Self {
        entries.sort_by_key(tree_key);
        Self { entries }
    }

    /// Returns the entries in storage order.
    pub fn entries(&self) -> &[TreeEntry] {
        &self.entries
    }

    /// Parses SHA-1 tree object content.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        Self::parse_with_algorithm(bytes, HashAlgorithm::Sha1)
    }

    /// Parses tree object content using the repository hash algorithm.
    pub fn parse_with_algorithm(bytes: &[u8], algorithm: HashAlgorithm) -> Result<Self> {
        let mut entries = Vec::new();
        let mut position = 0;

        while position < bytes.len() {
            let mode_end = bytes[position..]
                .iter()
                .position(|byte| *byte == b' ')
                .map(|offset| position + offset)
                .ok_or_else(|| Error::corrupt("tree entry is missing a mode separator"))?;
            let mode = parse_mode(&bytes[position..mode_end])?;

            let name_start = mode_end + 1;
            let name_end = bytes[name_start..]
                .iter()
                .position(|byte| *byte == 0)
                .map(|offset| name_start + offset)
                .ok_or_else(|| Error::corrupt("tree entry is missing a name terminator"))?;
            let name = bytes[name_start..name_end].to_vec();
            validate_name(&name)?;

            let oid_start = name_end + 1;
            let oid_end = oid_start + algorithm.oid_len();
            if oid_end > bytes.len() {
                return Err(Error::corrupt("tree entry is missing an object id"));
            }
            let oid = ObjectId::from_slice(&bytes[oid_start..oid_end])
                .map_err(|error| Error::corrupt(format!("invalid tree object id: {error}")))?;

            entries.push(TreeEntry { mode, name, oid });
            position = oid_end;
        }

        Ok(Self { entries })
    }

    /// Serializes the tree using SHA-1 identifiers.
    pub fn serialize(&self) -> Vec<u8> {
        self.serialize_with_algorithm(HashAlgorithm::Sha1)
    }

    /// Serializes the tree using the repository hash algorithm.
    pub fn serialize_with_algorithm(&self, _algorithm: HashAlgorithm) -> Vec<u8> {
        let mut entries = self.entries.clone();
        entries.sort_by_key(tree_key);

        let mut output = Vec::new();
        for entry in entries {
            output.extend_from_slice(format!("{:o}", entry.mode).as_bytes());
            output.push(b' ');
            output.extend_from_slice(&entry.name);
            output.push(0);
            output.extend_from_slice(entry.oid.as_bytes());
        }
        output
    }

    /// Returns whether the tree has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn parse_mode(bytes: &[u8]) -> Result<u32> {
    let text = std::str::from_utf8(bytes).map_err(|_| Error::corrupt("tree mode is not ASCII"))?;
    let mode =
        u32::from_str_radix(text, 8).map_err(|_| Error::corrupt("tree mode is not octal"))?;

    match mode & 0o170_000 {
        0o040_000 | 0o100_000 | 0o120_000 | 0o160_000 => Ok(mode),
        _ => Err(Error::corrupt(format!(
            "unsupported tree entry mode {mode:o}"
        ))),
    }
}

fn validate_name(name: &[u8]) -> Result<()> {
    if name.is_empty() || name == b"." || name == b".." || name.contains(&b'/') || name.contains(&0)
    {
        return Err(Error::corrupt(format!(
            "invalid tree entry name: {}",
            String::from_utf8_lossy(name)
        )));
    }
    Ok(())
}

/// Git sorts trees as if directory names had a trailing slash.
fn tree_key(entry: &TreeEntry) -> Vec<u8> {
    let mut key = entry.name.clone();
    if entry.is_tree() {
        key.push(b'/');
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oid(value: u8) -> ObjectId {
        ObjectId::from_bytes([value; 20])
    }

    #[test]
    fn roundtrips_tree_entries() {
        let tree = Tree::new(vec![
            TreeEntry::new(MODE_TREE, b"src".to_vec(), oid(0x22)),
            TreeEntry::new(MODE_FILE, b"README.md".to_vec(), oid(0x11)),
        ]);
        let bytes = tree.serialize();
        let parsed = Tree::parse(&bytes).expect("parse");

        assert_eq!(parsed.entries()[0].name, b"README.md");
        assert_eq!(parsed.entries()[1].name, b"src");
        assert!(parsed.entries()[1].is_tree());
    }

    #[test]
    fn sorts_like_git() {
        let tree = Tree::new(vec![
            TreeEntry::new(MODE_TREE, b"foo".to_vec(), oid(1)),
            TreeEntry::new(MODE_FILE, b"foo.bar".to_vec(), oid(2)),
            TreeEntry::new(MODE_FILE, b"foo.txt".to_vec(), oid(3)),
        ]);
        let names: Vec<&[u8]> = tree
            .entries()
            .iter()
            .map(|entry| entry.name.as_slice())
            .collect();
        assert_eq!(names, vec![b"foo.bar".as_slice(), b"foo.txt", b"foo"]);
    }

    #[test]
    fn rejects_malformed_trees() {
        assert!(Tree::parse(b"100644 file").is_err());
        assert!(Tree::parse(b"100644 file\0short").is_err());
        assert!(Tree::parse(b"100644 bad/name\0").is_err());
    }
}
