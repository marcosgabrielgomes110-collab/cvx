use std::path::{Path, PathBuf};

use cvx_core::{Error, Result};
use cvx_object::ObjectId;

use crate::Head;
use crate::packed::read_packed_refs;

const MAX_SYMBOLIC_DEPTH: usize = 8;

/// Access to references stored under a control directory.
#[derive(Debug, Clone)]
pub struct RefStore {
    git_dir: PathBuf,
}

impl RefStore {
    /// Creates a reference store rooted at a control directory.
    pub fn new(git_dir: impl Into<PathBuf>) -> Self {
        Self {
            git_dir: git_dir.into(),
        }
    }

    /// Returns the control directory.
    pub fn git_dir(&self) -> &Path {
        &self.git_dir
    }

    /// Reads and resolves `HEAD`.
    pub fn read_head(&self) -> Result<Head> {
        let path = self.git_dir.join("HEAD");
        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::corrupt(format!(
                    "{} is missing; this is not a valid repository",
                    path.display()
                )));
            }
            Err(error) => return Err(Error::io(path.display().to_string(), error)),
        };

        let contents = contents.trim();
        if let Some(target) = contents.strip_prefix("ref:") {
            let target = target.trim();
            cvx_core::refname::validate_ref_name(target)?;
            let oid = self.read_ref_inner(target, 0)?;
            return Ok(Head::Symbolic {
                target: target.to_owned(),
                oid,
            });
        }

        let oid = ObjectId::from_hex(contents).map_err(|error| {
            Error::corrupt(format!(
                "{} contains an invalid HEAD: {error}",
                path.display()
            ))
        })?;
        Ok(Head::Detached(oid))
    }

    /// Writes a symbolic `HEAD`, used when initializing a repository.
    pub fn write_head_symbolic(&self, target: &str) -> Result<()> {
        cvx_core::refname::validate_ref_name(target)?;
        cvx_core::fs::write_atomic(
            &self.git_dir.join("HEAD"),
            format!("ref: {target}\n").as_bytes(),
            Some(0o644),
        )
    }

    /// Reads a reference, returning `None` when it does not exist.
    pub fn read_ref(&self, name: &str) -> Result<Option<ObjectId>> {
        cvx_core::refname::validate_ref_name(name)?;
        self.read_ref_inner(name, 0)
    }

    /// Resolves the commit `HEAD` points to.
    pub fn resolve_head(&self) -> Result<Option<ObjectId>> {
        Ok(self.read_head()?.oid())
    }

    /// Writes a direct reference.
    pub fn write_ref(&self, name: &str, oid: ObjectId) -> Result<()> {
        cvx_core::refname::validate_ref_name(name)?;
        cvx_core::fs::write_atomic(
            &self.git_dir.join(name),
            format!("{oid}\n").as_bytes(),
            Some(0o644),
        )
    }

    /// Writes a detached `HEAD`.
    pub fn write_head_detached(&self, oid: ObjectId) -> Result<()> {
        cvx_core::fs::write_atomic(
            &self.git_dir.join("HEAD"),
            format!("{oid}\n").as_bytes(),
            Some(0o644),
        )
    }

    /// Updates a reference with compare-and-swap semantics.
    ///
    /// `expected_old` is `None` for an unborn branch. The update fails if the
    /// current value differs, preventing concurrent commits from silently
    /// overwriting each other.
    pub fn update_ref(
        &self,
        name: &str,
        new_oid: ObjectId,
        expected_old: Option<ObjectId>,
    ) -> Result<()> {
        cvx_core::refname::validate_ref_name(name)?;
        let ref_path = self.git_dir.join(name);
        let lock_path = self.git_dir.join(format!("{name}.lock"));

        if let Some(parent) = lock_path.parent() {
            cvx_core::fs::ensure_dir(parent)?;
        }

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    Error::Message(format!(
                        "unable to create '{}': another process may be updating this reference",
                        lock_path.display()
                    ))
                } else {
                    Error::io(lock_path.display().to_string(), error)
                }
            })?;

        let current = match self.read_ref(name) {
            Ok(current) => current,
            Err(error) => {
                let _ = std::fs::remove_file(&lock_path);
                return Err(error);
            }
        };
        if current != expected_old {
            let _ = std::fs::remove_file(&lock_path);
            return Err(Error::Message(format!(
                "failed to update '{name}': reference changed concurrently"
            )));
        }

        use std::io::Write;
        let write_result = (|| -> Result<()> {
            file.write_all(format!("{new_oid}\n").as_bytes())
                .map_err(|error| Error::io(lock_path.display().to_string(), error))?;
            file.sync_all()
                .map_err(|error| Error::io(lock_path.display().to_string(), error))?;
            drop(file);
            std::fs::rename(&lock_path, &ref_path)
                .map_err(|error| Error::io(ref_path.display().to_string(), error))?;
            cvx_core::fs::sync_parent(&ref_path)
        })();

        if write_result.is_err() {
            let _ = std::fs::remove_file(&lock_path);
        }
        write_result
    }

    fn read_ref_inner(&self, name: &str, depth: usize) -> Result<Option<ObjectId>> {
        if depth > MAX_SYMBOLIC_DEPTH {
            return Err(Error::corrupt(format!(
                "symbolic reference chain is too deep at {name}"
            )));
        }

        let path = self.git_dir.join(name);
        match std::fs::read_to_string(&path) {
            Ok(contents) => {
                let contents = contents.trim();
                if contents.is_empty() {
                    return Err(Error::corrupt(format!("{} is empty", path.display())));
                }

                if let Some(target) = contents.strip_prefix("ref:") {
                    let target = target.trim();
                    cvx_core::refname::validate_ref_name(target)?;
                    return self.read_ref_inner(target, depth + 1);
                }

                ObjectId::from_hex(contents).map(Some).map_err(|error| {
                    Error::corrupt(format!(
                        "{} contains an invalid object id: {error}",
                        path.display()
                    ))
                })
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(read_packed_refs(&self.git_dir)?.get(name).copied())
            }
            Err(error) => Err(Error::io(path.display().to_string(), error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, RefStore) {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = RefStore::new(temp.path());
        (temp, store)
    }

    #[test]
    fn reads_unborn_head() {
        let (_temp, store) = store();
        store
            .write_head_symbolic("refs/heads/main")
            .expect("write HEAD");

        let head = store.read_head().expect("read HEAD");
        assert_eq!(
            head,
            Head::Symbolic {
                target: "refs/heads/main".to_owned(),
                oid: None
            }
        );
        assert!(head.is_unborn());
    }

    #[test]
    fn resolves_symbolic_head() {
        let (_temp, store) = store();
        let oid = ObjectId::from_hex("1111111111111111111111111111111111111111").unwrap();
        store
            .write_head_symbolic("refs/heads/main")
            .expect("write HEAD");
        store.write_ref("refs/heads/main", oid).expect("write ref");

        let head = store.read_head().expect("read HEAD");
        assert_eq!(head.oid(), Some(oid));
        assert_eq!(head.target(), Some("refs/heads/main"));
    }

    #[test]
    fn reads_detached_head() {
        let (_temp, store) = store();
        let oid = ObjectId::from_hex("2222222222222222222222222222222222222222").unwrap();
        std::fs::write(store.git_dir().join("HEAD"), format!("{oid}\n")).expect("write");

        assert_eq!(store.read_head().expect("read"), Head::Detached(oid));
    }

    #[test]
    fn falls_back_to_packed_refs() {
        let (_temp, store) = store();
        let oid = ObjectId::from_hex("3333333333333333333333333333333333333333").unwrap();
        std::fs::write(
            store.git_dir().join("packed-refs"),
            format!("{oid} refs/heads/main\n"),
        )
        .expect("write packed refs");

        assert_eq!(store.read_ref("refs/heads/main").expect("read"), Some(oid));
    }

    #[test]
    fn follows_symbolic_loose_refs() {
        let (_temp, store) = store();
        let oid = ObjectId::from_hex("4444444444444444444444444444444444444444").unwrap();
        store.write_ref("refs/heads/main", oid).expect("write ref");
        std::fs::create_dir_all(store.git_dir().join("refs/remotes/origin")).expect("mkdir");
        std::fs::write(
            store.git_dir().join("refs/remotes/origin/HEAD"),
            "ref: refs/heads/main\n",
        )
        .expect("write symbolic");

        assert_eq!(
            store.read_ref("refs/remotes/origin/HEAD").expect("read"),
            Some(oid)
        );
    }

    #[test]
    fn rejects_invalid_head() {
        let (_temp, store) = store();
        std::fs::write(store.git_dir().join("HEAD"), "not a reference\n").expect("write");
        assert!(store.read_head().is_err());
    }
}
