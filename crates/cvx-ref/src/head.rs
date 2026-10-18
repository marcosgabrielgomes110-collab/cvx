use cvx_object::ObjectId;

/// The state of `HEAD`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Head {
    /// `HEAD` points at a branch, which may still be unborn.
    Symbolic {
        /// Full reference name, such as `refs/heads/main`.
        target: String,
        /// Current commit, or `None` when the branch has no commits yet.
        oid: Option<ObjectId>,
    },

    /// `HEAD` points directly at a commit.
    Detached(ObjectId),
}

impl Head {
    /// Returns the commit `HEAD` resolves to, if any.
    pub fn oid(&self) -> Option<ObjectId> {
        match self {
            Self::Symbolic { oid, .. } => *oid,
            Self::Detached(oid) => Some(*oid),
        }
    }

    /// Returns the branch reference when `HEAD` is symbolic.
    pub fn target(&self) -> Option<&str> {
        match self {
            Self::Symbolic { target, .. } => Some(target),
            Self::Detached(_) => None,
        }
    }

    /// Returns whether this is a symbolic reference to an unborn branch.
    pub fn is_unborn(&self) -> bool {
        matches!(self, Self::Symbolic { oid: None, .. })
    }
}
