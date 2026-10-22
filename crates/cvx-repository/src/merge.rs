use std::collections::{BTreeMap, BTreeSet};

use cvx_core::{Error, Result};
use cvx_index::{Index, IndexEntry, IndexLock, MODE_GITLINK, MODE_SYMLINK};
use cvx_merge::{FileMerge, MergeLabels, conflict_markers, merge_file, merge_mode};
use cvx_object::{ObjectId, ObjectKind, hash_object_with};
use cvx_ref::Head;

use crate::commit::IdentityRole;
use crate::repository::Repository;
use crate::status::StatusOptions;
use crate::switch::TreeSnapshot;
use crate::worktree_ops::{WorktreeContent, read_content, remove_file, write_file};

/// Options for `cvx merge`.
#[derive(Debug, Clone)]
pub struct MergeOptions {
    /// Branch, tag or revision to merge.
    pub revision: String,
    /// Optional merge-commit message.
    pub message: Option<String>,
}

/// Result of a merge operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome {
    /// The target was already reachable from `HEAD`.
    AlreadyUpToDate,
    /// `HEAD` was moved directly to the target.
    FastForward {
        /// New commit at `HEAD`.
        oid: ObjectId,
    },
    /// A merge commit was created.
    Merged {
        /// New merge commit.
        oid: ObjectId,
        /// First line of the merge message.
        subject: String,
    },
    /// The merge stopped with unresolved paths.
    Conflicts {
        /// Conflicted paths.
        paths: Vec<Vec<u8>>,
        /// Message stored in `MERGE_MSG`.
        message: String,
    },
}

impl Repository {
    /// Merges a revision into the current branch.
    pub fn merge(&self, options: &MergeOptions) -> Result<MergeOutcome> {
        if self.merge_head()?.is_some() {
            return Err(Error::Message(
                "a merge is already in progress; use `cvx merge --continue` or `cvx merge --abort`"
                    .to_owned(),
            ));
        }

        let status = self.status(StatusOptions {
            untracked: true,
            collapse_untracked: false,
        })?;
        if !status.conflicts.is_empty() {
            return Err(Error::Message(
                "cannot merge with unresolved index entries".to_owned(),
            ));
        }
        if !status.staged.is_empty() || !status.unstaged.is_empty() {
            return Err(Error::Message(
                "local changes would be overwritten; commit them before merging".to_owned(),
            ));
        }

        let head = self.head()?;
        let head_oid = head
            .oid()
            .ok_or_else(|| Error::Message("cannot merge before the first commit".to_owned()))?;
        let target_oid = self.resolve_revision(&options.revision)?;
        self.read_commit(target_oid)?;

        if target_oid == head_oid {
            return Ok(MergeOutcome::AlreadyUpToDate);
        }

        let base = cvx_revision::merge_base(head_oid, target_oid, |oid| self.read_commit(oid))?;
        if base == Some(target_oid) {
            return Ok(MergeOutcome::AlreadyUpToDate);
        }
        if base == Some(head_oid) {
            return self.fast_forward(&head, head_oid, target_oid, &options.revision);
        }
        let base_oid =
            base.ok_or_else(|| Error::Message("refusing to merge unrelated histories".to_owned()))?;

        let base_tree = self.tree_snapshot_for_commit(base_oid)?;
        let ours_tree = self.tree_snapshot_for_commit(head_oid)?;
        let theirs_tree = self.tree_snapshot_for_commit(target_oid)?;

        let message = match &options.message {
            Some(message) => normalize_message(message)?,
            None => self.default_merge_message(&options.revision, target_oid)?,
        };

        let filemode = self.filemode()?;
        let current_index = self.read_index()?;
        let paths = base_tree
            .keys()
            .chain(ours_tree.keys())
            .chain(theirs_tree.keys())
            .cloned()
            .collect::<BTreeSet<_>>();

        let mut final_paths = BTreeSet::new();
        for path in &paths {
            let base = base_tree.get(path).copied();
            let ours = ours_tree.get(path).copied();
            let theirs = theirs_tree.get(path).copied();

            let present = if ours == theirs {
                ours.is_some()
            } else if ours == base {
                theirs.is_some()
            } else if theirs == base {
                ours.is_some()
            } else {
                true
            };

            if present {
                final_paths.insert(path.clone());
            }
            if ours.is_none() && theirs.is_some() && theirs != base {
                self.ensure_merge_path_available(&current_index, path, filemode)?;
            }
        }
        for path in &final_paths {
            for (position, byte) in path.iter().enumerate() {
                if *byte == b'/' && final_paths.contains(&path[..position]) {
                    return Err(Error::unsupported(
                        "directory/file conflicts in three-way merge",
                    ));
                }
            }
        }

        self.write_orig_head(head_oid)?;
        self.write_merge_state(target_oid, &message)?;

        let labels = MergeLabels {
            ours: "HEAD",
            theirs: &options.revision,
        };
        let lock = IndexLock::acquire(self.control_dir())?;
        let mut index = self.read_index()?;
        let mut conflicts = Vec::new();

        for path in paths {
            let base = base_tree.get(&path).copied();
            let ours = ours_tree.get(&path).copied();
            let theirs = theirs_tree.get(&path).copied();

            if ours == theirs {
                continue;
            }
            if ours == base {
                self.apply_merge_side(&mut index, &path, theirs, filemode)?;
                continue;
            }
            if theirs == base {
                continue;
            }

            match (base, ours, theirs) {
                (None, Some(ours), Some(theirs)) => {
                    let ours_content = self.blob_content(ours.1)?;
                    let theirs_content = self.blob_content(theirs.1)?;
                    let markers = conflict_markers(&ours_content, &theirs_content, labels);
                    self.write_conflict(
                        &mut index,
                        &path,
                        Some((ours.0, &markers)),
                        &[(2, ours), (3, theirs)],
                        filemode,
                    )?;
                    conflicts.push(path);
                }
                (Some(base), None, Some(theirs)) => {
                    let theirs_content = self.blob_content(theirs.1)?;
                    self.write_conflict(
                        &mut index,
                        &path,
                        Some((theirs.0, &theirs_content)),
                        &[(1, base), (3, theirs)],
                        filemode,
                    )?;
                    conflicts.push(path);
                }
                (Some(base), Some(ours), None) => {
                    self.write_conflict(
                        &mut index,
                        &path,
                        None,
                        &[(1, base), (2, ours)],
                        filemode,
                    )?;
                    conflicts.push(path);
                }
                (Some(base), Some(ours), Some(theirs)) => {
                    let base_content = self.blob_content(base.1)?;
                    let ours_content = self.blob_content(ours.1)?;
                    let theirs_content = self.blob_content(theirs.1)?;
                    let merged = merge_file(&base_content, &ours_content, &theirs_content, labels);
                    let mode = merge_mode(base.0, ours.0, theirs.0);

                    if merged.clean {
                        if let Some(mode) = mode {
                            let oid = hash_object_with(
                                self.hash_algorithm(),
                                ObjectKind::Blob,
                                &merged.content,
                            );
                            self.write_clean(
                                &mut index,
                                &path,
                                mode,
                                oid,
                                &merged.content,
                                filemode,
                            )?;
                        } else {
                            self.write_conflict(
                                &mut index,
                                &path,
                                None,
                                &[(1, base), (2, ours), (3, theirs)],
                                filemode,
                            )?;
                            conflicts.push(path);
                        }
                    } else {
                        self.write_content_conflict(
                            &mut index,
                            &path,
                            ours.0,
                            &merged,
                            &[(1, base), (2, ours), (3, theirs)],
                            filemode,
                        )?;
                        conflicts.push(path);
                    }
                }
                _ => unreachable!("all merge cases are covered"),
            }
        }

        lock.commit(&index)?;

        if conflicts.is_empty() {
            let outcome =
                self.commit_with_parents(vec![message.clone()], vec![head_oid, target_oid], true)?;
            self.clear_merge_state()?;
            Ok(MergeOutcome::Merged {
                oid: outcome.oid,
                subject: outcome.subject,
            })
        } else {
            Ok(MergeOutcome::Conflicts {
                paths: conflicts,
                message,
            })
        }
    }

    /// Aborts an in-progress merge and restores the pre-merge state.
    pub fn merge_abort(&self) -> Result<()> {
        let merge_head = self
            .merge_head()?
            .ok_or_else(|| Error::Message("there is no merge in progress".to_owned()))?;
        let _ = merge_head;
        let original = self
            .read_control_oid("ORIG_HEAD")?
            .ok_or_else(|| Error::Message("ORIG_HEAD is missing; cannot abort".to_owned()))?;

        let head = self.head()?;
        let current = head
            .oid()
            .ok_or_else(|| Error::Message("HEAD has no commits".to_owned()))?;
        let target_tree = self.tree_snapshot_for_commit(original)?;
        let index = self.read_index()?;
        let index_snapshot: BTreeMap<Vec<u8>, (u32, ObjectId)> = index
            .entries()
            .iter()
            .filter(|entry| entry.stage == 0)
            .map(|entry| (entry.path.clone(), (entry.mode, entry.oid)))
            .collect();
        let filemode = self.filemode()?;

        self.checkout_target(&target_tree, &index_snapshot, filemode, true)?;
        let new_index = self.index_from_tree(&target_tree, filemode)?;
        let lock = IndexLock::acquire(self.control_dir())?;
        lock.commit(&new_index)?;

        match &head {
            Head::Symbolic { target, .. } => {
                self.refs().update_ref(target, original, Some(current))?;
            }
            Head::Detached(_) => {
                self.refs().write_head_detached(original)?;
            }
        }
        if self.reflog_enabled()?
            && let Ok(identity) = self.resolve_signature(IdentityRole::Committer)
        {
            let _ = self.append_reflog(
                "HEAD",
                Some(current),
                original,
                &identity,
                "reset: moving to ORIG_HEAD",
            );
        }

        self.clear_merge_state()?;
        Ok(())
    }

    /// Concludes a merge after conflicts were resolved and staged.
    pub fn merge_continue(&self, message: Option<&str>) -> Result<MergeOutcome> {
        let merge_head = self
            .merge_head()?
            .ok_or_else(|| Error::Message("there is no merge in progress".to_owned()))?;
        let index = self.read_index()?;
        if index.has_conflicts() {
            return Err(Error::Message(
                "cannot continue: unmerged paths remain in the index".to_owned(),
            ));
        }
        let head_oid = self
            .head()?
            .oid()
            .ok_or_else(|| Error::Message("HEAD has no commits".to_owned()))?;

        let message = match message {
            Some(message) => normalize_message(message)?,
            None => match self.read_control_text("MERGE_MSG")? {
                Some(message) if !message.trim().is_empty() => message,
                _ => self.default_merge_message("merge", merge_head)?,
            },
        };

        let outcome = self.commit_with_parents(vec![message], vec![head_oid, merge_head], true)?;
        self.clear_merge_state()?;
        Ok(MergeOutcome::Merged {
            oid: outcome.oid,
            subject: outcome.subject,
        })
    }

    /// Returns the merge target recorded in `MERGE_HEAD`, if any.
    pub(crate) fn merge_head(&self) -> Result<Option<ObjectId>> {
        self.read_control_oid("MERGE_HEAD")
    }

    fn fast_forward(
        &self,
        head: &Head,
        head_oid: ObjectId,
        target_oid: ObjectId,
        revision: &str,
    ) -> Result<MergeOutcome> {
        let target_tree = self.tree_snapshot_for_commit(target_oid)?;
        let index = self.read_index()?;
        let snapshot: BTreeMap<Vec<u8>, (u32, ObjectId)> = index
            .entries()
            .iter()
            .filter(|entry| entry.stage == 0)
            .map(|entry| (entry.path.clone(), (entry.mode, entry.oid)))
            .collect();
        let filemode = self.filemode()?;

        self.checkout_target(&target_tree, &snapshot, filemode, false)?;
        let new_index = self.index_from_tree(&target_tree, filemode)?;
        let lock = IndexLock::acquire(self.control_dir())?;
        lock.commit(&new_index)?;

        match head {
            Head::Symbolic { target, .. } => {
                self.refs().update_ref(target, target_oid, Some(head_oid))?;
            }
            Head::Detached(_) => {
                self.refs().write_head_detached(target_oid)?;
            }
        }
        if self.reflog_enabled()?
            && let Ok(identity) = self.resolve_signature(IdentityRole::Committer)
        {
            let message = format!("merge {revision}: Fast-forward");
            let _ = self.append_reflog("HEAD", Some(head_oid), target_oid, &identity, &message);
        }
        self.write_orig_head(head_oid)?;

        Ok(MergeOutcome::FastForward { oid: target_oid })
    }

    pub(crate) fn tree_snapshot_for_commit(&self, oid: ObjectId) -> Result<TreeSnapshot> {
        let commit = self.read_commit(oid)?;
        Ok(self
            .read_tree_recursive(commit.tree)?
            .into_iter()
            .map(|(path, leaf)| (path, (leaf.mode, leaf.oid)))
            .collect())
    }

    fn apply_merge_side(
        &self,
        index: &mut Index,
        path: &[u8],
        side: Option<(u32, ObjectId)>,
        filemode: bool,
    ) -> Result<()> {
        match side {
            Some((mode, oid)) => {
                if mode & 0o170_000 == MODE_GITLINK {
                    return Err(Error::unsupported("merging submodule entries"));
                }
                let content = self.blob_content(oid)?;
                self.write_clean(index, path, mode, oid, &content, filemode)
            }
            None => {
                index.remove_path(path);
                remove_file(self.work_tree(), path)
            }
        }
    }

    pub(crate) fn write_clean(
        &self,
        index: &mut Index,
        path: &[u8],
        mode: u32,
        oid: ObjectId,
        content: &[u8],
        filemode: bool,
    ) -> Result<()> {
        self.ensure_merge_path_available(index, path, filemode)?;
        if mode & 0o170_000 == MODE_GITLINK {
            return Err(Error::unsupported("merging submodule entries"));
        }

        self.objects().write(ObjectKind::Blob, content)?;
        if mode & 0o170_000 == MODE_SYMLINK {
            write_file(self.work_tree(), path, mode, content)?;
        } else {
            let attributes = self.effective_attributes_for(path, &self.attributes()?)?;
            crate::worktree_ops::write_file_with_attributes(
                self.work_tree(),
                path,
                mode,
                content,
                &attributes,
            )?;
        }

        let absolute = self.work_tree().join(cvx_index::path_from_bytes(path)?);
        let metadata = std::fs::symlink_metadata(&absolute)
            .map_err(|error| Error::io(absolute.display().to_string(), error))?;
        let mut entry = IndexEntry::from_metadata(path.to_vec(), oid, &metadata, filemode)?;
        entry.mode = mode;
        index.remove_path(path);
        index.insert(entry)?;
        Ok(())
    }

    fn write_content_conflict(
        &self,
        index: &mut Index,
        path: &[u8],
        worktree_mode: u32,
        merged: &FileMerge,
        stages: &[(u8, (u32, ObjectId))],
        filemode: bool,
    ) -> Result<()> {
        self.write_conflict(
            index,
            path,
            Some((worktree_mode, &merged.content)),
            stages,
            filemode,
        )
    }

    fn write_conflict(
        &self,
        index: &mut Index,
        path: &[u8],
        worktree: Option<(u32, &[u8])>,
        stages: &[(u8, (u32, ObjectId))],
        filemode: bool,
    ) -> Result<()> {
        if let Some((mode, content)) = worktree {
            self.ensure_merge_path_available(index, path, filemode)?;
            if mode & 0o170_000 == MODE_GITLINK {
                return Err(Error::unsupported("merging submodule entries"));
            }
            if mode & 0o170_000 == MODE_SYMLINK {
                write_file(self.work_tree(), path, mode, content)?;
            } else {
                let attributes = self.effective_attributes_for(path, &self.attributes()?)?;
                crate::worktree_ops::write_file_with_attributes(
                    self.work_tree(),
                    path,
                    mode,
                    content,
                    &attributes,
                )?;
            }
        }

        index.remove_path(path);
        for (stage, (mode, oid)) in stages {
            let mut entry = IndexEntry::new(path.to_vec(), *oid, *mode);
            entry.stage = *stage;
            index.insert(entry)?;
        }
        Ok(())
    }

    fn ensure_merge_path_available(
        &self,
        index: &Index,
        path: &[u8],
        filemode: bool,
    ) -> Result<()> {
        if index.get(path).is_some() {
            return Ok(());
        }
        match read_content(self.work_tree(), path, filemode)? {
            WorktreeContent::File { .. } => Err(Error::Message(format!(
                "untracked file {} would be overwritten by the merge",
                String::from_utf8_lossy(path)
            ))),
            WorktreeContent::Directory => Err(Error::Message(format!(
                "a directory is in the way of {}",
                String::from_utf8_lossy(path)
            ))),
            WorktreeContent::Missing => Ok(()),
        }
    }

    pub(crate) fn blob_content(&self, oid: ObjectId) -> Result<Vec<u8>> {
        let object = self.read_object(oid)?;
        if object.kind != ObjectKind::Blob {
            return Err(Error::corrupt(format!(
                "{oid} is a {}, not a blob",
                object.kind
            )));
        }
        Ok(object.data)
    }

    fn default_merge_message(&self, revision: &str, target: ObjectId) -> Result<String> {
        if self
            .refs()
            .read_ref(&format!("refs/heads/{revision}"))?
            .is_some()
        {
            return Ok(format!("Merge branch '{revision}'\n"));
        }
        if self
            .refs()
            .read_ref(&format!("refs/tags/{revision}"))?
            .is_some()
        {
            return Ok(format!("Merge tag '{revision}'\n"));
        }
        Ok(format!("Merge commit '{}'\n", target.short(7)))
    }

    fn write_orig_head(&self, oid: ObjectId) -> Result<()> {
        cvx_core::fs::write_atomic(
            &self.control_dir().join("ORIG_HEAD"),
            format!("{oid}\n").as_bytes(),
            Some(0o644),
        )
    }

    fn write_merge_state(&self, target: ObjectId, message: &str) -> Result<()> {
        let directory = self.control_dir();
        cvx_core::fs::write_atomic(
            &directory.join("MERGE_HEAD"),
            format!("{target}\n").as_bytes(),
            Some(0o644),
        )?;
        cvx_core::fs::write_atomic(
            &directory.join("MERGE_MSG"),
            message.as_bytes(),
            Some(0o644),
        )
    }

    pub(crate) fn clear_merge_state(&self) -> Result<()> {
        let directory = self.control_dir();
        for name in ["MERGE_HEAD", "MERGE_MSG"] {
            let path = directory.join(name);
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(Error::io(path.display().to_string(), error)),
            }
        }
        Ok(())
    }

    fn read_control_oid(&self, name: &str) -> Result<Option<ObjectId>> {
        let Some(text) = self.read_control_text(name)? else {
            return Ok(None);
        };
        ObjectId::from_hex(text.trim()).map(Some).map_err(|error| {
            Error::corrupt(format!("{} contains an invalid object id: {error}", name))
        })
    }

    fn read_control_text(&self, name: &str) -> Result<Option<String>> {
        let path = self.control_dir().join(name);
        match std::fs::read_to_string(&path) {
            Ok(contents) => Ok(Some(contents)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(Error::io(path.display().to_string(), error)),
        }
    }
}

fn normalize_message(message: &str) -> Result<String> {
    let mut lines: Vec<&str> = message.lines().map(str::trim_end).collect();
    while lines.first().is_some_and(|line| line.is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    if lines.is_empty() {
        return Err(Error::Message("empty merge message".to_owned()));
    }
    let mut message = lines.join("\n");
    message.push('\n');
    Ok(message)
}
