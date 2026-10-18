use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use crate::error::{Error, IoResultExt, Result};

/// Creates a directory and all of its parents.
pub fn ensure_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).path_context(path)
}

/// Reads a complete file into memory.
pub fn read(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).path_context(path)
}

/// Reads a complete UTF-8 file into memory.
pub fn read_to_string(path: &Path) -> Result<String> {
    fs::read_to_string(path).path_context(path)
}

/// Writes a file, creating its parent directory when necessary.
pub fn write(path: &Path, contents: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    fs::write(path, contents).path_context(path)
}

/// Atomically replaces a file by writing a temporary sibling and renaming it.
///
/// On Unix, `mode` controls the permissions of the resulting file. On other
/// platforms it is ignored.
pub fn write_atomic(path: &Path, contents: &[u8], mode: Option<u32>) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        Error::Message(format!(
            "cannot write {}: path has no parent",
            path.display()
        ))
    })?;
    ensure_dir(parent)?;

    let mut temp = tempfile::Builder::new()
        .prefix(".cvx-tmp-")
        .tempfile_in(parent)
        .path_context(parent)?;

    temp.write_all(contents).path_context(temp.path())?;
    temp.as_file().sync_all().path_context(temp.path())?;

    #[cfg(unix)]
    if let Some(mode) = mode {
        use std::os::unix::fs::PermissionsExt;
        temp.as_file()
            .set_permissions(fs::Permissions::from_mode(mode))
            .path_context(temp.path())?;
    }
    #[cfg(not(unix))]
    let _ = mode;

    temp.persist(path)
        .map_err(|error| Error::io(format!("failed to replace {}", path.display()), error.error))?;

    sync_parent(path)?;
    Ok(())
}

/// Removes a directory tree if it exists.
pub fn remove_dir_all_if_exists(path: &Path) -> Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Error::io(
            format!("failed to remove {}", path.display()),
            error,
        )),
    }
}

/// Flushes the parent directory entry to disk on Unix.
#[cfg(unix)]
fn sync_parent(path: &Path) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let directory = File::open(parent).path_context(parent)?;
    directory.sync_all().path_context(parent)
}

#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_creates_and_replaces_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("nested").join("file.txt");

        write_atomic(&path, b"first", Some(0o644)).expect("first write");
        assert_eq!(read(&path).expect("read"), b"first");

        write_atomic(&path, b"second", Some(0o644)).expect("second write");
        assert_eq!(read(&path).expect("read"), b"second");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_applies_mode() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("secret");
        write_atomic(&path, b"content", Some(0o600)).expect("write");

        let mode = fs::metadata(&path).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
