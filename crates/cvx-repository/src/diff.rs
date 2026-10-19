use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use cvx_core::{Error, IoResultExt, Result};
use cvx_diff::{DiffFile, DiffOptions as RenderOptions, DiffSide, render};
use cvx_index::path_to_bytes;
use cvx_object::{ObjectId, ObjectKind, hash_object};

use crate::repository::Repository;
use crate::worktree_ops::{WorktreeContent, read_content};

/// Options for `cvx diff`.
#[derive(Debug, Clone)]
pub struct DiffOptions {
    /// Compare HEAD with the index instead of the index with the worktree.
    pub cached: bool,
    /// Render a stat summary.
    pub stat: bool,
    /// Context lines.
    pub context: usize,
    /// Zero, one or two revisions.
    pub revisions: Vec<String>,
    /// Optional pathspecs after `--`.
    pub paths: Vec<PathBuf>,
    /// Current directory used to resolve pathspecs.
    pub cwd: PathBuf,
}

impl Default for DiffOptions {
    fn default() -> Self {
        Self {
            cached: false,
            stat: false,
            context: 3,
            revisions: Vec::new(),
            paths: Vec::new(),
            cwd: PathBuf::from("."),
        }
    }
}

#[derive(Debug, Clone)]
struct SnapshotEntry {
    mode: u32,
    oid: ObjectId,
    content: Option<Vec<u8>>,
}

type Snapshot = BTreeMap<Vec<u8>, SnapshotEntry>;

impl Repository {
    /// Computes and renders a diff.
    pub fn diff(&self, options: &DiffOptions) -> Result<String> {
        if options.revisions.len() > 2 {
            return Err(Error::Message(
                "cvx diff accepts at most two revisions".to_owned(),
            ));
        }

        let pathspecs = self.resolve_diff_pathspecs(options)?;
        let index = self.normalized_index_snapshot()?;

        let (old, new) = match options.revisions.len() {
            0 => {
                if options.cached {
                    let head = self.head_tree_snapshot()?;
                    (head, index)
                } else {
                    let paths: BTreeSet<Vec<u8>> = index.keys().cloned().collect();
                    let worktree = self.worktree_snapshot(&paths, &pathspecs)?;
                    (index, worktree)
                }
            }
            1 => {
                let revision = self.resolve_revision(&options.revisions[0])?;
                let commit = self.read_commit(revision)?;
                let tree = self.diff_tree_snapshot(commit.tree)?;
                if options.cached {
                    (tree, index)
                } else {
                    let mut paths: BTreeSet<Vec<u8>> = tree.keys().cloned().collect();
                    paths.extend(index.keys().cloned());
                    let worktree = self.worktree_snapshot(&paths, &pathspecs)?;
                    (tree, worktree)
                }
            }
            2 => {
                if options.cached {
                    return Err(Error::Message(
                        "--cached cannot be combined with two revisions".to_owned(),
                    ));
                }
                let first = self.resolve_revision(&options.revisions[0])?;
                let second = self.resolve_revision(&options.revisions[1])?;
                let old = self.diff_tree_snapshot(self.read_commit(first)?.tree)?;
                let new = self.diff_tree_snapshot(self.read_commit(second)?.tree)?;
                (old, new)
            }
            _ => unreachable!("length is checked above"),
        };

        let files = self.collect_diff_files(&old, &new, &pathspecs)?;

        Ok(render(
            &files,
            &RenderOptions {
                context: options.context,
                stat: options.stat,
            },
        ))
    }

    fn normalized_index_snapshot(&self) -> Result<Snapshot> {
        let index = self.read_index()?;
        if index.has_conflicts() {
            return Err(Error::Message(
                "diff with unmerged index entries is not supported yet".to_owned(),
            ));
        }

        Ok(index
            .entries()
            .iter()
            .filter(|entry| entry.stage == 0)
            .map(|entry| {
                (
                    entry.path.clone(),
                    SnapshotEntry {
                        mode: entry.mode,
                        oid: entry.oid,
                        content: None,
                    },
                )
            })
            .collect())
    }

    fn head_tree_snapshot(&self) -> Result<Snapshot> {
        match self.head_commit()? {
            Some(commit) => self.diff_tree_snapshot(commit.tree),
            None => Ok(BTreeMap::new()),
        }
    }

    fn diff_tree_snapshot(&self, tree: ObjectId) -> Result<Snapshot> {
        Ok(self
            .read_tree_recursive(tree)?
            .into_iter()
            .map(|(path, leaf)| {
                (
                    path,
                    SnapshotEntry {
                        mode: leaf.mode,
                        oid: leaf.oid,
                        content: None,
                    },
                )
            })
            .collect())
    }

    fn worktree_snapshot(
        &self,
        paths: &BTreeSet<Vec<u8>>,
        pathspecs: &[Vec<u8>],
    ) -> Result<Snapshot> {
        let filemode = self.filemode()?;
        let mut snapshot = Snapshot::new();

        for path in paths {
            if !pathspec_matches(path, pathspecs) {
                continue;
            }
            match read_content(self.work_tree(), path, filemode)? {
                WorktreeContent::Missing | WorktreeContent::Directory => {}
                WorktreeContent::File { mode, content } => {
                    let oid = hash_object(ObjectKind::Blob, &content);
                    snapshot.insert(
                        path.clone(),
                        SnapshotEntry {
                            mode,
                            oid,
                            content: Some(content),
                        },
                    );
                }
            }
        }

        Ok(snapshot)
    }

    fn collect_diff_files(
        &self,
        old: &Snapshot,
        new: &Snapshot,
        pathspecs: &[Vec<u8>],
    ) -> Result<Vec<DiffFile>> {
        let mut paths: BTreeSet<Vec<u8>> = old.keys().cloned().collect();
        paths.extend(new.keys().cloned());

        let mut files = Vec::new();
        let mut deleted = Vec::new();
        let mut added = Vec::new();

        for path in paths {
            if !pathspec_matches(&path, pathspecs) {
                continue;
            }

            let old_entry = old.get(&path);
            let new_entry = new.get(&path);
            match (old_entry, new_entry) {
                (Some(old), Some(new)) => {
                    if old.mode == new.mode && old.oid == new.oid {
                        continue;
                    }
                    files.push(DiffFile {
                        path,
                        old_path: None,
                        similarity: None,
                        old: Some(self.diff_side(old)?),
                        new: Some(self.diff_side(new)?),
                    });
                }
                (Some(old), None) => deleted.push((path, self.diff_side(old)?)),
                (None, Some(new)) => added.push((path, self.diff_side(new)?)),
                (None, None) => unreachable!("path came from one of the snapshots"),
            }
        }

        let mut candidates = Vec::new();
        for (deleted_index, (_, old_side)) in deleted.iter().enumerate() {
            for (added_index, (_, new_side)) in added.iter().enumerate() {
                let score = cvx_diff::similarity(&old_side.content, &new_side.content);
                if score >= 0.5 {
                    candidates.push((score, deleted_index, added_index));
                }
            }
        }
        candidates.sort_by(|left, right| {
            right
                .0
                .partial_cmp(&left.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| deleted[left.1].0.cmp(&deleted[right.1].0))
                .then_with(|| added[left.2].0.cmp(&added[right.2].0))
        });

        let mut deleted_used = vec![false; deleted.len()];
        let mut added_used = vec![false; added.len()];
        for (score, deleted_index, added_index) in candidates {
            if deleted_used[deleted_index] || added_used[added_index] {
                continue;
            }
            deleted_used[deleted_index] = true;
            added_used[added_index] = true;

            let (old_path, old_side) = &deleted[deleted_index];
            let (new_path, new_side) = &added[added_index];
            files.push(DiffFile {
                path: new_path.clone(),
                old_path: Some(old_path.clone()),
                similarity: Some((score * 100.0).round().clamp(1.0, 100.0) as u8),
                old: Some(old_side.clone()),
                new: Some(new_side.clone()),
            });
        }

        for (index, (path, side)) in deleted.into_iter().enumerate() {
            if !deleted_used[index] {
                files.push(DiffFile {
                    path,
                    old_path: None,
                    similarity: None,
                    old: Some(side),
                    new: None,
                });
            }
        }
        for (index, (path, side)) in added.into_iter().enumerate() {
            if !added_used[index] {
                files.push(DiffFile {
                    path,
                    old_path: None,
                    similarity: None,
                    old: None,
                    new: Some(side),
                });
            }
        }

        files.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(files)
    }

    fn diff_side(&self, entry: &SnapshotEntry) -> Result<DiffSide> {
        let content = match &entry.content {
            Some(content) => content.clone(),
            None => {
                let object = self.read_object(entry.oid)?;
                match object.kind {
                    ObjectKind::Blob => object.data,
                    ObjectKind::Commit if entry.mode & 0o170_000 == 0o160_000 => {
                        return Err(Error::unsupported("diffing submodule entries"));
                    }
                    other => {
                        return Err(Error::corrupt(format!(
                            "diff path references a {other}, not a blob"
                        )));
                    }
                }
            }
        };

        Ok(DiffSide {
            mode: entry.mode,
            oid: entry.oid,
            content,
        })
    }

    fn resolve_diff_pathspecs(&self, options: &DiffOptions) -> Result<Vec<Vec<u8>>> {
        if options.paths.is_empty() {
            return Ok(Vec::new());
        }

        let cwd = std::fs::canonicalize(&options.cwd).path_context(&options.cwd)?;
        let work_tree = self.work_tree();
        let mut specs = Vec::new();

        for path in &options.paths {
            let absolute = lexical_absolute(&cwd, path);
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

fn pathspec_matches(path: &[u8], specs: &[Vec<u8>]) -> bool {
    specs.is_empty() || specs.iter().any(|spec| path_matches(path, spec))
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

    #[test]
    fn diffs_index_against_worktree() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repository = repository(temp.path());
        std::fs::write(temp.path().join("file.txt"), "one\n").expect("write");
        add_all(&repository, temp.path());
        commit(&repository, "first");
        std::fs::write(temp.path().join("file.txt"), "two\n").expect("write");

        let diff = repository
            .diff(&DiffOptions {
                cwd: temp.path().to_path_buf(),
                ..DiffOptions::default()
            })
            .expect("diff");
        assert!(diff.contains("-one"));
        assert!(diff.contains("+two"));
    }

    #[test]
    fn cached_diff_compares_head_and_index() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repository = repository(temp.path());
        std::fs::write(temp.path().join("file.txt"), "one\n").expect("write");
        add_all(&repository, temp.path());
        commit(&repository, "first");
        std::fs::write(temp.path().join("file.txt"), "two\n").expect("write");
        add_all(&repository, temp.path());

        let diff = repository
            .diff(&DiffOptions {
                cached: true,
                cwd: temp.path().to_path_buf(),
                ..DiffOptions::default()
            })
            .expect("diff");
        assert!(diff.contains("-one"));
        assert!(diff.contains("+two"));
    }

    #[test]
    fn filters_by_pathspec() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repository = repository(temp.path());
        std::fs::write(temp.path().join("a.txt"), "a\n").expect("write");
        std::fs::write(temp.path().join("b.txt"), "b\n").expect("write");
        add_all(&repository, temp.path());
        commit(&repository, "first");
        std::fs::write(temp.path().join("a.txt"), "a changed\n").expect("write");
        std::fs::write(temp.path().join("b.txt"), "b changed\n").expect("write");

        let diff = repository
            .diff(&DiffOptions {
                paths: vec![PathBuf::from("a.txt")],
                cwd: temp.path().to_path_buf(),
                ..DiffOptions::default()
            })
            .expect("diff");
        assert!(diff.contains("a.txt"));
        assert!(!diff.contains("b.txt"));
    }
}
