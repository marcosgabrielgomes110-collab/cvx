use std::collections::BTreeSet;

use cvx_core::Result;
use cvx_index::{Index, IndexLock};
use cvx_object::{Commit, ObjectId, ObjectKind, Tag, Tree};

use crate::Repository;
use crate::switch::TreeSnapshot;

/// Options for `cvx fsck`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FsckOptions {
    /// Rebuild a missing or unreadable index from `HEAD`.
    pub repair: bool,
}

/// Result of an integrity check over a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsckReport {
    /// Number of reachable objects successfully checked.
    pub checked_objects: usize,
    /// Objects that are present but not reachable from any reference.
    pub dangling: Vec<ObjectId>,
    /// Human-readable integrity errors.
    pub errors: Vec<String>,
    /// Repairs performed by `--repair`.
    pub repairs: Vec<String>,
}

impl FsckReport {
    /// Returns whether the repository passed every check.
    pub fn is_clean(&self) -> bool {
        self.errors.is_empty()
    }
}

impl Repository {
    /// Verifies references, objects, trees, tags and the index.
    ///
    /// The check is read-only. Missing or mismatched objects are reported in
    /// [`FsckReport::errors`] instead of aborting at the first problem.
    pub fn fsck(&self) -> Result<FsckReport> {
        self.fsck_with_options(FsckOptions::default())
    }

    /// Verifies the repository and optionally repairs a missing/corrupt index.
    pub fn fsck_with_options(&self, options: FsckOptions) -> Result<FsckReport> {
        let mut state = FsckState::default();
        let mut repairs = Vec::new();

        if let Ok(head) = self.head()
            && let Some(oid) = head.oid()
        {
            state.ensure(self, oid, None, "HEAD");
        }

        match self.refs().list_refs("refs") {
            Ok(references) => {
                for (name, oid) in references {
                    let expected =
                        if name.starts_with("refs/heads/") || name.starts_with("refs/remotes/") {
                            Some(ObjectKind::Commit)
                        } else {
                            None
                        };
                    state.ensure(self, oid, expected, &name);
                }
            }
            Err(error) => state
                .errors
                .push(format!("cannot read references: {error}")),
        }

        let mut indexed = BTreeSet::new();
        let existing = Index::read_from_with_algorithm(&self.index_path(), self.hash_algorithm());
        let index = match existing {
            Ok(Some(index)) => Some(index),
            Ok(None) if options.repair && !self.is_bare() => match self.repair_index() {
                Ok(index) => {
                    repairs.push("rebuilt missing index from HEAD".to_owned());
                    Some(index)
                }
                Err(error) => {
                    state.errors.push(format!("cannot rebuild index: {error}"));
                    None
                }
            },
            Ok(None) => None,
            Err(error) if options.repair && !self.is_bare() => match self.repair_index() {
                Ok(index) => {
                    repairs.push(format!("rebuilt unreadable index from HEAD: {error}"));
                    Some(index)
                }
                Err(error) => {
                    state.errors.push(format!("cannot rebuild index: {error}"));
                    None
                }
            },
            Err(error) => {
                state.errors.push(format!("cannot read index: {error}"));
                None
            }
        };
        if let Some(index) = index {
            for entry in index.entries() {
                indexed.insert(entry.oid);
                state.ensure(
                    self,
                    entry.oid,
                    Some(ObjectKind::Blob),
                    &format!("index path {}", String::from_utf8_lossy(&entry.path)),
                );
            }
        }

        let mut dangling = Vec::new();
        match self.objects().iter_oids() {
            Ok(oids) => {
                for oid in oids {
                    if !state.visited.contains(&oid) && !indexed.contains(&oid) {
                        dangling.push(oid);
                    }
                }
            }
            Err(error) => state
                .errors
                .push(format!("cannot enumerate objects: {error}")),
        }

        Ok(FsckReport {
            checked_objects: state.checked_objects,
            dangling,
            errors: state.errors,
            repairs,
        })
    }

    fn repair_index(&self) -> Result<Index> {
        let snapshot: TreeSnapshot = match self.head()?.oid() {
            Some(oid) => {
                let commit = self.read_commit(oid)?;
                self.read_tree_recursive(commit.tree)?
                    .into_iter()
                    .map(|(path, leaf)| (path, (leaf.mode, leaf.oid)))
                    .collect()
            }
            None => TreeSnapshot::new(),
        };
        let index = self.index_from_snapshot(&snapshot)?;
        let lock = IndexLock::acquire(self.control_dir())?;
        lock.commit(&index)?;
        Ok(index)
    }
}

#[derive(Default)]
struct FsckState {
    visited: BTreeSet<ObjectId>,
    checked_objects: usize,
    errors: Vec<String>,
}

impl FsckState {
    fn ensure(
        &mut self,
        repository: &Repository,
        oid: ObjectId,
        expected: Option<ObjectKind>,
        context: &str,
    ) {
        if !self.visited.insert(oid) {
            if let Some(expected) = expected {
                match repository.objects().read(oid) {
                    Ok(object) if object.kind != expected => self.errors.push(format!(
                        "{context}: {oid} is a {}, expected {}",
                        object.kind, expected
                    )),
                    Err(_) => {}
                    Ok(_) => {}
                }
            }
            return;
        }

        let object = match repository.objects().read(oid) {
            Ok(object) => object,
            Err(error) => {
                self.errors.push(format!("{context}: {error}"));
                return;
            }
        };
        self.checked_objects += 1;

        if let Some(expected) = expected
            && object.kind != expected
        {
            self.errors.push(format!(
                "{context}: {oid} is a {}, expected {}",
                object.kind, expected
            ));
            return;
        }

        match object.kind {
            ObjectKind::Commit => match Commit::parse(&object.data) {
                Ok(commit) => {
                    self.ensure(
                        repository,
                        commit.tree,
                        Some(ObjectKind::Tree),
                        &format!("{oid}:tree"),
                    );
                    for parent in commit.parents {
                        self.ensure(
                            repository,
                            parent,
                            Some(ObjectKind::Commit),
                            &format!("{oid}:parent"),
                        );
                    }
                }
                Err(error) => self.errors.push(format!("{oid}: {error}")),
            },
            ObjectKind::Tree => {
                match Tree::parse_with_algorithm(&object.data, repository.hash_algorithm()) {
                    Ok(tree) => {
                        for entry in tree.entries() {
                            if entry.is_gitlink() {
                                continue;
                            }
                            let expected = if entry.is_tree() {
                                ObjectKind::Tree
                            } else {
                                ObjectKind::Blob
                            };
                            self.ensure(
                                repository,
                                entry.oid,
                                Some(expected),
                                &format!("{oid}:{}", String::from_utf8_lossy(&entry.name)),
                            );
                        }
                    }
                    Err(error) => self.errors.push(format!("{oid}: {error}")),
                }
            }
            ObjectKind::Tag => match Tag::parse(&object.data) {
                Ok(tag) => {
                    self.ensure(
                        repository,
                        tag.object,
                        Some(tag.kind),
                        &format!("{oid}:tag target"),
                    );
                }
                Err(error) => self.errors.push(format!("{oid}: {error}")),
            },
            ObjectKind::Blob => {}
        }
    }
}
