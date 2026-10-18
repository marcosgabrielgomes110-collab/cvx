use std::path::{Path, PathBuf};

/// Result type used throughout the CVX engine.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Errors produced by the CVX engine.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No repository was found while walking up from the given path.
    #[error("not a CVX or Git repository (or any parent directory): {path}")]
    NotARepository {
        /// The path where discovery started.
        path: PathBuf,
    },

    /// The repository uses a feature that CVX does not implement yet.
    #[error("unsupported repository feature: {feature}")]
    Unsupported {
        /// Human-readable feature name.
        feature: String,
    },

    /// A reference name does not satisfy the Git reference rules.
    #[error("invalid reference name: {name}")]
    InvalidRefName {
        /// The rejected name.
        name: String,
    },

    /// A path is not safe or not valid inside a repository.
    #[error("invalid repository path: {path}")]
    InvalidPath {
        /// The rejected path.
        path: String,
    },

    /// Repository data is malformed or inconsistent.
    #[error("corrupt repository data: {message}")]
    Corrupt {
        /// Description of the inconsistency.
        message: String,
    },

    /// An object was requested but is not present in the object database.
    #[error("object not found: {oid}")]
    ObjectNotFound {
        /// Missing object identifier.
        oid: String,
    },

    /// The requested operation has no changes to record.
    #[error("{message}")]
    NothingToCommit {
        /// Human-readable explanation.
        message: String,
    },

    /// A configuration file could not be parsed or interpreted.
    #[error("configuration error in {path}: {message}")]
    Config {
        /// File that produced the error.
        path: PathBuf,
        /// Description of the problem.
        message: String,
    },

    /// An I/O operation failed with additional context.
    #[error("{context}: {source}")]
    Io {
        /// What CVX was trying to do.
        context: String,
        /// Underlying operating-system error.
        #[source]
        source: std::io::Error,
    },

    /// A general-purpose error with a ready-to-display message.
    #[error("{0}")]
    Message(String),
}

impl Error {
    /// Wraps an I/O error with context.
    pub fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }

    /// Creates an unsupported-feature error.
    pub fn unsupported(feature: impl Into<String>) -> Self {
        Self::Unsupported {
            feature: feature.into(),
        }
    }

    /// Creates an invalid-reference error.
    pub fn invalid_ref(name: impl Into<String>) -> Self {
        Self::InvalidRefName { name: name.into() }
    }

    /// Creates a corruption error.
    pub fn corrupt(message: impl Into<String>) -> Self {
        Self::Corrupt {
            message: message.into(),
        }
    }

    /// Creates a missing-object error.
    pub fn object_not_found(oid: impl Into<String>) -> Self {
        Self::ObjectNotFound { oid: oid.into() }
    }

    /// Creates a no-changes error.
    pub fn nothing_to_commit(message: impl Into<String>) -> Self {
        Self::NothingToCommit {
            message: message.into(),
        }
    }

    /// Returns the process exit code associated with this error.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::NothingToCommit { .. } => 1,
            _ => 128,
        }
    }

    /// Creates a configuration error.
    pub fn config(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self::Config {
            path: path.into(),
            message: message.into(),
        }
    }
}

/// Adds path and operation context to [`std::io::Result`] values.
pub trait IoResultExt<T> {
    /// Adds a free-form operation description.
    fn context(self, context: impl Into<String>) -> Result<T>;

    /// Adds the path being operated on as context.
    fn path_context(self, path: &Path) -> Result<T>;
}

impl<T> IoResultExt<T> for std::io::Result<T> {
    fn context(self, context: impl Into<String>) -> Result<T> {
        self.map_err(|source| Error::io(context, source))
    }

    fn path_context(self, path: &Path) -> Result<T> {
        self.map_err(|source| Error::io(path.display().to_string(), source))
    }
}
