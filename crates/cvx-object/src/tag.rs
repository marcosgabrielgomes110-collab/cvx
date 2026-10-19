use cvx_core::{Error, Result};

use crate::{ObjectId, ObjectKind, Signature};

/// A parsed annotated-tag object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    /// Object the tag points at.
    pub object: ObjectId,
    /// Type of the tagged object.
    pub kind: ObjectKind,
    /// Tag name, as stored without the `refs/tags/` prefix.
    pub name: String,
    /// Identity that created the tag, when present.
    pub tagger: Option<Signature>,
    /// Tag message, including its trailing newline when present.
    pub message: String,
    /// Headers that CVX does not interpret yet.
    pub extra_headers: Vec<(String, String)>,
}

impl Tag {
    /// Parses annotated-tag object content.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| Error::corrupt("tag object is not valid UTF-8"))?;
        let (headers, message) = match text.split_once("\n\n") {
            Some((headers, message)) => (headers, message.to_owned()),
            None => (text, String::new()),
        };

        let mut object = None;
        let mut kind = None;
        let mut name = None;
        let mut tagger = None;
        let mut extra_headers: Vec<(String, String)> = Vec::new();
        let mut current_extra: Option<usize> = None;

        for line in headers.split('\n') {
            if let Some(continuation) = line.strip_prefix(' ') {
                let Some(index) = current_extra else {
                    return Err(Error::corrupt("tag header continuation without a header"));
                };
                extra_headers[index].1.push('\n');
                extra_headers[index].1.push_str(continuation);
                continue;
            }

            current_extra = None;
            let Some((key, value)) = line.split_once(' ') else {
                return Err(Error::corrupt(format!(
                    "tag header is missing a value: {line}"
                )));
            };

            match key {
                "object" => {
                    object = Some(ObjectId::from_hex(value).map_err(|error| {
                        Error::corrupt(format!("invalid tag object id: {error}"))
                    })?);
                }
                "type" => {
                    kind = Some(
                        ObjectKind::parse(value)
                            .ok_or_else(|| Error::corrupt(format!("invalid tag type: {value}")))?,
                    );
                }
                "tag" => name = Some(value.to_owned()),
                "tagger" => tagger = Some(Signature::parse(value)?),
                _ => {
                    extra_headers.push((key.to_owned(), value.to_owned()));
                    current_extra = Some(extra_headers.len() - 1);
                }
            }
        }

        Ok(Self {
            object: object.ok_or_else(|| Error::corrupt("tag is missing an object"))?,
            kind: kind.ok_or_else(|| Error::corrupt("tag is missing a type"))?,
            name: name.ok_or_else(|| Error::corrupt("tag is missing a name"))?,
            tagger,
            message,
            extra_headers,
        })
    }

    /// Serializes the tag object.
    pub fn serialize(&self) -> Vec<u8> {
        let mut output = String::new();
        output.push_str(&format!("object {}\n", self.object));
        output.push_str(&format!("type {}\n", self.kind));
        output.push_str(&format!("tag {}\n", self.name));
        if let Some(tagger) = &self.tagger {
            output.push_str(&format!("tagger {}\n", tagger.to_git_string()));
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    const TAG: &str = "object 1111111111111111111111111111111111111111\n\
                       type commit\n\
                       tag v1.0.0\n\
                       tagger Jane Doe <jane@example.com> 1700000000 +0000\n\
                       \n\
                       Release 1.0\n";

    #[test]
    fn parses_and_roundtrips_tags() {
        let tag = Tag::parse(TAG.as_bytes()).expect("parse");
        assert_eq!(
            tag.object.to_hex(),
            "1111111111111111111111111111111111111111"
        );
        assert_eq!(tag.kind, ObjectKind::Commit);
        assert_eq!(tag.name, "v1.0.0");
        assert_eq!(tag.message, "Release 1.0\n");
        assert_eq!(tag.tagger.as_ref().expect("tagger").name, "Jane Doe");

        let parsed = Tag::parse(&tag.serialize()).expect("roundtrip");
        assert_eq!(parsed, tag);
    }

    #[test]
    fn accepts_missing_tagger() {
        let input = "object 1111111111111111111111111111111111111111\n\
                     type blob\n\
                     tag blob-tag\n\
                     \n\
                     message\n";
        let tag = Tag::parse(input.as_bytes()).expect("parse");
        assert!(tag.tagger.is_none());
        assert_eq!(tag.kind, ObjectKind::Blob);
    }

    #[test]
    fn rejects_malformed_tags() {
        assert!(Tag::parse(b"type commit\ntag v1\n\nmessage\n").is_err());
        assert!(
            Tag::parse(b"object 1111111111111111111111111111111111111111\ntag v1\n\n").is_err()
        );
        assert!(
            Tag::parse(b"object 1111111111111111111111111111111111111111\ntype commit\n\n")
                .is_err()
        );
    }
}
