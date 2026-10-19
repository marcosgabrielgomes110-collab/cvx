use std::collections::{BTreeMap, BTreeSet};
#[cfg(unix)]
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use cvx_core::{Error, IoResultExt, Result};
use cvx_ignore::WorktreeWalker;
#[cfg(unix)]
use cvx_index::MODE_EXECUTABLE;
use cvx_index::{IndexEntry, MODE_FILE, MODE_SYMLINK, path_to_bytes};
use cvx_object::{ObjectId, ObjectKind, hash_object};
use cvx_ref::Head;

use crate::repository::{Repository, RepositoryKind, TreeLeaf};

/// Options for `cvx status`.
#[derive(Debug, Clone)]
pub struct StatusOptions {
    /// Include untracked files.
    pub untracked: bool,
    /// Collapse untracked directories into a single entry.
    pub collapse_untracked: bool,
}

impl Default for StatusOptions {
    fn default() -> Self {
        Self {
            untracked: true,
            collapse_untracked: true,
        }
    }
}

/// A staged or unstaged change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusEntry {
    /// Repository-relative path bytes.
    pub path: Vec<u8>,
    /// Kind of change.
    pub kind: StatusKind,
}

/// Kind of change reported by status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    /// Path exists in one side but not the other.
    Added,
    /// Path exists on both sides with different content or mode.
    Modified,
    /// Path exists in the baseline but not in the compared side.
    Deleted,
    /// File type changed, for example file to symlink.
    TypeChanged,
}

/// An untracked path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UntrackedEntry {
    /// Repository-relative path bytes.
    pub path: Vec<u8>,
    /// Whether this entry represents a collapsed directory.
    pub is_dir: bool,
}

/// Result of comparing `HEAD`, the index and the worktree.
#[derive(Debug, Clone)]
pub struct Status {
    /// Current `HEAD` state.
    pub head: Head,
    /// Current branch name, when symbolic.
    pub branch: Option<String>,
    /// Whether the current branch has no commits.
    pub unborn: bool,
    /// Changes between `HEAD` and the index.
    pub staged: Vec<StatusEntry>,
    /// Changes between the index and the worktree.
    pub unstaged: Vec<StatusEntry>,
    /// Untracked paths.
    pub untracked: Vec<UntrackedEntry>,
    /// Paths with unmerged index stages.
    pub conflicts: Vec<Vec<u8>>,
}

impl Status {
    /// Returns whether there is nothing to report.
    pub fn is_clean(&self) -> bool {
        self.staged.is_empty()
            && self.unstaged.is_empty()
            && self.untracked.is_empty()
            && self.conflicts.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorktreeContent {
    Missing,
    Directory,
    File { mode: u32, oid: ObjectId },
}

impl Repository {
    /// Computes repository status.
    pub fn status(&self, options: StatusOptions) -> Result<Status> {
        let head = self.head()?;
        let branch = head
            .target()
            .and_then(|target| target.strip_prefix("refs/heads/"))
            .map(str::to_owned);
        let unborn = head.is_unborn();
        let filemode = self.filemode()?;

        let index = self.read_index()?;
        let index_paths: BTreeSet<Vec<u8>> = index
            .entries()
            .iter()
            .map(|entry| entry.path.clone())
            .collect();
        let conflict_paths: BTreeSet<Vec<u8>> = index
            .entries()
            .iter()
            .filter(|entry| entry.stage != 0)
            .map(|entry| entry.path.clone())
            .collect();
        let index_stage0: BTreeMap<Vec<u8>, &IndexEntry> = index
            .entries()
            .iter()
            .filter(|entry| entry.stage == 0)
            .map(|entry| (entry.path.clone(), entry))
            .collect();

        let head_leaves = match head.oid() {
            Some(oid) => {
                let commit = self.read_commit(oid)?;
                self.read_tree_recursive(commit.tree)?
            }
            None => BTreeMap::new(),
        };

        let mut staged = BTreeMap::new();
        for (path, entry) in &index_stage0 {
            if conflict_paths.contains(path) {
                continue;
            }
            match head_leaves.get(path) {
                None => {
                    staged.insert(path.clone(), StatusKind::Added);
                }
                Some(leaf) => {
                    if let Some(kind) = compare_head_and_index(leaf, entry) {
                        staged.insert(path.clone(), kind);
                    }
                }
            }
        }
        for path in head_leaves.keys() {
            if conflict_paths.contains(path) || index_stage0.contains_key(path) {
                continue;
            }
            staged.insert(path.clone(), StatusKind::Deleted);
        }

        let mut unstaged = BTreeMap::new();
        for (path, entry) in &index_stage0 {
            if conflict_paths.contains(path) || entry.assume_valid || entry.skip_worktree() {
                continue;
            }

            match worktree_content(self.work_tree(), path, filemode)? {
                WorktreeContent::Missing => {
                    unstaged.insert(path.clone(), StatusKind::Deleted);
                }
                WorktreeContent::Directory => {
                    unstaged.insert(path.clone(), StatusKind::TypeChanged);
                }
                WorktreeContent::File { mode, oid } => {
                    let mode_changed = if filemode {
                        mode != entry.mode
                    } else {
                        mode_bits(mode) != mode_bits(entry.mode)
                    };
                    if mode_bits(mode) != mode_bits(entry.mode) {
                        unstaged.insert(path.clone(), StatusKind::TypeChanged);
                    } else if mode_changed || oid != entry.oid {
                        unstaged.insert(path.clone(), StatusKind::Modified);
                    }
                }
            }
        }

        let untracked = if options.untracked {
            self.collect_untracked(
                &index,
                &index_paths,
                &conflict_paths,
                options.collapse_untracked,
            )?
        } else {
            Vec::new()
        };

        Ok(Status {
            head,
            branch,
            unborn,
            staged: into_entries(staged),
            unstaged: into_entries(unstaged),
            untracked,
            conflicts: conflict_paths.into_iter().collect(),
        })
    }

    fn collect_untracked(
        &self,
        index: &cvx_index::Index,
        index_paths: &BTreeSet<Vec<u8>>,
        conflict_paths: &BTreeSet<Vec<u8>>,
        collapse: bool,
    ) -> Result<Vec<UntrackedEntry>> {
        let walker = WorktreeWalker::new(self.work_tree(), self.control_dir())
            .read_git_exclude(self.kind() == RepositoryKind::Git);
        let entries = walker.walk(self.work_tree())?;

        let mut tracked_dirs = BTreeSet::new();
        for entry in index.entries() {
            let mut path = entry.path.as_slice();
            while let Some(position) = path.iter().rposition(|byte| *byte == b'/') {
                path = &path[..position];
                tracked_dirs.insert(path.to_vec());
            }
        }

        let mut untracked: BTreeMap<Vec<u8>, bool> = BTreeMap::new();
        for entry in entries {
            if entry.is_dir {
                continue;
            }
            let relative = match entry.path.strip_prefix(self.work_tree()) {
                Ok(path) => path_to_bytes(path)?,
                Err(_) => continue,
            };
            if index_paths.contains(&relative) || conflict_paths.contains(&relative) {
                continue;
            }

            if !collapse {
                untracked.insert(relative, false);
                continue;
            }

            let mut root = Vec::new();
            let mut current = relative.as_slice();
            while let Some(position) = current.iter().rposition(|byte| *byte == b'/') {
                let parent = &current[..position];
                if tracked_dirs.contains(parent) {
                    break;
                }
                root = parent.to_vec();
                current = parent;
            }

            if root.is_empty() {
                untracked.insert(relative, false);
            } else {
                untracked.insert(root, true);
            }
        }

        Ok(untracked
            .into_iter()
            .map(|(path, is_dir)| UntrackedEntry { path, is_dir })
            .collect())
    }
}

fn into_entries(entries: BTreeMap<Vec<u8>, StatusKind>) -> Vec<StatusEntry> {
    entries
        .into_iter()
        .map(|(path, kind)| StatusEntry { path, kind })
        .collect()
}

fn compare_head_and_index(leaf: &TreeLeaf, entry: &IndexEntry) -> Option<StatusKind> {
    if mode_bits(leaf.mode) != mode_bits(entry.mode) {
        Some(StatusKind::TypeChanged)
    } else if leaf.oid != entry.oid || leaf.mode != entry.mode {
        Some(StatusKind::Modified)
    } else {
        None
    }
}

fn mode_bits(mode: u32) -> u32 {
    mode & 0o170_000
}

fn worktree_content(work_tree: &Path, relative: &[u8], filemode: bool) -> Result<WorktreeContent> {
    let components: Vec<&[u8]> = relative.split(|byte| *byte == b'/').collect();
    if components.is_empty() {
        return Ok(WorktreeContent::Missing);
    }

    let mut current = work_tree.to_path_buf();
    for (position, component) in components.iter().enumerate() {
        push_component(&mut current, component)?;
        let metadata = match std::fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                return Ok(WorktreeContent::Missing);
            }
            Err(error) => {
                return Err(Error::io(current.display().to_string(), error));
            }
        };

        if position + 1 < components.len() {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Ok(WorktreeContent::Missing);
            }
            continue;
        }

        return classify_metadata(&current, &metadata, filemode);
    }

    Ok(WorktreeContent::Missing)
}

fn classify_metadata(
    path: &Path,
    metadata: &std::fs::Metadata,
    filemode: bool,
) -> Result<WorktreeContent> {
    let file_type = metadata.file_type();

    if file_type.is_symlink() {
        let target = std::fs::read_link(path).path_context(path)?;
        let target = path_to_bytes(&target)?;
        return Ok(WorktreeContent::File {
            mode: MODE_SYMLINK,
            oid: hash_object(ObjectKind::Blob, &target),
        });
    }

    if !file_type.is_file() {
        return Ok(WorktreeContent::Directory);
    }

    let content = std::fs::read(path).path_context(path)?;
    let mode = worktree_mode(metadata, filemode);
    Ok(WorktreeContent::File {
        mode,
        oid: hash_object(ObjectKind::Blob, &content),
    })
}

fn worktree_mode(metadata: &std::fs::Metadata, filemode: bool) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if filemode && metadata.permissions().mode() & 0o111 != 0 {
            return MODE_EXECUTABLE;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        let _ = filemode;
    }
    MODE_FILE
}

fn push_component(path: &mut PathBuf, component: &[u8]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        path.push(OsStr::from_bytes(component));
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let component = std::str::from_utf8(component).map_err(|_| Error::InvalidPath {
            path: String::from_utf8_lossy(component).into_owned(),
        })?;
        path.push(component);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AddOptions, CommitOptions, InitOptions};
    use std::path::PathBuf;

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
        configure_identity(&repository);
        repository
    }

    fn configure_identity(repository: &Repository) {
        let path = repository.control_dir().join("config");
        let mut config = std::fs::read_to_string(&path).expect("config");
        config.push_str("\n[user]\n\tname = Test User\n\temail = test@example.com\n");
        std::fs::write(path, config).expect("write config");
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

    fn commit(repository: &Repository, message: &str) -> cvx_object::ObjectId {
        repository
            .commit(CommitOptions {
                messages: vec![message.to_owned()],
                allow_empty: false,
            })
            .expect("commit")
            .oid
    }

    #[test]
    fn reports_untracked_and_staged_changes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repository = repository(temp.path());
        std::fs::write(temp.path().join("tracked.txt"), "tracked\n").expect("write");
        add_all(&repository, temp.path());

        let status = repository.status(StatusOptions::default()).expect("status");
        assert!(status.unborn);
        assert_eq!(status.staged.len(), 1);
        assert_eq!(status.staged[0].kind, StatusKind::Added);
        assert!(status.untracked.is_empty());

        commit(&repository, "initial");
        std::fs::write(temp.path().join("tracked.txt"), "changed\n").expect("write");
        std::fs::write(temp.path().join("new.txt"), "new\n").expect("write");

        let status = repository.status(StatusOptions::default()).expect("status");
        assert!(!status.is_clean());
        assert_eq!(status.unstaged[0].kind, StatusKind::Modified);
        assert!(
            status
                .untracked
                .iter()
                .any(|entry| entry.path == b"new.txt")
        );
    }

    #[test]
    fn reports_staged_deletion() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repository = repository(temp.path());
        std::fs::write(temp.path().join("file.txt"), "content\n").expect("write");
        add_all(&repository, temp.path());
        commit(&repository, "initial");

        std::fs::remove_file(temp.path().join("file.txt")).expect("remove");
        add_all(&repository, temp.path());

        let status = repository.status(StatusOptions::default()).expect("status");
        assert_eq!(status.staged.len(), 1);
        assert_eq!(status.staged[0].kind, StatusKind::Deleted);
        assert!(status.unstaged.is_empty());
    }

    #[test]
    fn collapses_untracked_directories() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repository = repository(temp.path());
        std::fs::write(temp.path().join("tracked.txt"), "tracked\n").expect("write");
        add_all(&repository, temp.path());
        commit(&repository, "initial");

        std::fs::create_dir_all(temp.path().join("src/deep")).expect("mkdir");
        std::fs::write(temp.path().join("src/deep/a.txt"), "a").expect("write");
        std::fs::write(temp.path().join("src/deep/b.txt"), "b").expect("write");

        let status = repository.status(StatusOptions::default()).expect("status");
        assert_eq!(status.untracked.len(), 1);
        assert_eq!(status.untracked[0].path, b"src");
        assert!(status.untracked[0].is_dir);
    }

    #[test]
    fn reports_type_changes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repository = repository(temp.path());
        std::fs::write(temp.path().join("file"), "content").expect("write");
        add_all(&repository, temp.path());
        commit(&repository, "initial");

        #[cfg(unix)]
        {
            std::fs::remove_file(temp.path().join("file")).expect("remove");
            std::os::unix::fs::symlink("target", temp.path().join("file")).expect("symlink");
            let status = repository.status(StatusOptions::default()).expect("status");
            assert_eq!(status.unstaged[0].kind, StatusKind::TypeChanged);
        }
    }

    #[test]
    fn ignores_gitignored_untracked_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repository = repository(temp.path());
        std::fs::write(temp.path().join(".gitignore"), "ignored.txt\n").expect("write");
        std::fs::write(temp.path().join("tracked.txt"), "tracked").expect("write");
        add_all(&repository, temp.path());
        commit(&repository, "initial");

        std::fs::write(temp.path().join("ignored.txt"), "ignored").expect("write");
        let status = repository.status(StatusOptions::default()).expect("status");
        assert!(status.unstaged.is_empty());
        assert!(status.untracked.is_empty());
    }
}
