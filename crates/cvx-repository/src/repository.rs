use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use cvx_config::Config;
use cvx_core::{Error, IoResultExt, Result};
use cvx_index::Index;
use cvx_object::{Commit, HashAlgorithm, ObjectId, ObjectKind, Tree};
use cvx_odb::{ObjectStore, StoredObject};
use cvx_ref::{Head, RefStore};
use cvx_transport::NetworkTransport;

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

/// A non-tree leaf in a recursively expanded tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeLeaf {
    /// File or link mode.
    pub mode: u32,
    /// Referenced blob or gitlink.
    pub oid: ObjectId,
}

/// An opened repository.
#[derive(Debug, Clone)]
pub struct Repository {
    control_dir: PathBuf,
    work_tree: PathBuf,
    kind: RepositoryKind,
    objects: Rc<ObjectStore>,
    algorithm: HashAlgorithm,
    bare: bool,
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
            if is_bare_repository(current) {
                return Self::open(current);
            }

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

        let config = Config::load_file(control_dir.join("config"))?;
        let bare = config.get_bool("core.bare")?.unwrap_or(false);
        let work_tree = if bare {
            control_dir.clone()
        } else {
            control_dir
                .parent()
                .ok_or_else(|| {
                    Error::Message(format!("{} has no parent directory", control_dir.display()))
                })?
                .to_path_buf()
        };

        let kind = if control_dir.file_name() == Some(OsStr::new(CVX_DIR))
            || control_dir.join("cvx.toml").is_file()
        {
            RepositoryKind::Cvx
        } else {
            RepositoryKind::Git
        };

        let algorithm = config
            .get("extensions.objectformat")
            .map(HashAlgorithm::parse)
            .transpose()
            .map_err(|error| Error::unsupported(error.to_string()))?
            .unwrap_or_default();
        let objects = Rc::new(ObjectStore::open_with_algorithm(
            control_dir.join("objects"),
            algorithm,
        )?);
        let repository = Self {
            control_dir,
            work_tree,
            kind,
            objects,
            algorithm,
            bare,
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

    /// Returns whether this is a bare repository without a worktree.
    pub fn is_bare(&self) -> bool {
        self.bare
    }

    /// Rejects operations that require a worktree.
    pub fn require_work_tree(&self) -> Result<()> {
        if self.bare {
            return Err(Error::Message(
                "this operation must be run in a work tree".to_owned(),
            ));
        }
        Ok(())
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
    pub fn hash_algorithm(&self) -> HashAlgorithm {
        self.algorithm
    }

    /// Returns the object database for this repository.
    pub fn objects(&self) -> &ObjectStore {
        &self.objects
    }

    /// Returns the path to the index file.
    pub fn index_path(&self) -> PathBuf {
        self.control_dir.join("index")
    }

    /// Reads the index, returning an empty index when it does not exist.
    pub fn read_index(&self) -> Result<Index> {
        Ok(
            Index::read_from_with_algorithm(&self.index_path(), self.algorithm)?
                .unwrap_or_else(|| Index::new_with_algorithm(self.algorithm)),
        )
    }

    /// Reads and verifies an object.
    ///
    /// In a partial clone, missing objects are fetched from the promisor
    /// remote and stored before being returned.
    pub fn read_object(&self, oid: ObjectId) -> Result<StoredObject> {
        match self.objects().read(oid) {
            Ok(object) => Ok(object),
            Err(Error::ObjectNotFound { .. }) => {
                let Some(url) = self.partial_clone_url()? else {
                    return Err(Error::object_not_found(oid.to_string()));
                };
                let transport = NetworkTransport::open(&url)?;
                let response = transport.fetch_full_to_file(
                    &[oid],
                    None,
                    &[],
                    Some("blob:none"),
                    self.hash_algorithm(),
                    true,
                )?;
                let pack = std::fs::read(&response.pack_path)
                    .map_err(|error| Error::io(response.pack_path.display().to_string(), error));
                let _ = std::fs::remove_file(&response.pack_path);
                let pack = pack?;
                for object in self.ingest_pack_with_local_bases(&pack, self.hash_algorithm())? {
                    self.objects().write(object.kind, &object.data)?;
                }
                self.objects().read(oid)
            }
            Err(error) => Err(error),
        }
    }

    pub(crate) fn ingest_pack_with_local_bases(
        &self,
        pack: &[u8],
        algorithm: HashAlgorithm,
    ) -> Result<Vec<cvx_pack::IngestedObject>> {
        cvx_pack::ingest_pack_with_bases(pack, algorithm, |oid| match self.objects().read(oid) {
            Ok(object) => Ok(Some(cvx_pack::IngestedObject {
                oid,
                kind: object.kind,
                data: object.data,
            })),
            Err(Error::ObjectNotFound { .. }) => Ok(None),
            Err(error) => Err(error),
        })
    }

    /// Returns the promisor remote URL for a partial clone.
    pub fn partial_clone_url(&self) -> Result<Option<String>> {
        let config = self.config()?;
        let Some(remote) = config.get("cvx.partialclone") else {
            return Ok(None);
        };
        self.remote_url(remote)
    }

    /// Reads and parses a commit object.
    pub fn read_commit(&self, oid: ObjectId) -> Result<Commit> {
        let object = self.read_object(oid)?;
        if object.kind != ObjectKind::Commit {
            return Err(Error::corrupt(format!(
                "object {oid} is a {}, not a commit",
                object.kind
            )));
        }
        Commit::parse(&object.data)
    }

    /// Reads and parses a tree object.
    pub fn read_tree(&self, oid: ObjectId) -> Result<Tree> {
        let object = self.read_object(oid)?;
        if object.kind != ObjectKind::Tree {
            return Err(Error::corrupt(format!(
                "object {oid} is a {}, not a tree",
                object.kind
            )));
        }
        Tree::parse_with_algorithm(&object.data, self.algorithm)
    }

    /// Reads `HEAD`'s commit, if the current branch has one.
    pub fn head_commit(&self) -> Result<Option<Commit>> {
        let Some(oid) = self.head()?.oid() else {
            return Ok(None);
        };
        Ok(Some(self.read_commit(oid)?))
    }

    /// Recursively expands a tree into path-to-leaf entries.
    pub fn read_tree_recursive(&self, oid: ObjectId) -> Result<BTreeMap<Vec<u8>, TreeLeaf>> {
        let mut leaves = BTreeMap::new();
        let mut expanded = 0usize;
        self.expand_tree(oid, &[], &mut leaves, 0, &mut expanded)?;
        Ok(leaves)
    }

    fn expand_tree(
        &self,
        oid: ObjectId,
        prefix: &[u8],
        leaves: &mut BTreeMap<Vec<u8>, TreeLeaf>,
        depth: usize,
        expanded: &mut usize,
    ) -> Result<()> {
        if depth > 64 {
            return Err(Error::corrupt("tree nesting is too deep"));
        }

        for entry in self.read_tree(oid)?.entries() {
            *expanded = expanded
                .checked_add(1)
                .filter(|value| *value <= 1_000_000)
                .ok_or_else(|| Error::corrupt("tree expands to too many entries"))?;
            let mut path = prefix.to_vec();
            if !path.is_empty() {
                path.push(b'/');
            }
            path.extend_from_slice(&entry.name);

            if entry.is_tree() {
                self.expand_tree(entry.oid, &path, leaves, depth + 1, expanded)?;
            } else {
                leaves.insert(
                    path,
                    TreeLeaf {
                        mode: entry.mode,
                        oid: entry.oid,
                    },
                );
            }
        }

        Ok(())
    }

    /// Returns the short current branch name, if `HEAD` is symbolic.
    pub fn branch_name(&self) -> Result<Option<String>> {
        Ok(self
            .head()?
            .target()
            .and_then(|target| target.strip_prefix("refs/heads/"))
            .map(str::to_owned))
    }

    /// Resolves a configuration value, preferring the local repository.
    pub fn config_value(&self, key: &str) -> Result<Option<String>> {
        if let Some(value) = self.config()?.get(key) {
            return Ok(Some(value.to_owned()));
        }
        Ok(cvx_config::load_global()?.get(key).map(str::to_owned))
    }

    /// Resolves a boolean configuration value, preferring the local repository.
    pub fn config_bool(&self, key: &str) -> Result<Option<bool>> {
        if let Some(value) = self.config()?.get_bool(key)? {
            return Ok(Some(value));
        }
        cvx_config::load_global()?.get_bool(key)
    }

    /// Returns whether executable bits should be honored.
    pub fn filemode(&self) -> Result<bool> {
        Ok(self.config_bool("core.filemode")?.unwrap_or(cfg!(unix)))
    }

    /// Loads the root `.gitattributes` file.
    pub fn attributes(&self) -> Result<cvx_attributes::AttributeSet> {
        cvx_attributes::AttributeSet::load(self.work_tree())
    }

    /// Returns the effective CRLF policy from configuration.
    pub fn autocrlf(&self) -> Result<cvx_attributes::AutoCrlf> {
        let Some(value) = self.config_value("core.autocrlf")? else {
            return Ok(cvx_attributes::AutoCrlf::False);
        };
        cvx_attributes::AutoCrlf::parse(&value)
            .ok_or_else(|| Error::Message(format!("invalid core.autocrlf value: {value}")))
    }

    /// Returns effective attributes for a repository-relative path.
    pub fn effective_attributes_for(
        &self,
        path: &[u8],
        attributes: &cvx_attributes::AttributeSet,
    ) -> Result<cvx_attributes::Attributes> {
        Ok(cvx_attributes::effective_attributes(
            &attributes.for_path(path),
            self.autocrlf()?,
        ))
    }

    /// Returns the shallow boundaries recorded in the repository.
    pub fn shallow_commits(&self) -> Result<BTreeSet<ObjectId>> {
        let path = self.control_dir().join("shallow");
        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(BTreeSet::new());
            }
            Err(error) => return Err(Error::io(path.display().to_string(), error)),
        };
        let mut boundaries = BTreeSet::new();
        for line in contents.lines() {
            if line.trim().is_empty() {
                continue;
            }
            boundaries.insert(
                ObjectId::from_hex(line.trim()).map_err(|error| {
                    Error::corrupt(format!("invalid shallow boundary: {error}"))
                })?,
            );
        }
        Ok(boundaries)
    }

    /// Writes the shallow boundary file, removing it when empty.
    pub(crate) fn write_shallow(&self, boundaries: &[ObjectId]) -> Result<()> {
        let path = self.control_dir().join("shallow");
        if boundaries.is_empty() {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(Error::io(path.display().to_string(), error)),
            }
            return Ok(());
        }

        let mut boundaries = boundaries.to_vec();
        boundaries.sort_unstable();
        boundaries.dedup();
        let mut contents = String::new();
        for oid in boundaries {
            contents.push_str(&oid.to_hex());
            contents.push('\n');
        }
        cvx_core::fs::write_atomic(&path, contents.as_bytes(), Some(0o644))
    }

    fn validate_format(&self) -> Result<()> {
        let config = self.config()?;

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

        Ok(())
    }
}

fn is_bare_repository(path: &Path) -> bool {
    if !path.join("HEAD").is_file() || !path.join("objects").is_dir() || !path.join("refs").is_dir()
    {
        return false;
    }
    Config::load_file(path.join("config"))
        .ok()
        .and_then(|config| config.get_bool("core.bare").ok().flatten())
        .unwrap_or(false)
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
            object_format: cvx_object::HashAlgorithm::Sha1,
            bare: false,
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
    fn opens_sha256_repositories() {
        let temp = tempfile::tempdir().expect("tempdir");
        let control = temp.path().join(".cvx");
        std::fs::create_dir_all(control.join("objects")).expect("mkdir");
        std::fs::write(
            control.join("config"),
            "[core]\n repositoryformatversion = 1\n[extensions]\n objectformat = sha256\n",
        )
        .expect("write config");

        let repository = Repository::open(&control).expect("open sha256");
        assert_eq!(repository.hash_algorithm(), HashAlgorithm::Sha256);
    }

    #[test]
    fn opens_bare_repositories() {
        let temp = tempfile::tempdir().expect("tempdir");
        let control = temp.path().join("bare.git");
        std::fs::create_dir_all(control.join("objects")).expect("mkdir");
        std::fs::create_dir_all(control.join("refs")).expect("mkdir");
        std::fs::write(control.join("HEAD"), "ref: refs/heads/main\n").expect("HEAD");
        std::fs::write(
            control.join("config"),
            "[core]\n repositoryformatversion = 0\n bare = true\n",
        )
        .expect("write config");

        let repository = Repository::open(&control).expect("open bare");
        assert!(repository.is_bare());
        assert!(repository.require_work_tree().is_err());
    }
}
