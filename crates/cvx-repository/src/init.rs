use std::path::{Path, PathBuf};

use cvx_config::load_global;
use cvx_core::fs;
use cvx_core::{Error, IoResultExt, Result};

use crate::repository::{CVX_DIR, GIT_DIR, Repository};

const DEFAULT_BRANCH: &str = "main";

/// Storage convention to create.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InitStorage {
    /// Create a CVX-branded `.cvx/` control directory with Git-compatible
    /// contents.
    #[default]
    Cvx,
    /// Create a standard `.git/` directory for strict Git interoperability.
    Git,
}

impl InitStorage {
    /// Returns the control-directory name for this storage convention.
    pub const fn dir_name(self) -> &'static str {
        match self {
            Self::Cvx => CVX_DIR,
            Self::Git => GIT_DIR,
        }
    }
}

/// Options for repository initialization.
#[derive(Debug, Clone)]
pub struct InitOptions {
    /// Directory that will contain the worktree and control directory.
    pub directory: PathBuf,
    /// Initial branch name. Falls back to `init.defaultBranch`, then `main`.
    pub initial_branch: Option<String>,
    /// Control-directory convention.
    pub storage: InitStorage,
    /// Object format for new repositories.
    pub object_format: cvx_object::HashAlgorithm,
    /// Create a bare repository without a worktree.
    pub bare: bool,
    /// Suppress the human-readable summary.
    pub quiet: bool,
}

impl Default for InitOptions {
    fn default() -> Self {
        Self {
            directory: PathBuf::from("."),
            initial_branch: None,
            storage: InitStorage::Cvx,
            object_format: cvx_object::HashAlgorithm::Sha1,
            bare: false,
            quiet: false,
        }
    }
}

/// Result of `cvx init`.
#[derive(Debug, Clone)]
pub enum InitOutcome {
    /// A new repository was created.
    Created(Repository),
    /// An existing repository was reinitialized.
    Reinitialized(Repository),
    /// An existing `.git` repository was found and will be used in
    /// compatibility mode.
    ExistingGit(Repository),
}

impl InitOutcome {
    /// Returns the opened repository.
    pub fn repository(&self) -> &Repository {
        match self {
            Self::Created(repository)
            | Self::Reinitialized(repository)
            | Self::ExistingGit(repository) => repository,
        }
    }

    /// Returns whether the repository was newly created.
    pub fn was_created(&self) -> bool {
        matches!(self, Self::Created(_))
    }

    /// Returns whether an existing repository was reinitialized.
    pub fn was_reinitialized(&self) -> bool {
        matches!(self, Self::Reinitialized(_))
    }

    /// Returns whether an existing Git repository was detected.
    pub fn was_existing_git(&self) -> bool {
        matches!(self, Self::ExistingGit(_))
    }
}

impl Repository {
    /// Creates or reinitializes a repository.
    pub fn init(options: InitOptions) -> Result<InitOutcome> {
        let requested = if options.directory.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            options.directory.clone()
        };

        if requested.exists() && !requested.is_dir() {
            return Err(Error::Message(format!(
                "cannot initialize repository: {} is not a directory",
                requested.display()
            )));
        }
        fs::ensure_dir(&requested)?;
        let target = std::fs::canonicalize(&requested).path_context(&requested)?;

        if options.bare {
            return Self::init_bare(&target, &options);
        }

        let control_name = match options.storage {
            InitStorage::Cvx => CVX_DIR,
            InitStorage::Git => GIT_DIR,
        };
        let control_dir = target.join(control_name);

        if options.storage == InitStorage::Cvx {
            let existing_git = target.join(GIT_DIR);
            if !control_dir.exists() && existing_git.is_dir() {
                let repository = Repository::open(&existing_git)?;
                return Ok(InitOutcome::ExistingGit(repository));
            }
            if !control_dir.exists() && existing_git.exists() {
                return Err(Error::unsupported(
                    "linked Git worktrees and submodule checkouts are not supported yet",
                ));
            }
        } else if !control_dir.exists() && target.join(CVX_DIR).is_dir() {
            return Err(Error::Message(format!(
                "a CVX repository already exists at {}; run `cvx init` without --storage git to use it",
                target.join(CVX_DIR).display()
            )));
        }

        if control_dir.exists() {
            if !control_dir.is_dir() {
                return Err(Error::Message(format!(
                    "cannot initialize repository: {} is not a directory",
                    control_dir.display()
                )));
            }
            let branch = if control_dir.join("HEAD").exists() {
                None
            } else {
                Some(resolve_initial_branch(options.initial_branch.as_deref())?)
            };
            initialize_layout(
                &control_dir,
                options.storage,
                options.object_format,
                branch.as_deref(),
                false,
            )?;
            return Ok(InitOutcome::Reinitialized(Repository::open(&control_dir)?));
        }

        let branch = resolve_initial_branch(options.initial_branch.as_deref())?;
        let temporary = tempfile::Builder::new()
            .prefix(".cvx-init-")
            .tempdir_in(&target)
            .path_context(&target)?;
        initialize_layout(
            temporary.path(),
            options.storage,
            options.object_format,
            Some(&branch),
            false,
        )?;

        let temporary_path = temporary.keep();
        if let Err(error) = std::fs::rename(&temporary_path, &control_dir) {
            let _ = fs::remove_dir_all_if_exists(&temporary_path);
            return Err(Error::io(
                format!("failed to create {}", control_dir.display()),
                error,
            ));
        }

        Ok(InitOutcome::Created(Repository::open(&control_dir)?))
    }
}

impl Repository {
    fn init_bare(target: &Path, options: &InitOptions) -> Result<InitOutcome> {
        if target.join(CVX_DIR).is_dir() || target.join(GIT_DIR).is_dir() {
            return Err(Error::Message(format!(
                "cannot initialize a bare repository inside a worktree at {}",
                target.display()
            )));
        }

        let config_path = target.join("config");
        let existing = config_path.is_file();
        if existing {
            let config = cvx_config::Config::load_file(&config_path)?;
            if !config.get_bool("core.bare")?.unwrap_or(false) {
                return Err(Error::Message(format!(
                    "{} is a non-bare repository",
                    target.display()
                )));
            }
        }

        let branch = if target.join("HEAD").exists() {
            None
        } else {
            Some(resolve_initial_branch(options.initial_branch.as_deref())?)
        };
        initialize_layout(
            target,
            options.storage,
            options.object_format,
            branch.as_deref(),
            true,
        )?;
        let repository = Repository::open(target)?;
        Ok(if existing {
            InitOutcome::Reinitialized(repository)
        } else {
            InitOutcome::Created(repository)
        })
    }
}

fn resolve_initial_branch(explicit: Option<&str>) -> Result<String> {
    let branch = match explicit {
        Some(branch) => branch.to_owned(),
        None => load_global()?
            .get("init.defaultbranch")
            .filter(|branch| !branch.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| DEFAULT_BRANCH.to_owned()),
    };

    cvx_core::refname::validate_branch_name(&branch)?;
    Ok(branch)
}

fn initialize_layout(
    control_dir: &Path,
    storage: InitStorage,
    object_format: cvx_object::HashAlgorithm,
    branch: Option<&str>,
    bare: bool,
) -> Result<()> {
    for directory in [
        "branches",
        "hooks",
        "info",
        "objects/info",
        "objects/pack",
        "refs/heads",
        "refs/tags",
    ] {
        fs::ensure_dir(&control_dir.join(directory))?;
    }

    let filemode = probe_filemode(control_dir);
    let ignorecase = probe_ignorecase(control_dir);

    if let Some(branch) = branch {
        write_if_missing(
            &control_dir.join("HEAD"),
            format!("ref: refs/heads/{branch}\n").as_bytes(),
            0o644,
        )?;
    }
    write_if_missing(
        &control_dir.join("config"),
        config_contents(filemode, ignorecase, object_format, bare).as_bytes(),
        0o644,
    )?;
    write_if_missing(
        &control_dir.join("description"),
        b"Unnamed repository; edit this file 'description' to name the repository.\n",
        0o644,
    )?;
    write_if_missing(
        &control_dir.join("info/exclude"),
        exclude_contents(storage).as_bytes(),
        0o644,
    )?;

    if storage == InitStorage::Cvx {
        let metadata = format!(
            "# CVX repository metadata. Internal; do not edit.\n\
             [repository]\n\
             format-version = 1\n\
             object-format = \"{}\"\n\
             storage = \"git-compatible\"\n",
            object_format.name()
        );
        write_if_missing(&control_dir.join("cvx.toml"), metadata.as_bytes(), 0o644)?;
    }

    Ok(())
}

fn write_if_missing(path: &Path, contents: &[u8], mode: u32) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    fs::write_atomic(path, contents, Some(mode))
}

fn config_contents(
    filemode: bool,
    ignorecase: bool,
    object_format: cvx_object::HashAlgorithm,
    bare: bool,
) -> String {
    let mut config = String::from("[core]\n");
    config.push_str(&format!(
        "\trepositoryformatversion = {}\n",
        u8::from(object_format == cvx_object::HashAlgorithm::Sha256)
    ));
    config.push_str(&format!("\tfilemode = {filemode}\n"));
    config.push_str(&format!("\tbare = {bare}\n"));
    config.push_str("\tlogallrefupdates = true\n");
    if ignorecase {
        config.push_str("\tignorecase = true\n");
    }
    if object_format != cvx_object::HashAlgorithm::Sha1 {
        config.push_str("[extensions]\n");
        config.push_str(&format!("\tobjectformat = {}\n", object_format.name()));
    }
    config
}

fn exclude_contents(storage: InitStorage) -> String {
    let mut contents = String::from("# git ls-files --others --exclude-from=");
    contents.push_str(storage.dir_name());
    contents.push_str(
        "/info/exclude\n\
         # Lines that start with '#' are comments.\n\
         # For a project mostly in C, the following would be a good set of\n\
         # exclude patterns (uncomment them if you want to use them):\n\
         # *.[oa]\n\
         # *~\n",
    );
    if storage == InitStorage::Cvx {
        contents.push_str("/.cvx/\n");
    }
    contents
}

#[cfg(unix)]
fn probe_filemode(directory: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    let Ok(file) = tempfile::Builder::new()
        .prefix(".cvx-filemode-")
        .tempfile_in(directory)
    else {
        return false;
    };

    if file
        .as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o755))
        .is_err()
    {
        return false;
    }

    file.as_file()
        .metadata()
        .map(|metadata| metadata.permissions().mode() & 0o100 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn probe_filemode(_directory: &Path) -> bool {
    false
}

fn probe_ignorecase(directory: &Path) -> bool {
    let Ok(file) = tempfile::Builder::new()
        .prefix("CvxCaseProbe")
        .tempfile_in(directory)
    else {
        return false;
    };

    let Some(name) = file.path().file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let upper = name.to_ascii_uppercase();
    upper != name && file.path().with_file_name(upper).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_cvx_repository() {
        let temp = tempfile::tempdir().expect("tempdir");
        let outcome = Repository::init(InitOptions {
            directory: temp.path().to_path_buf(),
            initial_branch: Some("trunk".to_owned()),
            storage: InitStorage::Cvx,
            object_format: cvx_object::HashAlgorithm::Sha1,
            bare: false,
            quiet: true,
        })
        .expect("init");

        assert!(outcome.was_created());
        let repository = outcome.repository();
        assert_eq!(repository.kind(), crate::RepositoryKind::Cvx);
        assert!(repository.control_dir().join("objects/pack").is_dir());
        assert!(repository.control_dir().join("refs/heads").is_dir());
        assert_eq!(
            std::fs::read_to_string(repository.control_dir().join("HEAD")).unwrap(),
            "ref: refs/heads/trunk\n"
        );
        assert_eq!(
            repository.head().expect("head"),
            cvx_ref::Head::Symbolic {
                target: "refs/heads/trunk".to_owned(),
                oid: None
            }
        );
        assert!(repository.control_dir().join("cvx.toml").is_file());
    }

    #[test]
    fn reinitialization_does_not_overwrite_head() {
        let temp = tempfile::tempdir().expect("tempdir");
        let options = InitOptions {
            directory: temp.path().to_path_buf(),
            initial_branch: Some("main".to_owned()),
            storage: InitStorage::Cvx,
            object_format: cvx_object::HashAlgorithm::Sha1,
            bare: false,
            quiet: true,
        };
        Repository::init(options.clone()).expect("first init");

        let head = temp.path().join(".cvx/HEAD");
        std::fs::write(&head, "ref: refs/heads/custom\n").expect("custom HEAD");

        let outcome = Repository::init(options).expect("second init");
        assert!(outcome.was_reinitialized());
        assert_eq!(
            std::fs::read_to_string(head).unwrap(),
            "ref: refs/heads/custom\n"
        );
    }

    #[test]
    fn uses_existing_git_repository() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(temp.path().join(".git/objects")).expect("mkdir");
        std::fs::write(temp.path().join(".git/HEAD"), "ref: refs/heads/main\n").expect("HEAD");
        std::fs::write(
            temp.path().join(".git/config"),
            "[core]\n repositoryformatversion = 0\n",
        )
        .expect("config");

        let outcome = Repository::init(InitOptions {
            directory: temp.path().to_path_buf(),
            ..InitOptions::default()
        })
        .expect("init");

        assert!(outcome.was_existing_git());
        assert!(!temp.path().join(".cvx").exists());
    }

    #[test]
    fn rejects_invalid_initial_branch() {
        let temp = tempfile::tempdir().expect("tempdir");
        let result = Repository::init(InitOptions {
            directory: temp.path().to_path_buf(),
            initial_branch: Some("bad..name".to_owned()),
            ..InitOptions::default()
        });
        assert!(matches!(result, Err(Error::InvalidRefName { .. })));
    }

    #[test]
    fn creates_git_directory_when_requested() {
        let temp = tempfile::tempdir().expect("tempdir");
        let outcome = Repository::init(InitOptions {
            directory: temp.path().to_path_buf(),
            storage: InitStorage::Git,
            ..InitOptions::default()
        })
        .expect("init");

        assert!(outcome.was_created());
        assert_eq!(outcome.repository().kind(), crate::RepositoryKind::Git);
        assert!(temp.path().join(".git/HEAD").is_file());
        assert!(!temp.path().join(".cvx").exists());
    }
}
