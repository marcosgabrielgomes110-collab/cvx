use std::collections::BTreeMap;
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

    /// Writes a symbolic reference that points at another reference.
    pub fn write_ref_symbolic(&self, name: &str, target: &str) -> Result<()> {
        cvx_core::refname::validate_ref_name(name)?;
        cvx_core::refname::validate_ref_name(target)?;
        cvx_core::fs::write_atomic(
            &self.git_dir.join(name),
            format!("ref: {target}\n").as_bytes(),
            Some(0o644),
        )
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

    /// Lists references under a prefix such as `refs/heads`.
    pub fn list_refs(&self, prefix: &str) -> Result<BTreeMap<String, ObjectId>> {
        let prefix = prefix.trim_end_matches('/');
        if !cvx_core::refname::is_valid_ref_name(&format!("{prefix}/placeholder")) {
            return Err(Error::invalid_ref(prefix));
        }

        let mut refs = BTreeMap::new();
        for (name, oid) in read_packed_refs(&self.git_dir)? {
            if name.starts_with(&format!("{prefix}/")) {
                refs.insert(name, oid);
            }
        }

        let directory = self.git_dir.join(prefix);
        self.collect_loose_refs(&directory, &mut refs, 0)?;
        Ok(refs)
    }

    fn collect_loose_refs(
        &self,
        directory: &Path,
        refs: &mut BTreeMap<String, ObjectId>,
        depth: usize,
    ) -> Result<()> {
        if depth > 16 {
            return Err(Error::corrupt("reference directory is too deep"));
        }

        let entries = match std::fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(Error::io(directory.display().to_string(), error)),
        };

        for entry in entries {
            let entry = entry.map_err(|error| Error::io(directory.display().to_string(), error))?;
            let path = entry.path();
            let metadata = entry
                .metadata()
                .map_err(|error| Error::io(path.display().to_string(), error))?;

            if metadata.is_dir() {
                self.collect_loose_refs(&path, refs, depth + 1)?;
                continue;
            }

            let relative = path
                .strip_prefix(&self.git_dir)
                .map_err(|_| Error::corrupt("reference path escaped the repository"))?;
            let name = relative
                .components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            if name.ends_with(".lock") {
                continue;
            }
            cvx_core::refname::validate_ref_name(&name)?;
            if let Some(oid) = self.read_ref(&name)? {
                refs.insert(name, oid);
            }
        }

        Ok(())
    }

    /// Deletes a reference using compare-and-swap semantics.
    pub fn delete_ref(&self, name: &str, expected_old: Option<ObjectId>) -> Result<()> {
        cvx_core::refname::validate_ref_name(name)?;
        let lock_path = self.git_dir.join(format!("{name}.lock"));
        if let Some(parent) = lock_path.parent() {
            cvx_core::fs::ensure_dir(parent)?;
        }

        let lock = cvx_core::fs::create_lock_file(&lock_path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                Error::Message(format!(
                    "unable to create '{}': another process may be updating this reference; remove the lock if no process is active",
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
                "failed to delete '{name}': reference changed concurrently"
            )));
        }
        if current.is_none() {
            let _ = std::fs::remove_file(&lock_path);
            return Err(Error::Message(format!("reference '{name}' does not exist")));
        }

        let result = (|| -> Result<()> {
            self.remove_packed_ref(name)?;
            let loose = self.git_dir.join(name);
            match std::fs::remove_file(&loose) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(Error::io(loose.display().to_string(), error)),
            }
            let reflog = self.git_dir.join("logs").join(name);
            let _ = std::fs::remove_file(reflog);
            cvx_core::fs::sync_parent(&loose)
        })();

        drop(lock);
        let _ = std::fs::remove_file(&lock_path);
        result
    }

    fn remove_packed_ref(&self, name: &str) -> Result<()> {
        let path = self.git_dir.join("packed-refs");
        if !path.exists() {
            return Ok(());
        }

        let contents = cvx_core::fs::read_to_string(&path)?;
        let lock_path = self.git_dir.join("packed-refs.lock");
        let mut lock = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    Error::Message(format!(
                        "unable to create '{}': another process may be packing references",
                        lock_path.display()
                    ))
                } else {
                    Error::io(lock_path.display().to_string(), error)
                }
            })?;

        let mut output = String::new();
        let mut changed = false;
        let mut skip_peeled = false;
        for line in contents.lines() {
            if skip_peeled && line.starts_with('^') {
                skip_peeled = false;
                continue;
            }
            skip_peeled = false;

            let mut fields = line.split_whitespace();
            let is_deleted =
                fields.next().is_some() && fields.next().is_some_and(|field| field == name);
            if is_deleted {
                changed = true;
                skip_peeled = true;
                continue;
            }
            output.push_str(line);
            output.push('\n');
        }

        if !changed {
            drop(lock);
            let _ = std::fs::remove_file(&lock_path);
            return Ok(());
        }

        use std::io::Write;
        lock.write_all(output.as_bytes())
            .map_err(|error| Error::io(lock_path.display().to_string(), error))?;
        lock.sync_all()
            .map_err(|error| Error::io(lock_path.display().to_string(), error))?;
        drop(lock);
        std::fs::rename(&lock_path, &path)
            .map_err(|error| Error::io(path.display().to_string(), error))?;
        cvx_core::fs::sync_parent(&path)
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

        let mut file = cvx_core::fs::create_lock_file(&lock_path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                Error::Message(format!(
                    "unable to create '{}': another process may be updating this reference; remove the lock if no process is active",
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
            cvx_core::failpoint::hit("ref.before-publish")?;
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
    fn lists_loose_and_packed_refs() {
        let (_temp, store) = store();
        let main = ObjectId::from_hex("5555555555555555555555555555555555555555").unwrap();
        let old = ObjectId::from_hex("6666666666666666666666666666666666666666").unwrap();
        store.write_ref("refs/heads/main", main).expect("write ref");
        std::fs::write(
            store.git_dir().join("packed-refs"),
            format!("{old} refs/heads/old\n"),
        )
        .expect("write packed refs");

        let refs = store.list_refs("refs/heads").expect("list");
        assert_eq!(refs.get("refs/heads/main"), Some(&main));
        assert_eq!(refs.get("refs/heads/old"), Some(&old));
    }

    #[test]
    fn deletes_loose_and_packed_refs() {
        let (_temp, store) = store();
        let main = ObjectId::from_hex("7777777777777777777777777777777777777777").unwrap();
        let other = ObjectId::from_hex("8888888888888888888888888888888888888888").unwrap();
        store.write_ref("refs/heads/main", main).expect("write ref");
        std::fs::write(
            store.git_dir().join("packed-refs"),
            format!(
                "# pack-refs with: peeled fully-peeled sorted\n{other} refs/heads/other\n{main} refs/heads/main\n^9999999999999999999999999999999999999999\n"
            ),
        )
        .expect("write packed refs");
        std::fs::create_dir_all(store.git_dir().join("logs/refs/heads")).expect("mkdir");
        std::fs::write(store.git_dir().join("logs/refs/heads/main"), "reflog\n")
            .expect("write reflog");

        store
            .delete_ref("refs/heads/main", Some(main))
            .expect("delete");
        assert_eq!(store.read_ref("refs/heads/main").expect("read"), None);
        assert_eq!(
            store.read_ref("refs/heads/other").expect("read"),
            Some(other)
        );
        assert!(!store.git_dir().join("logs/refs/heads/main").exists());

        let packed =
            std::fs::read_to_string(store.git_dir().join("packed-refs")).expect("packed refs");
        assert!(!packed.contains("refs/heads/main"));
        assert!(packed.contains("refs/heads/other"));
        assert!(!packed.contains("9999999999999999999999999999999999999999"));
    }
}
