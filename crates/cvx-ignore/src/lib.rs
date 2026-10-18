//! Gitignore rule loading and path matching.
//!
//! This crate wraps the `ignore` crate so the rest of CVX sees one deterministic
//! walker with Git semantics: hidden files are included, `.gitignore` files and
//! global excludes are honored, symbolic links are not followed, and the
//! repository control directory is never traversed.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use cvx_core::{Error, Result};
use ignore::WalkBuilder;

/// One entry produced by a worktree walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkEntry {
    /// Absolute or root-relative path as produced by the walker.
    pub path: PathBuf,
    /// Whether this entry is a directory.
    pub is_dir: bool,
}

/// Deterministic worktree walker.
#[derive(Debug, Clone)]
pub struct WorktreeWalker {
    root: PathBuf,
    control_dir: PathBuf,
    read_git_exclude: bool,
    respect_ignore: bool,
    reject_nested_repositories: bool,
}

impl WorktreeWalker {
    /// Creates a walker rooted at `root`.
    ///
    /// `control_dir` is the `.cvx` or `.git` directory and is always skipped.
    pub fn new(root: impl Into<PathBuf>, control_dir: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            control_dir: control_dir.into(),
            read_git_exclude: false,
            respect_ignore: true,
            reject_nested_repositories: false,
        }
    }

    /// Enables or disables `.gitignore` and global ignore rules.
    pub fn respect_ignore(mut self, enabled: bool) -> Self {
        self.respect_ignore = enabled;
        self
    }

    /// Rejects nested Git/CVX repositories instead of silently walking past
    /// their control directory.
    pub fn reject_nested_repositories(mut self, enabled: bool) -> Self {
        self.reject_nested_repositories = enabled;
        self
    }

    /// Enables reading `<control>/info/exclude`.
    ///
    /// The `ignore` crate looks for `.git/info/exclude`, so this is useful for
    /// compatibility mode. CVX's own `.cvx/info/exclude` is handled by the
    /// explicit control-directory filter.
    pub fn read_git_exclude(mut self, enabled: bool) -> Self {
        self.read_git_exclude = enabled;
        self
    }

    /// Walks from `start` and returns all non-ignored entries.
    pub fn walk(&self, start: &Path) -> Result<Vec<WalkEntry>> {
        let mut builder = WalkBuilder::new(start);
        builder
            .hidden(false)
            .parents(false)
            .ignore(self.respect_ignore)
            .git_ignore(self.respect_ignore)
            .git_global(self.respect_ignore)
            .git_exclude(self.respect_ignore && self.read_git_exclude)
            .require_git(false)
            .follow_links(false)
            .sort_by_file_path(|left, right| left.cmp(right));

        let control_dir = self.control_dir.clone();
        builder.filter_entry(move |entry| !is_within(entry.path(), &control_dir));

        let mut entries = Vec::new();
        for result in builder.build() {
            let entry = result.map_err(|error| {
                Error::Message(format!("failed to walk {}: {error}", self.root.display()))
            })?;

            if is_within(entry.path(), &self.control_dir) {
                continue;
            }
            if is_nested_control_dir(entry.path()) {
                if self.reject_nested_repositories {
                    return Err(Error::unsupported("nested Git repositories and submodules"));
                }
                continue;
            }

            let is_dir = entry
                .file_type()
                .map(|file_type| file_type.is_dir())
                .unwrap_or(false);
            entries.push(WalkEntry {
                path: entry.into_path(),
                is_dir,
            });
        }

        Ok(entries)
    }
}

fn is_within(path: &Path, directory: &Path) -> bool {
    path == directory || path.starts_with(directory)
}

fn is_nested_control_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == ".git" || name == ".cvx")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn includes_hidden_files_and_honors_gitignore() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::create_dir(temp.path().join(".hidden-dir")).expect("mkdir");
        fs::write(temp.path().join(".hidden"), "hidden").expect("write");
        fs::write(temp.path().join("tracked.txt"), "tracked").expect("write");
        fs::write(temp.path().join("ignored.txt"), "ignored").expect("write");
        fs::write(temp.path().join(".gitignore"), "ignored.txt\n").expect("write");

        let walker = WorktreeWalker::new(temp.path(), temp.path().join(".cvx"));
        let paths: Vec<String> = walker
            .walk(temp.path())
            .expect("walk")
            .into_iter()
            .filter(|entry| !entry.is_dir)
            .map(|entry| {
                entry
                    .path
                    .strip_prefix(temp.path())
                    .unwrap()
                    .display()
                    .to_string()
            })
            .collect();

        assert!(paths.contains(&".hidden".to_owned()));
        assert!(paths.contains(&"tracked.txt".to_owned()));
        assert!(!paths.contains(&"ignored.txt".to_owned()));
    }

    #[test]
    fn skips_control_directory() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::create_dir(temp.path().join(".cvx")).expect("mkdir");
        fs::write(temp.path().join(".cvx/HEAD"), "ref: refs/heads/main\n").expect("write");
        fs::write(temp.path().join("file.txt"), "content").expect("write");

        let walker = WorktreeWalker::new(temp.path(), temp.path().join(".cvx"));
        let paths: Vec<PathBuf> = walker
            .walk(temp.path())
            .expect("walk")
            .into_iter()
            .map(|entry| entry.path)
            .collect();

        assert!(
            !paths
                .iter()
                .any(|path| path.starts_with(temp.path().join(".cvx")))
        );
        assert!(paths.contains(&temp.path().join("file.txt")));
    }
}
