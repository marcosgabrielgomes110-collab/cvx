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
    ///
    /// This is a simple replacement used by the first milestone. Reference
    /// transactions with compare-and-swap will be added with commits.
    pub fn write_ref(&self, name: &str, oid: ObjectId) -> Result<()> {
        cvx_core::refname::validate_ref_name(name)?;
        cvx_core::fs::write_atomic(
            &self.git_dir.join(name),
            format!("{oid}\n").as_bytes(),
            Some(0o644),
        )
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
