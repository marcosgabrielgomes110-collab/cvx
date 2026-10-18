use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use cvx_config::Config;
use cvx_core::{Error, IoResultExt, Result};
use cvx_object::HashAlgorithm;
use cvx_ref::{Head, RefStore};

/// Name of the CVX control directory.
pub const CVX_DIR: &str = ".cvx";

/// Name of the Git control directory used in compatibility mode.
pub const GIT_DIR: &str = ".git";

/// How a repository stores its control directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryKind {
    /// A CVX repository in `.cvx/`, using Git-compatible storage.
    Cvx,
    /// An existing Git repository in `.git/`, used in compatibility mode.
    Git,
}

impl RepositoryKind {
    /// Returns the directory name used by this kind.
    pub const fn dir_name(self) -> &'static str {
        match self {
            Self::Cvx => CVX_DIR,
            Self::Git => GIT_DIR,
        }
    }

    /// Returns a human-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cvx => "CVX",
            Self::Git => "Git",
        }
    }
}

/// An opened repository.
#[derive(Debug, Clone)]
pub struct Repository {
    control_dir: PathBuf,
    work_tree: PathBuf,
    kind: RepositoryKind,
}

impl Repository {
    /// Finds a repository by walking up from `start`.
    ///
    /// `CVX_DIR` overrides discovery for scripts and tests. A `.cvx` directory
    /// takes precedence over a sibling `.git` directory.
    pub fn discover(start: impl AsRef<Path>) -> Result<Self> {
        if let Some(directory) = env::var_os("CVX_DIR").filter(|value| !value.is_empty()) {
            return Self::open(PathBuf::from(directory));
        }

        let start = start.as_ref();
        let start = if start.as_os_str().is_empty() {
            Path::new(".")
        } else {
            start
        };
        let absolute = std::fs::canonicalize(start).path_context(start)?;
        let mut current = if absolute.is_dir() {
            absolute.as_path()
        } else {
            absolute.parent().unwrap_or(absolute.as_path())
        };

        loop {
            let cvx = current.join(CVX_DIR);
            let git = current.join(GIT_DIR);

            if cvx.is_dir() {
                return Self::open(&cvx);
            }
            if cvx.exists() {
                return Err(Error::unsupported(format!(
                    "{} exists but is not a directory",
                    cvx.display()
                )));
            }

            if git.is_dir() {
                return Self::open(&git);
            }
            if git.exists() {
                return Err(Error::unsupported(
                    "linked Git worktrees and submodule checkouts are not supported yet",
                ));
            }

            let Some(parent) = current.parent() else {
                break;
            };
            current = parent;
        }

        Err(Error::NotARepository {
            path: start.to_path_buf(),
        })
    }

    /// Opens an existing control directory.
    pub fn open(control_dir: impl AsRef<Path>) -> Result<Self> {
        let control_dir =
            std::fs::canonicalize(control_dir.as_ref()).path_context(control_dir.as_ref())?;
        if !control_dir.is_dir() {
            return Err(Error::NotARepository { path: control_dir });
        }

        let work_tree = control_dir
            .parent()
            .ok_or_else(|| {
                Error::Message(format!("{} has no parent directory", control_dir.display()))
            })?
            .to_path_buf();

        let kind = if control_dir.file_name() == Some(OsStr::new(CVX_DIR))
            || control_dir.join("cvx.toml").is_file()
        {
            RepositoryKind::Cvx
        } else {
            RepositoryKind::Git
        };

        let repository = Self {
            control_dir,
            work_tree,
            kind,
        };
        repository.validate_format()?;
        Ok(repository)
    }

    /// Returns the control directory (`.cvx` or `.git`).
    pub fn control_dir(&self) -> &Path {
        &self.control_dir
    }

    /// Returns the worktree root.
    pub fn work_tree(&self) -> &Path {
        &self.work_tree
    }

    /// Returns which control-directory convention is in use.
    pub fn kind(&self) -> RepositoryKind {
        self.kind
    }

    /// Returns a reference store for this repository.
    pub fn refs(&self) -> RefStore {
        RefStore::new(&self.control_dir)
    }

    /// Reads and resolves `HEAD`.
    pub fn head(&self) -> Result<Head> {
        self.refs().read_head()
    }

    /// Returns whether the current branch has no commits yet.
    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.head()?.is_unborn())
    }

    /// Loads the repository-local configuration.
    pub fn config(&self) -> Result<Config> {
        Config::load_file(self.control_dir.join("config"))
    }

    /// Returns the object format declared by the repository.
    pub fn hash_algorithm(&self) -> Result<HashAlgorithm> {
        let config = self.config()?;
        match config.get("extensions.objectformat") {
            Some(format) => {
                HashAlgorithm::parse(format).map_err(|error| Error::unsupported(error.to_string()))
            }
            None => Ok(HashAlgorithm::Sha1),
        }
    }

    fn validate_format(&self) -> Result<()> {
        let config = self.config()?;

        if config.get_bool("core.bare")?.unwrap_or(false) {
            return Err(Error::unsupported("bare repositories"));
        }

        let version = config
            .get("core.repositoryformatversion")
            .unwrap_or("0")
            .parse::<u32>()
            .map_err(|_| {
                Error::corrupt(format!(
                    "{} contains an invalid repository format version",
                    self.control_dir.join("config").display()
                ))
            })?;
        if version > 1 {
            return Err(Error::unsupported(format!(
                "repository format version {version}"
            )));
        }

        if let Some(object_format) = config.get("extensions.objectformat") {
            HashAlgorithm::parse(object_format)
                .map_err(|error| Error::unsupported(error.to_string()))?;
        }

        if let Some(reference_storage) = config.get("extensions.refstorage") {
            if !reference_storage.eq_ignore_ascii_case("files") {
                return Err(Error::unsupported(format!(
                    "reference storage '{reference_storage}'"
                )));
            }
        }

        if config.get("extensions.partialclone").is_some() {
            return Err(Error::unsupported("partial clone repositories"));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InitOptions, InitStorage};

    #[test]
    fn discovers_initialized_repository_from_nested_directory() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("project");
        let nested = root.join("src").join("module");
        std::fs::create_dir_all(&nested).expect("mkdir");

        let outcome = Repository::init(InitOptions {
            directory: root.clone(),
            initial_branch: None,
            storage: InitStorage::Cvx,
            quiet: true,
        })
        .expect("init");
        let expected = outcome.repository().control_dir().to_path_buf();

        let repository = Repository::discover(&nested).expect("discover");
        assert_eq!(repository.control_dir(), expected);
        assert_eq!(repository.work_tree(), root.canonicalize().unwrap());
        assert_eq!(repository.kind(), RepositoryKind::Cvx);
    }

    #[test]
    fn rejects_sha256_repositories() {
        let temp = tempfile::tempdir().expect("tempdir");
        let control = temp.path().join(".cvx");
        std::fs::create_dir_all(control.join("objects")).expect("mkdir");
        std::fs::write(
            control.join("config"),
            "[core]\n repositoryformatversion = 1\n[extensions]\n objectformat = sha256\n",
        )
        .expect("write config");

        let result = Repository::open(&control);
        assert!(matches!(result, Err(Error::Unsupported { .. })));
    }

    #[test]
    fn rejects_bare_repositories() {
        let temp = tempfile::tempdir().expect("tempdir");
        let control = temp.path().join(".cvx");
        std::fs::create_dir_all(control.join("objects")).expect("mkdir");
        std::fs::write(
            control.join("config"),
            "[core]\n repositoryformatversion = 0\n bare = true\n",
        )
        .expect("write config");

        let result = Repository::open(&control);
        assert!(matches!(result, Err(Error::Unsupported { .. })));
    }
}
