use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use cvx_core::{Error, Result};

use crate::Index;

/// Exclusive lock for updating the index.
///
/// The lock follows Git's `index.lock` protocol: creation is exclusive, the new
/// index is written to the lock file, and a rename publishes it atomically.
#[derive(Debug)]
pub struct IndexLock {
    lock_path: PathBuf,
    index_path: PathBuf,
    file: Option<File>,
    committed: bool,
}

impl IndexLock {
    /// Acquires `index.lock` inside a control directory.
    pub fn acquire(git_dir: &Path) -> Result<Self> {
        let lock_path = git_dir.join("index.lock");
        let index_path = git_dir.join("index");

        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    Error::Message(format!(
                        "unable to create '{}': another CVX process may be running",
                        lock_path.display()
                    ))
                } else {
                    Error::io(lock_path.display().to_string(), error)
                }
            })?;

        Ok(Self {
            lock_path,
            index_path,
            file: Some(file),
            committed: false,
        })
    }

    /// Writes the index and atomically replaces the current one.
    pub fn commit(mut self, index: &Index) -> Result<()> {
        let bytes = index.serialize()?;
        let mut file = self
            .file
            .take()
            .ok_or_else(|| Error::Message("index lock is already closed".to_owned()))?;

        file.write_all(&bytes)
            .map_err(|error| Error::io(self.lock_path.display().to_string(), error))?;
        file.sync_all()
            .map_err(|error| Error::io(self.lock_path.display().to_string(), error))?;
        drop(file);

        cvx_core::failpoint::hit("index.before-publish")?;
        std::fs::rename(&self.lock_path, &self.index_path)
            .map_err(|error| Error::io(self.index_path.display().to_string(), error))?;
        cvx_core::fs::sync_parent(&self.index_path)?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for IndexLock {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(&self.lock_path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::IndexEntry;
    use cvx_object::ObjectId;

    #[test]
    fn commits_an_index_atomically() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut index = Index::new();
        index
            .insert(IndexEntry::new(
                b"file".to_vec(),
                ObjectId::from_bytes([1; 20]),
                crate::entry::MODE_FILE,
            ))
            .expect("insert");

        let lock = IndexLock::acquire(temp.path()).expect("lock");
        lock.commit(&index).expect("commit");

        assert!(!temp.path().join("index.lock").exists());
        let loaded = Index::read_from(&temp.path().join("index"))
            .expect("read")
            .expect("present");
        assert_eq!(loaded.entries().len(), 1);
    }

    #[test]
    fn releases_lock_on_drop() {
        let temp = tempfile::tempdir().expect("tempdir");
        {
            let _lock = IndexLock::acquire(temp.path()).expect("lock");
            assert!(temp.path().join("index.lock").exists());
        }
        assert!(!temp.path().join("index.lock").exists());
    }

    #[test]
    fn rejects_concurrent_lock() {
        let temp = tempfile::tempdir().expect("tempdir");
        let _lock = IndexLock::acquire(temp.path()).expect("first");
        assert!(IndexLock::acquire(temp.path()).is_err());
    }
}
