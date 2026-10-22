use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use cvx_core::{Error, Result};
use cvx_index::{Index, IndexEntry, IndexLock};
use cvx_object::ObjectId;
use cvx_ref::Head;

use crate::commit::IdentityRole;
use crate::repository::Repository;
use crate::restore::path_matches;
use crate::switch::TreeSnapshot;

/// How much state `cvx reset` restores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResetMode {
    /// Move the branch or detached HEAD only.
    Soft,
    /// Move HEAD and reset the index.
    #[default]
    Mixed,
    /// Move HEAD, reset the index and overwrite the worktree.
    Hard,
}

/// Options for `cvx reset`.
#[derive(Debug, Clone)]
pub struct ResetOptions {
    /// Commit or revision to reset to.
    pub target: String,
    /// Which parts of the repository to update.
    pub mode: ResetMode,
    /// Optional pathspecs. When present, only the index is updated.
    pub paths: Vec<PathBuf>,
    /// Current directory used to resolve pathspecs.
    pub cwd: PathBuf,
}

/// Result of a reset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetOutcome {
    /// Commit HEAD pointed at before the reset, if any.
    pub previous: Option<ObjectId>,
    /// Commit HEAD points at after the reset.
    pub target: ObjectId,
    /// Reset mode that was applied.
    pub mode: ResetMode,
    /// Number of index paths updated by a pathspec reset.
    pub updated_paths: usize,
}

impl Repository {
    /// Moves HEAD to a commit and optionally resets the index and worktree.
    pub fn reset(&self, options: &ResetOptions) -> Result<ResetOutcome> {
        self.require_work_tree()?;

        if !options.paths.is_empty() {
            if options.mode != ResetMode::Mixed {
                return Err(Error::Message(
                    "--soft and --hard cannot be combined with pathspecs".to_owned(),
                ));
            }
            return self.reset_paths(options);
        }

        let previous = self.head()?.oid();
        let target = self.resolve_revision(&options.target)?;
        let commit = self.read_commit(target)?;
        let snapshot: TreeSnapshot = self
            .read_tree_recursive(commit.tree)?
            .into_iter()
            .map(|(path, leaf)| (path, (leaf.mode, leaf.oid)))
            .collect();

        if options.mode != ResetMode::Soft {
            let lock = IndexLock::acquire(self.control_dir())?;
            if options.mode == ResetMode::Hard {
                let filemode = self.filemode()?;
                let current = self.read_index()?;
                let current_snapshot: BTreeMap<Vec<u8>, (u32, ObjectId)> = current
                    .entries()
                    .iter()
                    .filter(|entry| entry.stage == 0)
                    .map(|entry| (entry.path.clone(), (entry.mode, entry.oid)))
                    .collect();
                self.checkout_target(&snapshot, &current_snapshot, filemode, true)?;
                let new_index = self.index_from_tree(&snapshot, filemode)?;
                lock.commit(&new_index)?;
            } else {
                let new_index = self.index_from_snapshot(&snapshot)?;
                lock.commit(&new_index)?;
            }
        }

        self.move_head(target, previous, &options.target)?;
        if let Some(previous) = previous {
            let path = self.control_dir().join("ORIG_HEAD");
            cvx_core::fs::write_atomic(&path, format!("{previous}\n").as_bytes(), Some(0o644))?;
        }

        Ok(ResetOutcome {
            previous,
            target,
            mode: options.mode,
            updated_paths: 0,
        })
    }

    fn reset_paths(&self, options: &ResetOptions) -> Result<ResetOutcome> {
        let previous = self.head()?.oid();
        let target = self.resolve_revision(&options.target)?;
        let commit = self.read_commit(target)?;
        let snapshot: TreeSnapshot = self
            .read_tree_recursive(commit.tree)?
            .into_iter()
            .map(|(path, leaf)| (path, (leaf.mode, leaf.oid)))
            .collect();
        let specs = self.resolve_restore_pathspecs(&options.cwd, &options.paths)?;

        let mut index = self.read_index()?;
        let mut candidates = BTreeSet::new();
        candidates.extend(snapshot.keys().cloned());
        candidates.extend(index.entries().iter().map(|entry| entry.path.clone()));
        let selected: Vec<Vec<u8>> = candidates
            .into_iter()
            .filter(|path| specs.iter().any(|spec| path_matches(path, spec)))
            .collect();
        if selected.is_empty() {
            return Err(Error::Message(
                "pathspec did not match any tracked files".to_owned(),
            ));
        }

        let lock = IndexLock::acquire(self.control_dir())?;
        let mut updated_paths = 0;
        for path in selected {
            match snapshot.get(&path) {
                Some((mode, oid)) => {
                    index.remove_path(&path);
                    index.insert(IndexEntry::new(path, *oid, *mode))?;
                    updated_paths += 1;
                }
                None => {
                    if !index.remove_path(&path).is_empty() {
                        updated_paths += 1;
                    }
                }
            }
        }
        index.sort();
        lock.commit(&index)?;

        Ok(ResetOutcome {
            previous,
            target,
            mode: ResetMode::Mixed,
            updated_paths,
        })
    }

    pub(crate) fn index_from_snapshot(&self, snapshot: &TreeSnapshot) -> Result<Index> {
        let mut index = Index::new_with_algorithm(self.hash_algorithm());
        for (path, (mode, oid)) in snapshot {
            index.insert(IndexEntry::new(path.clone(), *oid, *mode))?;
        }
        Ok(index)
    }

    fn move_head(&self, target: ObjectId, previous: Option<ObjectId>, label: &str) -> Result<()> {
        let head = self.head()?;
        let message = format!("reset: moving to {label}");
        let identity = self.resolve_signature(IdentityRole::Committer).ok();

        match head {
            Head::Symbolic {
                target: reference,
                oid,
            } => {
                if oid == previous {
                    self.refs().update_ref(&reference, target, oid)?;
                } else {
                    return Err(Error::Message(
                        "failed to reset: HEAD changed concurrently".to_owned(),
                    ));
                }
                if let (Some(identity), true) = (&identity, self.reflog_enabled()?) {
                    let _ = self.append_reflog(&reference, previous, target, identity, &message);
                    let _ = self.append_reflog("HEAD", previous, target, identity, &message);
                }
            }
            Head::Detached(oid) => {
                if Some(oid) != previous {
                    return Err(Error::Message(
                        "failed to reset: HEAD changed concurrently".to_owned(),
                    ));
                }
                self.refs().write_head_detached(target)?;
                if let (Some(identity), true) = (&identity, self.reflog_enabled()?) {
                    let _ = self.append_reflog("HEAD", previous, target, identity, &message);
                }
            }
        }

        Ok(())
    }
}
