use cvx_core::{Error, Result};
use cvx_object::{ObjectId, ObjectKind, Tag};

use crate::commit::IdentityRole;
use crate::repository::Repository;

/// Options for `cvx tag`.
#[derive(Debug, Clone)]
pub struct TagOptions {
    /// Tag name without the `refs/tags/` prefix.
    pub name: String,
    /// Target revision. Defaults to `HEAD`.
    pub target: Option<String>,
    /// Annotated tag message.
    pub message: Option<String>,
    /// Create an annotated tag even without a message.
    pub annotated: bool,
    /// Replace an existing tag.
    pub force: bool,
}

/// Result of tag creation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagOutcome {
    /// Tag name.
    pub name: String,
    /// Object stored in `refs/tags/<name>`.
    pub oid: ObjectId,
    /// Commit the tag ultimately points at.
    pub target: ObjectId,
    /// Whether an annotated tag object was written.
    pub annotated: bool,
}

impl Repository {
    /// Creates a lightweight or annotated tag.
    pub fn create_tag(&self, options: &TagOptions) -> Result<TagOutcome> {
        let reference = format!("refs/tags/{}", options.name);
        cvx_core::refname::validate_ref_name(&reference)?;

        let existing = self.refs().read_ref(&reference)?;
        if existing.is_some() && !options.force {
            return Err(Error::Message(format!(
                "tag '{}' already exists",
                options.name
            )));
        }

        let target = self.resolve_revision(options.target.as_deref().unwrap_or("HEAD"))?;
        let annotated = options.annotated || options.message.is_some();

        let oid = if annotated {
            let message = options.message.as_deref().ok_or_else(|| {
                Error::Message(
                    "annotated tags require a message; use -m or create a lightweight tag"
                        .to_owned(),
                )
            })?;
            let message = normalize_message(message)?;
            let tagger = self.resolve_signature(IdentityRole::Committer)?;
            let kind = self.read_object(target)?.kind;
            let tag = Tag {
                object: target,
                kind,
                name: options.name.clone(),
                tagger: Some(tagger),
                message,
                extra_headers: Vec::new(),
            };
            self.objects().write(ObjectKind::Tag, &tag.serialize())?
        } else {
            target
        };

        if existing.is_some() {
            self.refs().update_ref(&reference, oid, existing)?;
        } else {
            self.refs().update_ref(&reference, oid, None)?;
        }

        Ok(TagOutcome {
            name: options.name.clone(),
            oid,
            target,
            annotated,
        })
    }

    /// Lists local tags in name order.
    pub fn tags(&self) -> Result<Vec<(String, ObjectId)>> {
        Ok(self
            .refs()
            .list_refs("refs/tags")?
            .into_iter()
            .filter_map(|(reference, oid)| {
                reference
                    .strip_prefix("refs/tags/")
                    .map(|name| (name.to_owned(), oid))
            })
            .collect())
    }
}

fn normalize_message(message: &str) -> Result<String> {
    let mut lines: Vec<&str> = message.lines().map(str::trim_end).collect();
    while lines.first().is_some_and(|line| line.is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    if lines.is_empty() {
        return Err(Error::Message("empty tag message".to_owned()));
    }
    let mut message = lines.join("\n");
    message.push('\n');
    Ok(message)
}
