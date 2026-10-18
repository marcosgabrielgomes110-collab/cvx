use cvx_core::{Error, Result};

use crate::{ObjectId, Signature};

/// A parsed commit object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    /// Root tree.
    pub tree: ObjectId,
    /// Parent commits in order.
    pub parents: Vec<ObjectId>,
    /// Author identity.
    pub author: Signature,
    /// Committer identity.
    pub committer: Signature,
    /// Commit message, including its trailing newline when present.
    pub message: String,
    /// Headers that CVX does not interpret yet, such as `gpgsig`.
    pub extra_headers: Vec<(String, String)>,
}

impl Commit {
    /// Parses commit object content.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| Error::corrupt("commit object is not valid UTF-8"))?;

        let (headers, message) = match text.split_once("\n\n") {
            Some((headers, message)) => (headers, message.to_owned()),
            None => (text, String::new()),
        };

        let mut tree = None;
        let mut parents = Vec::new();
        let mut author = None;
        let mut committer = None;
        let mut extra_headers: Vec<(String, String)> = Vec::new();
        let mut current_extra: Option<usize> = None;

        for line in headers.split('\n') {
            if let Some(continuation) = line.strip_prefix(' ') {
                let Some(index) = current_extra else {
                    return Err(Error::corrupt(
                        "commit header continuation without a header",
                    ));
                };
                extra_headers[index].1.push('\n');
                extra_headers[index].1.push_str(continuation);
                continue;
            }

            current_extra = None;
            let Some((key, value)) = line.split_once(' ') else {
                return Err(Error::corrupt(format!(
                    "commit header is missing a value: {line}"
                )));
            };

            match key {
                "tree" => {
                    tree = Some(ObjectId::from_hex(value).map_err(|error| {
                        Error::corrupt(format!("invalid commit tree id: {error}"))
                    })?);
                }
                "parent" => {
                    parents.push(
                        ObjectId::from_hex(value).map_err(|error| {
                            Error::corrupt(format!("invalid parent id: {error}"))
                        })?,
                    );
                }
                "author" => author = Some(Signature::parse(value)?),
                "committer" => committer = Some(Signature::parse(value)?),
                _ => {
                    extra_headers.push((key.to_owned(), value.to_owned()));
                    current_extra = Some(extra_headers.len() - 1);
                }
            }
        }

        Ok(Self {
            tree: tree.ok_or_else(|| Error::corrupt("commit is missing a tree"))?,
            parents,
            author: author.ok_or_else(|| Error::corrupt("commit is missing an author"))?,
            committer: committer.ok_or_else(|| Error::corrupt("commit is missing a committer"))?,
            message,
            extra_headers,
        })
    }

    /// Serializes the commit object.
    pub fn serialize(&self) -> Vec<u8> {
        let mut output = String::new();
        output.push_str(&format!("tree {}\n", self.tree));
        for parent in &self.parents {
            output.push_str(&format!("parent {parent}\n"));
        }
        output.push_str(&format!("author {}\n", self.author.to_git_string()));
        output.push_str(&format!("committer {}\n", self.committer.to_git_string()));
        for (key, value) in &self.extra_headers {
            output.push_str(key);
            output.push(' ');
            output.push_str(&value.replace('\n', "\n "));
            output.push('\n');
        }
        output.push('\n');
        output.push_str(&self.message);
        output.into_bytes()
    }

    /// Returns the first line of the commit message.
    pub fn subject(&self) -> &str {
        self.message.lines().next().unwrap_or("").trim_end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMIT: &str = "tree 4b825dc642cb6eb9a060e54bf8d69288fbee4904\n\
                          parent 1111111111111111111111111111111111111111\n\
                          author Jane Doe <jane@example.com> 1700000000 +0000\n\
                          committer John Doe <john@example.com> 1700000001 +0200\n\
                          \n\
                          Initial commit\n";

    #[test]
    fn parses_commit() {
        let commit = Commit::parse(COMMIT.as_bytes()).expect("parse");
        assert_eq!(
            commit.tree.to_hex(),
            "4b825dc642cb6eb9a060e54bf8d69288fbee4904"
        );
        assert_eq!(commit.parents.len(), 1);
        assert_eq!(commit.author.name, "Jane Doe");
        assert_eq!(commit.committer.offset_minutes, 120);
        assert_eq!(commit.subject(), "Initial commit");
    }

    #[test]
    fn roundtrips_commit() {
        let commit = Commit::parse(COMMIT.as_bytes()).expect("parse");
        let bytes = commit.serialize();
        let parsed = Commit::parse(&bytes).expect("parse serialized");
        assert_eq!(parsed, commit);
    }

    #[test]
    fn preserves_multiline_headers() {
        let input = "tree 4b825dc642cb6eb9a060e54bf8d69288fbee4904\n\
                     author A <a@b> 1 +0000\n\
                     committer A <a@b> 1 +0000\n\
                     gpgsig -----BEGIN SIGNATURE-----\n \
                     abc\n \
                     -----END SIGNATURE-----\n\
                     \n\
                     signed\n";
        let commit = Commit::parse(input.as_bytes()).expect("parse");
        assert_eq!(commit.extra_headers.len(), 1);
        assert!(commit.extra_headers[0].1.contains("abc"));
        assert_eq!(commit.subject(), "signed");
    }

    #[test]
    fn rejects_missing_required_headers() {
        assert!(Commit::parse(b"author A <a@b> 1 +0000\n\nmessage\n").is_err());
        assert!(
            Commit::parse(b"tree 4b825dc642cb6eb9a060e54bf8d69288fbee4904\n\nmessage\n").is_err()
        );
    }
}
