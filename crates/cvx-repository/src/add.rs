use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use cvx_core::{Error, IoResultExt, Result};
use cvx_ignore::WorktreeWalker;
use cvx_index::{IndexEntry, IndexLock, path_to_bytes};
use cvx_object::ObjectKind;

use crate::repository::{Repository, RepositoryKind};

/// Options for `cvx add`.
#[derive(Debug, Clone)]
pub struct AddOptions {
    /// Pathspecs relative to `cwd`, as typed by the user.
    pub paths: Vec<PathBuf>,
    /// Current working directory used to resolve relative pathspecs.
    pub cwd: PathBuf,
    /// Include ignored files.
    pub force: bool,
    /// Update tracked files only.
    pub update: bool,
    /// Stage deletions as well as additions (the default behavior for paths).
    pub all: bool,
}

/// Summary of a staging operation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AddOutcome {
    /// Newly staged paths.
    pub added: usize,
    /// Paths whose staged content changed.
    pub modified: usize,
    /// Paths staged for deletion.
    pub removed: usize,
    /// Paths that were already staged with identical content.
    pub unchanged: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathspecKind {
    File,
    Directory,
    Missing,
}

#[derive(Debug, Clone)]
struct Pathspec {
    relative: Vec<u8>,
    absolute: PathBuf,
    kind: PathspecKind,
    display: String,
}

impl Repository {
    /// Stages paths into the index.
    pub fn add(&self, options: AddOptions) -> Result<AddOutcome> {
        if options.paths.is_empty() {
            return Err(Error::Message(
                "nothing specified, nothing added".to_owned(),
            ));
        }

        if let Some(value) = self.config_value("core.autocrlf")? {
            if matches!(
                value.to_ascii_lowercase().as_str(),
                "true" | "input" | "always"
            ) {
                return Err(Error::unsupported(
                    "core.autocrlf text normalization is not implemented yet",
                ));
            }
        }

        let cwd = std::fs::canonicalize(&options.cwd).path_context(&options.cwd)?;
        let work_tree = self.work_tree();
        let filemode = self.filemode()?;

        let mut specs = Vec::with_capacity(options.paths.len());
        for path in &options.paths {
            specs.push(self.resolve_pathspec(&cwd, path)?);
        }

        let lock = IndexLock::acquire(self.control_dir())?;
        let mut index = self.read_index()?;
        let mut outcome = AddOutcome::default();
        let mut seen = HashSet::new();
        let missing_matches: Vec<bool> = specs
            .iter()
            .map(|spec| {
                spec.kind == PathspecKind::Missing
                    && index
                        .entries()
                        .iter()
                        .any(|entry| is_under(&entry.path, &spec.relative))
            })
            .collect();

        let walk_root = specs
            .iter()
            .any(|spec| spec.kind == PathspecKind::Directory);
        if walk_root {
            let walker = WorktreeWalker::new(work_tree, self.control_dir())
                .read_git_exclude(self.kind() == RepositoryKind::Git)
                .respect_ignore(!options.force)
                .reject_nested_repositories(true);

            for entry in walker.walk(work_tree)? {
                if entry.is_dir {
                    continue;
                }
                let relative = match entry.path.strip_prefix(work_tree) {
                    Ok(path) => path_to_bytes(path)?,
                    Err(_) => continue,
                };
                if contains_control_component(&relative) {
                    continue;
                }
                if !specs.iter().any(|spec| {
                    spec.kind == PathspecKind::Directory && is_under(&relative, &spec.relative)
                }) {
                    continue;
                }
                if options.update && index.get(&relative).is_none() {
                    continue;
                }
                if seen.insert(relative.clone()) {
                    self.stage_path(&mut index, &entry.path, relative, filemode, &mut outcome)?;
                }
            }
        }

        for spec in &specs {
            if spec.kind != PathspecKind::File {
                continue;
            }
            if options.update && index.get(&spec.relative).is_none() {
                continue;
            }
            if seen.insert(spec.relative.clone()) {
                self.stage_path(
                    &mut index,
                    &spec.absolute,
                    spec.relative.clone(),
                    filemode,
                    &mut outcome,
                )?;
            }
        }

        let entries = index.entries().to_vec();
        for entry in entries {
            if !specs.iter().any(|spec| {
                matches!(spec.kind, PathspecKind::Directory | PathspecKind::Missing)
                    && is_under(&entry.path, &spec.relative)
            }) {
                continue;
            }

            let path = entry.path_buf()?;
            let absolute = work_tree.join(path);
            match std::fs::symlink_metadata(&absolute) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    index.remove_path(&entry.path);
                    outcome.removed += 1;
                }
                Err(error) => {
                    return Err(Error::io(absolute.display().to_string(), error));
                }
                Ok(_) => {}
            }
        }

        for (spec, matched) in specs.iter().zip(missing_matches) {
            if spec.kind == PathspecKind::Missing && !matched {
                return Err(Error::Message(format!(
                    "pathspec '{}' did not match any files",
                    spec.display
                )));
            }
        }

        lock.commit(&index)?;
        Ok(outcome)
    }

    fn resolve_pathspec(&self, cwd: &Path, path: &Path) -> Result<Pathspec> {
        let absolute = lexical_absolute(cwd, path);
        let work_tree = self.work_tree();

        if !absolute.starts_with(work_tree) {
            return Err(Error::Message(format!(
                "pathspec '{}' is outside the repository",
                path.display()
            )));
        }

        let relative_path = absolute
            .strip_prefix(work_tree)
            .expect("starts_with checked");
        let relative = if relative_path.as_os_str().is_empty() {
            Vec::new()
        } else {
            path_to_bytes(relative_path)?
        };

        if contains_control_component(&relative) {
            return Err(Error::Message(format!(
                "cannot add repository control path '{}'",
                path.display()
            )));
        }
        ensure_no_nested_control(work_tree, &relative)?;

        let kind = match std::fs::symlink_metadata(&absolute) {
            Ok(metadata) if metadata.is_dir() => PathspecKind::Directory,
            Ok(_) => PathspecKind::File,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => PathspecKind::Missing,
            Err(error) => {
                return Err(Error::io(absolute.display().to_string(), error));
            }
        };

        Ok(Pathspec {
            relative,
            absolute,
            kind,
            display: path.display().to_string(),
        })
    }

    fn stage_path(
        &self,
        index: &mut cvx_index::Index,
        absolute: &Path,
        relative: Vec<u8>,
        filemode: bool,
        outcome: &mut AddOutcome,
    ) -> Result<()> {
        let metadata = std::fs::symlink_metadata(absolute).path_context(absolute)?;
        if metadata.is_dir() {
            return Ok(());
        }
        if !metadata.is_file() && !metadata.file_type().is_symlink() {
            return Err(Error::Message(format!(
                "unsupported file type: {}",
                absolute.display()
            )));
        }

        let content = if metadata.file_type().is_symlink() {
            let target = std::fs::read_link(absolute).path_context(absolute)?;
            path_to_bytes(&target)?
        } else {
            std::fs::read(absolute).path_context(absolute)?
        };

        let oid = self.objects().write(ObjectKind::Blob, &content)?;
        let entry = IndexEntry::from_metadata(relative.clone(), oid, &metadata, filemode)?;
        let mode = entry.mode;
        let previous = index.get_stage(&relative, 0).cloned();
        if index
            .entries()
            .iter()
            .any(|entry| entry.path == relative && entry.stage != 0)
        {
            index.remove_path(&relative);
        }
        index.insert(entry)?;

        match previous {
            None => outcome.added += 1,
            Some(previous) if previous.oid != oid || previous.mode != mode => {
                outcome.modified += 1;
            }
            Some(_) => outcome.unchanged += 1,
        }

        Ok(())
    }
}

fn is_under(path: &[u8], spec: &[u8]) -> bool {
    if spec.is_empty() {
        return true;
    }
    if path == spec {
        return true;
    }
    path.len() > spec.len() && path.starts_with(spec) && path[spec.len()] == b'/'
}

fn contains_control_component(path: &[u8]) -> bool {
    path.split(|byte| *byte == b'/')
        .any(|component| component == b".cvx" || component == b".git")
}

fn ensure_no_nested_control(work_tree: &Path, relative: &[u8]) -> Result<()> {
    let components: Vec<&[u8]> = relative.split(|byte| *byte == b'/').collect();
    let mut prefix = Vec::new();

    for component in &components[..components.len().saturating_sub(1)] {
        if !prefix.is_empty() {
            prefix.push(b'/');
        }
        prefix.extend_from_slice(component);

        let directory = work_tree.join(cvx_index::path_from_bytes(&prefix)?);
        if directory.join(".git").exists() || directory.join(".cvx").exists() {
            return Err(Error::unsupported("nested Git repositories and submodules"));
        }
    }

    Ok(())
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
    use crate::InitOptions;

    #[test]
    fn matches_directories_and_files() {
        assert!(is_under(b"src/main.rs", b""));
        assert!(is_under(b"src/main.rs", b"src"));
        assert!(is_under(b"src", b"src"));
        assert!(!is_under(b"src2/main.rs", b"src"));
        assert!(!is_under(b"other", b"src"));
    }

    #[test]
    fn rejects_control_components() {
        assert!(contains_control_component(b".cvx/HEAD"));
        assert!(contains_control_component(b"nested/.git/config"));
        assert!(!contains_control_component(b"src/.gitignore"));
    }

    #[test]
    fn normalizes_lexical_paths() {
        let cwd = Path::new("/repo/src");
        assert_eq!(
            lexical_absolute(cwd, Path::new("../README.md")),
            PathBuf::from("/repo/README.md")
        );
        assert_eq!(
            lexical_absolute(cwd, Path::new("./main.rs")),
            PathBuf::from("/repo/src/main.rs")
        );
    }

    #[test]
    fn add_stages_files_and_deletions() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repository = match Repository::init(InitOptions {
            directory: temp.path().to_path_buf(),
            ..InitOptions::default()
        })
        .expect("init")
        {
            crate::InitOutcome::Created(repository) => repository,
            _ => panic!("expected new repository"),
        };

        std::fs::write(temp.path().join("hello.txt"), "hello\n").expect("write");
        let outcome = repository
            .add(AddOptions {
                paths: vec![PathBuf::from(".")],
                cwd: temp.path().to_path_buf(),
                force: false,
                update: false,
                all: true,
            })
            .expect("add");
        assert_eq!(outcome.added, 1);

        let index = repository.read_index().expect("index");
        assert_eq!(index.entries().len(), 1);
        assert_eq!(
            index.get(b"hello.txt").expect("entry").oid,
            cvx_object::hash_object(ObjectKind::Blob, b"hello\n")
        );

        std::fs::remove_file(temp.path().join("hello.txt")).expect("remove");
        let outcome = repository
            .add(AddOptions {
                paths: vec![PathBuf::from(".")],
                cwd: temp.path().to_path_buf(),
                force: false,
                update: false,
                all: true,
            })
            .expect("add deletion");
        assert_eq!(outcome.removed, 1);
        assert!(repository.read_index().expect("index").is_empty());
    }

    #[test]
    fn add_rejects_missing_path() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repository = match Repository::init(InitOptions {
            directory: temp.path().to_path_buf(),
            ..InitOptions::default()
        })
        .expect("init")
        {
            crate::InitOutcome::Created(repository) => repository,
            _ => panic!("expected new repository"),
        };

        let error = repository
            .add(AddOptions {
                paths: vec![PathBuf::from("missing")],
                cwd: temp.path().to_path_buf(),
                force: false,
                update: false,
                all: true,
            })
            .expect_err("missing path");
        assert!(error.to_string().contains("did not match any files"));
    }

    #[test]
    fn add_honors_gitignore() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repository = match Repository::init(InitOptions {
            directory: temp.path().to_path_buf(),
            ..InitOptions::default()
        })
        .expect("init")
        {
            crate::InitOutcome::Created(repository) => repository,
            _ => panic!("expected new repository"),
        };

        std::fs::write(temp.path().join("tracked.txt"), "tracked").expect("write");
        std::fs::write(temp.path().join("ignored.txt"), "ignored").expect("write");
        std::fs::write(temp.path().join(".gitignore"), "ignored.txt\n").expect("write");

        repository
            .add(AddOptions {
                paths: vec![PathBuf::from(".")],
                cwd: temp.path().to_path_buf(),
                force: false,
                update: false,
                all: true,
            })
            .expect("add");

        let index = repository.read_index().expect("index");
        assert!(index.get(b"tracked.txt").is_some());
        assert!(index.get(b".gitignore").is_some());
        assert!(index.get(b"ignored.txt").is_none());
    }
}
