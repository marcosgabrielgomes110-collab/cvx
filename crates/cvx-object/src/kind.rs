use std::fmt;

/// The four object types stored in a Git object database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectKind {
    /// File contents.
    Blob,
    /// Directory listing.
    Tree,
    /// Snapshot with parents and metadata.
    Commit,
    /// Annotated tag.
    Tag,
}

impl ObjectKind {
    /// Returns the canonical lowercase name used in object headers.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Blob => "blob",
            Self::Tree => "tree",
            Self::Commit => "commit",
            Self::Tag => "tag",
        }
    }

    /// Parses a canonical object type name.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "blob" => Some(Self::Blob),
            "tree" => Some(Self::Tree),
            "commit" => Some(Self::Commit),
            "tag" => Some(Self::Tag),
            _ => None,
        }
    }
}

impl fmt::Display for ObjectKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_names() {
        for kind in [
            ObjectKind::Blob,
            ObjectKind::Tree,
            ObjectKind::Commit,
            ObjectKind::Tag,
        ] {
            assert_eq!(ObjectKind::parse(kind.as_str()), Some(kind));
            assert_eq!(kind.to_string(), kind.as_str());
        }
    }

    #[test]
    fn rejects_unknown_names() {
        assert_eq!(ObjectKind::parse("delta"), None);
        assert_eq!(ObjectKind::parse("Blob"), None);
    }
}
