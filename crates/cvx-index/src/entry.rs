use std::fs::Metadata;
use std::path::{Path, PathBuf};

use cvx_core::{Error, Result};
use cvx_object::ObjectId;

/// Regular file mode.
pub const MODE_FILE: u32 = 0o100_644;
/// Executable file mode.
pub const MODE_EXECUTABLE: u32 = 0o100_755;
/// Symbolic link mode.
pub const MODE_SYMLINK: u32 = 0o120_000;
/// Directory (tree) mode.
pub const MODE_TREE: u32 = 0o040_000;
/// Submodule (gitlink) mode.
pub const MODE_GITLINK: u32 = 0o160_000;

/// One entry in the staging area.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    /// Creation time seconds.
    pub ctime_sec: u32,
    /// Creation time nanoseconds.
    pub ctime_nsec: u32,
    /// Modification time seconds.
    pub mtime_sec: u32,
    /// Modification time nanoseconds.
    pub mtime_nsec: u32,
    /// Device number.
    pub dev: u32,
    /// Inode number.
    pub ino: u32,
    /// File mode, including type bits.
    pub mode: u32,
    /// Owner user id.
    pub uid: u32,
    /// Owner group id.
    pub gid: u32,
    /// File size modulo the index's 32-bit field.
    pub size: u32,
    /// Blob identifier.
    pub oid: ObjectId,
    /// Merge stage (0 for normal entries).
    pub stage: u8,
    /// `assume-unchanged` bit.
    pub assume_valid: bool,
    /// Raw extended flags (skip-worktree, intent-to-add, reserved bits).
    pub extended_flags: u16,
    /// Repository-relative path bytes.
    pub path: Vec<u8>,
}

impl IndexEntry {
    /// Creates an entry with zeroed stat information.
    pub fn new(path: impl Into<Vec<u8>>, oid: ObjectId, mode: u32) -> Self {
        Self {
            ctime_sec: 0,
            ctime_nsec: 0,
            mtime_sec: 0,
            mtime_nsec: 0,
            dev: 0,
            ino: 0,
            mode,
            uid: 0,
            gid: 0,
            size: 0,
            oid,
            stage: 0,
            assume_valid: false,
            extended_flags: 0,
            path: path.into(),
        }
    }

    /// Creates an entry from filesystem metadata.
    pub fn from_metadata(
        path: impl Into<Vec<u8>>,
        oid: ObjectId,
        metadata: &Metadata,
        filemode: bool,
    ) -> Result<Self> {
        let path = path.into();
        validate_path(&path)?;

        let file_type = metadata.file_type();
        let mode = if file_type.is_symlink() {
            MODE_SYMLINK
        } else if file_type.is_file() {
            if filemode && is_executable(metadata) {
                MODE_EXECUTABLE
            } else {
                MODE_FILE
            }
        } else if file_type.is_dir() {
            return Err(Error::InvalidPath {
                path: String::from_utf8_lossy(&path).into_owned(),
            });
        } else {
            return Err(Error::Message(format!(
                "unsupported file type: {}",
                String::from_utf8_lossy(&path)
            )));
        };

        let size = u32::try_from(metadata.len()).map_err(|_| {
            Error::Message(format!(
                "file is too large for the Git index: {}",
                String::from_utf8_lossy(&path)
            ))
        })?;

        #[cfg(unix)]
        let (ctime_sec, ctime_nsec, mtime_sec, mtime_nsec, dev, ino, uid, gid) = {
            use std::os::unix::fs::MetadataExt;
            (
                metadata.ctime() as u32,
                metadata.ctime_nsec() as u32,
                metadata.mtime() as u32,
                metadata.mtime_nsec() as u32,
                metadata.dev() as u32,
                metadata.ino() as u32,
                metadata.uid(),
                metadata.gid(),
            )
        };
        #[cfg(not(unix))]
        let (ctime_sec, ctime_nsec, mtime_sec, mtime_nsec, dev, ino, uid, gid) = {
            let (seconds, nanos) = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| (duration.as_secs() as u32, duration.subsec_nanos()))
                .unwrap_or((0, 0));
            (0, 0, seconds, nanos, 0, 0, 0, 0)
        };

        Ok(Self {
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
            stage: 0,
            assume_valid: false,
            extended_flags: 0,
            path,
        })
    }

    /// Returns the path as lossy UTF-8 for display.
    pub fn path_lossy(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.path)
    }

    /// Converts the path bytes to a platform path.
    pub fn path_buf(&self) -> Result<PathBuf> {
        path_from_bytes(&self.path)
    }

    /// Returns whether this is a regular file.
    pub fn is_file(&self) -> bool {
        self.mode & 0o170_000 == 0o100_000
    }

    /// Returns whether this is a symbolic link.
    pub fn is_symlink(&self) -> bool {
        self.mode & 0o170_000 == 0o120_000
    }

    /// Returns whether this is a submodule entry.
    pub fn is_gitlink(&self) -> bool {
        self.mode & 0o170_000 == 0o160_000
    }

    /// Returns whether the executable bit is set.
    pub fn is_executable(&self) -> bool {
        self.mode & 0o111 != 0
    }

    /// Returns the skip-worktree bit.
    pub fn skip_worktree(&self) -> bool {
        self.extended_flags & 0x4000 != 0
    }

    /// Returns the intent-to-add bit.
    pub fn intent_to_add(&self) -> bool {
        self.extended_flags & 0x2000 != 0
    }
}

/// Validates a repository-relative path stored in the index.
pub fn validate_path(path: &[u8]) -> Result<()> {
    let invalid = || {
        Err(Error::InvalidPath {
            path: String::from_utf8_lossy(path).into_owned(),
        })
    };

    if path.is_empty() || path[0] == b'/' || path.contains(&0) {
        return invalid();
    }
    if path.ends_with(b"/") {
        return invalid();
    }

    for component in path.split(|byte| *byte == b'/') {
        if component.is_empty()
            || component == b"."
            || component == b".."
            || component == b".git"
            || component == b".cvx"
        {
            return invalid();
        }
    }

    Ok(())
}

#[cfg(unix)]
fn is_executable(metadata: &Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_metadata: &Metadata) -> bool {
    false
}

/// Converts repository-relative path bytes into a platform path.
#[cfg(unix)]
pub fn path_from_bytes(bytes: &[u8]) -> Result<PathBuf> {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    Ok(PathBuf::from(OsStr::from_bytes(bytes)))
}

/// Converts repository-relative path bytes into a platform path.
#[cfg(not(unix))]
pub fn path_from_bytes(bytes: &[u8]) -> Result<PathBuf> {
    let path = std::str::from_utf8(bytes).map_err(|_| Error::InvalidPath {
        path: String::from_utf8_lossy(bytes).into_owned(),
    })?;
    Ok(PathBuf::from(path))
}

/// Converts a platform path into repository-relative bytes.
pub fn path_to_bytes(path: &Path) -> Result<Vec<u8>> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Ok(path.as_os_str().as_bytes().to_vec())
    }
    #[cfg(not(unix))]
    {
        let path = path.to_str().ok_or_else(|| Error::InvalidPath {
            path: path.display().to_string(),
        })?;
        Ok(path.replace('\\', "/").into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OID: &str = "1111111111111111111111111111111111111111";

    #[test]
    fn validates_paths() {
        validate_path(b"src/main.rs").expect("valid");
        validate_path(b"a").expect("valid");
        assert!(validate_path(b"").is_err());
        assert!(validate_path(b"/absolute").is_err());
        assert!(validate_path(b"../escape").is_err());
        assert!(validate_path(b"a/../b").is_err());
        assert!(validate_path(b"a//b").is_err());
        assert!(validate_path(b"a/").is_err());
        assert!(validate_path(b".git/config").is_err());
        assert!(validate_path(b"nested/.cvx/HEAD").is_err());
    }

    #[test]
    fn reports_modes() {
        let oid = ObjectId::from_hex(OID).unwrap();
        let file = IndexEntry::new(b"file".to_vec(), oid, MODE_FILE);
        let executable = IndexEntry::new(b"exec".to_vec(), oid, MODE_EXECUTABLE);
        let link = IndexEntry::new(b"link".to_vec(), oid, MODE_SYMLINK);
        let gitlink = IndexEntry::new(b"sub".to_vec(), oid, MODE_GITLINK);

        assert!(file.is_file());
        assert!(!file.is_executable());
        assert!(executable.is_executable());
        assert!(link.is_symlink());
        assert!(gitlink.is_gitlink());
    }

    #[test]
    fn roundtrips_path_bytes() {
        let bytes = b"src/arquivo.txt".to_vec();
        let path = path_from_bytes(&bytes).expect("path");
        assert_eq!(path_to_bytes(&path).expect("bytes"), bytes);
    }
}
