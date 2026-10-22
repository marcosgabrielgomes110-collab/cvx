use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use cvx_core::{Error, IoResultExt, Result};
use cvx_index::{IndexEntry, IndexLock, path_to_bytes};
use cvx_object::{MODE_GITLINK, ObjectId, ObjectKind};

use crate::repository::Repository;
use crate::worktree_ops::{WorktreeContent, read_content, remove_file, write_file};

/// Options for `cvx restore` and `cvx checkout -- <paths>`.
#[derive(Debug, Clone)]
pub struct RestoreOptions {
    /// Restore index entries.
    pub staged: bool,
    /// Restore worktree files.
    pub worktree: bool,
    /// Optional source revision.
    pub source: Option<String>,
    /// Pathspecs.
    pub paths: Vec<PathBuf>,
    /// Current directory used to resolve pathspecs.
    pub cwd: PathBuf,
    /// Overwrite untracked files.
    pub force: bool,
}

/// Summary of a restore operation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RestoreOutcome {
    /// Files written to the worktree.
    pub restored: usize,
    /// Files removed from the worktree.
    pub removed: usize,
    /// Index entries updated.
    pub staged: usize,
}

type Snapshot = BTreeMap<Vec<u8>, (u32, ObjectId)>;

impl Repository {
    /// Restores index entries and/or worktree files.
    pub fn restore(&self, options: &RestoreOptions) -> Result<RestoreOutcome> {
        self.require_work_tree()?;
        if options.paths.is_empty() {
            return Err(Error::Message(
                "nothing specified, nothing restored".to_owned(),
            ));
        }

        let staged = options.staged;
        let worktree = options.worktree || !staged;
        let cwd = std::fs::canonicalize(&options.cwd).path_context(&options.cwd)?;
        let specs = self.resolve_restore_pathspecs(&cwd, &options.paths)?;
        let filemode = self.filemode()?;
        let attributes = self.attributes()?;
        let autocrlf = self.autocrlf()?;

        let lock = if staged {
            Some(IndexLock::acquire(self.control_dir())?)
        } else {
            None
        };

        let index = self.read_index()?;
        let index_snapshot: Snapshot = index
            .entries()
            .iter()
            .filter(|entry| entry.stage == 0)
            .map(|entry| (entry.path.clone(), (entry.mode, entry.oid)))
            .collect();

        let source = if let Some(revision) = &options.source {
            let oid = self.resolve_revision(revision)?;
            self.snapshot_from_commit(oid)?
        } else if staged {
            match self.head_commit()? {
                Some(commit) => self.restore_tree_snapshot(commit.tree)?,
                None => {
                    return Err(Error::Message(
                        "cannot restore from HEAD: the current branch has no commits".to_owned(),
                    ));
                }
            }
        } else {
            index_snapshot.clone()
        };

        let all_paths: Vec<Vec<u8>> = source
            .keys()
            .chain(index_snapshot.keys())
            .cloned()
            .collect();

        for spec in &specs {
            if !all_paths.iter().any(|path| path_matches(path, spec)) {
                return Err(Error::Message(format!(
                    "pathspec '{}' did not match any files",
                    String::from_utf8_lossy(spec)
                )));
            }
        }

        let mut outcome = RestoreOutcome::default();

        if worktree {
            for (path, (mode, oid)) in &source {
                if !specs.iter().any(|spec| path_matches(path, spec)) {
                    continue;
                }
                if mode & 0o170_000 == MODE_GITLINK {
                    return Err(Error::unsupported("checking out submodule entries"));
                }

                if !options.force && !index_snapshot.contains_key(path) {
                    if let WorktreeContent::File { .. } =
                        read_content(self.work_tree(), path, filemode)?
                    {
                        return Err(Error::Message(format!(
                            "refusing to overwrite untracked file {}; use --force",
                            String::from_utf8_lossy(path)
                        )));
                    }
                }

                let object = self.read_object(*oid)?;
                if object.kind != ObjectKind::Blob {
                    return Err(Error::corrupt(format!(
                        "{} is a {}, not a blob",
                        oid, object.kind
                    )));
                }
                if *mode & 0o170_000 == cvx_object::MODE_SYMLINK {
                    write_file(self.work_tree(), path, *mode, &object.data)?;
                } else {
                    let effective =
                        cvx_attributes::effective_attributes(&attributes.for_path(path), autocrlf);
                    crate::worktree_ops::write_file_with_attributes(
                        self.work_tree(),
                        path,
                        *mode,
                        &object.data,
                        &effective,
                    )?;
                }
                outcome.restored += 1;
            }

            for path in index_snapshot.keys() {
                if source.contains_key(path) || !specs.iter().any(|spec| path_matches(path, spec)) {
                    continue;
                }
                remove_file(self.work_tree(), path)?;
                outcome.removed += 1;
            }
        }

        if staged {
            let mut new_index = index;
            let affected: Vec<Vec<u8>> = all_paths
                .iter()
                .filter(|path| specs.iter().any(|spec| path_matches(path, spec)))
                .cloned()
                .collect();

            for path in &affected {
                new_index.remove_path(path);
            }
            for (path, (mode, oid)) in &source {
                if !specs.iter().any(|spec| path_matches(path, spec)) {
                    continue;
                }
                new_index.insert(IndexEntry::new(path.clone(), *oid, *mode))?;
                outcome.staged += 1;
            }

            if let Some(lock) = lock {
                lock.commit(&new_index)?;
            }
        }

        Ok(outcome)
    }

    fn snapshot_from_commit(&self, oid: ObjectId) -> Result<Snapshot> {
        let commit = self.read_commit(oid)?;
        self.restore_tree_snapshot(commit.tree)
    }

    fn restore_tree_snapshot(&self, tree: ObjectId) -> Result<Snapshot> {
        Ok(self
            .read_tree_recursive(tree)?
            .into_iter()
            .map(|(path, leaf)| (path, (leaf.mode, leaf.oid)))
            .collect())
    }

    fn resolve_restore_pathspecs(&self, cwd: &Path, paths: &[PathBuf]) -> Result<Vec<Vec<u8>>> {
        let work_tree = self.work_tree();
        let mut specs = Vec::new();

        for path in paths {
            let absolute = lexical_absolute(cwd, path);
            if !absolute.starts_with(work_tree) {
                return Err(Error::Message(format!(
                    "pathspec '{}' is outside the repository",
                    path.display()
                )));
            }
            let relative = absolute
                .strip_prefix(work_tree)
                .expect("starts_with checked");
            specs.push(if relative.as_os_str().is_empty() {
                Vec::new()
            } else {
                path_to_bytes(relative)?
            });
        }

        Ok(specs)
    }
}

fn path_matches(path: &[u8], spec: &[u8]) -> bool {
    if spec.is_empty() {
        return true;
    }
    path == spec || (path.len() > spec.len() && path.starts_with(spec) && path[spec.len()] == b'/')
}

fn lexical_absolute(cwd: &Path, path: &Path) -> PathBuf {
    let combined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };

    let mut output = PathBuf::new();
    for component in combined.components() {
        match component {
            Component::Prefix(prefix) => output.push(prefix.as_os_str()),
            Component::RootDir => output.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                output.pop();
            }
            Component::Normal(name) => output.push(name),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AddOptions, CommitOptions, InitOptions};

    fn repository(temp: &Path) -> Repository {
        let repository = match Repository::init(InitOptions {
            directory: temp.to_path_buf(),
            ..InitOptions::default()
        })
        .expect("init")
        {
            crate::InitOutcome::Created(repository) => repository,
            _ => panic!("expected new repository"),
        };
        let config = repository.control_dir().join("config");
        let mut contents = std::fs::read_to_string(&config).expect("config");
        contents.push_str("\n[user]\n\tname = Test User\n\temail = test@example.com\n");
        std::fs::write(config, contents).expect("write config");
        repository
    }

    fn add_all(repository: &Repository, cwd: &Path) {
        repository
            .add(AddOptions {
                paths: vec![PathBuf::from(".")],
                cwd: cwd.to_path_buf(),
                force: false,
                update: false,
                all: true,
            })
            .expect("add");
    }

    fn commit(repository: &Repository, message: &str) {
        repository
            .commit(CommitOptions {
                messages: vec![message.to_owned()],
                allow_empty: false,
            })
            .expect("commit");
    }

    fn restore(cwd: &Path) -> RestoreOptions {
        RestoreOptions {
            staged: false,
            worktree: true,
            source: None,
            paths: vec![PathBuf::from(".")],
            cwd: cwd.to_path_buf(),
            force: false,
        }
    }

    #[test]
    fn restores_worktree_from_index() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repository = repository(temp.path());
        std::fs::write(temp.path().join("file.txt"), "one\n").expect("write");
        add_all(&repository, temp.path());
        commit(&repository, "first");

        std::fs::write(temp.path().join("file.txt"), "changed\n").expect("write");
        repository.restore(&restore(temp.path())).expect("restore");
        assert_eq!(
            std::fs::read_to_string(temp.path().join("file.txt")).unwrap(),
            "one\n"
        );
    }

    #[test]
    fn restores_staged_content_from_head() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repository = repository(temp.path());
        std::fs::write(temp.path().join("file.txt"), "one\n").expect("write");
        add_all(&repository, temp.path());
        commit(&repository, "first");

        std::fs::write(temp.path().join("file.txt"), "two\n").expect("write");
        add_all(&repository, temp.path());
        let mut options = restore(temp.path());
        options.staged = true;
        options.worktree = false;
        repository.restore(&options).expect("restore staged");

        let index = repository.read_index().expect("index");
        assert_eq!(
            index.get(b"file.txt").unwrap().oid,
            cvx_object::hash_object(ObjectKind::Blob, b"one\n")
        );
    }

    #[test]
    fn restores_deleted_file_from_head() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repository = repository(temp.path());
        std::fs::write(temp.path().join("file.txt"), "one\n").expect("write");
        add_all(&repository, temp.path());
        commit(&repository, "first");

        std::fs::remove_file(temp.path().join("file.txt")).expect("remove");
        let mut options = restore(temp.path());
        options.staged = true;
        options.worktree = true;
        options.source = Some("HEAD".to_owned());
        repository.restore(&options).expect("restore");

        assert_eq!(
            std::fs::read_to_string(temp.path().join("file.txt")).unwrap(),
            "one\n"
        );
        assert!(
            repository
                .read_index()
                .expect("index")
                .get(b"file.txt")
                .is_some()
        );
    }
}
