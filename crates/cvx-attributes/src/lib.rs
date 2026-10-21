//! Parsing and matching of Git attributes files.
//!
//! CVX implements the common `.gitattributes` attributes used for text
//! normalization: `text`, `text=auto`, `-text`, `eol=lf`, `eol=crlf` and
//! `binary`. Working-tree encodings and diff/merge drivers are parsed but not
//! applied yet.

#![forbid(unsafe_code)]

use std::path::Path;

use cvx_core::{Error, Result};
use globset::{GlobBuilder, GlobMatcher};

/// Line-ending convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eol {
    /// Unix line endings.
    Lf,
    /// Windows line endings.
    Crlf,
}

/// Effective attributes for one path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attributes {
    /// `text`, `-text`, or `text=auto` when `auto` is set.
    pub text: Option<bool>,
    /// Whether `text=auto` was requested.
    pub auto: bool,
    /// Explicit `eol` value.
    pub eol: Option<Eol>,
    /// Whether `binary` is set.
    pub binary: bool,
    /// Requested working-tree encoding.
    pub encoding: Option<String>,
}

impl Attributes {
    /// Returns whether content should be normalized before entering the index.
    pub fn normalize_for_index(&self, content: &[u8]) -> bool {
        if self.binary || self.text == Some(false) {
            return false;
        }
        if self.text == Some(true) || self.eol.is_some() {
            return true;
        }
        self.auto && !looks_binary(content)
    }

    /// Returns the line ending to use when writing a file to the worktree.
    pub fn worktree_eol(&self) -> Option<Eol> {
        if self.binary || self.text == Some(false) {
            return None;
        }
        self.eol
    }

    /// Converts CRLF input to the LF form stored in blobs.
    pub fn to_index_content(&self, content: &[u8]) -> Vec<u8> {
        if !self.normalize_for_index(content) {
            return content.to_vec();
        }
        normalize_crlf(content)
    }

    /// Converts LF blob content to the requested worktree line endings.
    pub fn to_worktree_content(&self, content: &[u8]) -> Vec<u8> {
        match self.worktree_eol() {
            Some(Eol::Crlf) => to_crlf(content),
            Some(Eol::Lf) | None => content.to_vec(),
        }
    }
}

/// One parsed attribute rule.
#[derive(Debug, Clone)]
struct Rule {
    matcher: GlobMatcher,
    basename_only: bool,
    base: String,
    attributes: Vec<(String, AttrValue)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AttrValue {
    Set,
    Unset,
    Value(String),
}

/// Parsed `.gitattributes` file.
#[derive(Debug, Clone, Default)]
pub struct AttributeSet {
    rules: Vec<Rule>,
}

impl AttributeSet {
    /// Parses attribute text.
    pub fn parse(text: &str) -> Result<Self> {
        Self::parse_with_base(text, "")
    }

    fn parse_with_base(text: &str, base: &str) -> Result<Self> {
        let mut rules = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut fields = line.split_whitespace();
            let Some(pattern) = fields.next() else {
                continue;
            };
            if pattern.starts_with('!') {
                // Negative patterns are forbidden by Git's attributes format.
                continue;
            }
            let basename_only = !pattern.contains('/');
            let pattern = pattern.strip_prefix('/').unwrap_or(pattern);
            let glob = GlobBuilder::new(pattern)
                .literal_separator(true)
                .backslash_escape(false)
                .build()
                .map_err(|error| {
                    Error::Message(format!(
                        "invalid .gitattributes pattern on line {}: {error}",
                        index + 1
                    ))
                })?;
            let mut attributes = Vec::new();
            for field in fields {
                if let Some((name, value)) = field.split_once('=') {
                    attributes.push((name.to_owned(), AttrValue::Value(value.to_owned())));
                } else if let Some(name) = field.strip_prefix('-') {
                    attributes.push((name.to_owned(), AttrValue::Unset));
                } else {
                    attributes.push((field.to_owned(), AttrValue::Set));
                }
            }
            rules.push(Rule {
                matcher: glob.compile_matcher(),
                basename_only,
                base: base.to_owned(),
                attributes,
            });
        }
        Ok(Self { rules })
    }

    /// Loads `.gitattributes` files from a worktree.
    ///
    /// Root and nested files are applied in path order so deeper files override
    /// shallower ones, matching Git's lookup order.
    pub fn load(work_tree: &Path) -> Result<Self> {
        let mut files = Vec::new();
        collect_attribute_files(work_tree, work_tree, &mut files)?;
        files.sort_by(|left, right| {
            left.components()
                .count()
                .cmp(&right.components().count())
                .then_with(|| left.cmp(right))
        });

        let mut rules = Vec::new();
        for file in files {
            let contents = std::fs::read_to_string(work_tree.join(&file))
                .map_err(|error| Error::io(work_tree.join(&file).display().to_string(), error))?;
            let base = file
                .parent()
                .map(|path| path.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            rules.extend(Self::parse_with_base(&contents, &base)?.rules);
        }
        Ok(Self { rules })
    }

    /// Loads a specific attributes file.
    pub fn load_file(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .map_err(|error| Error::io(path.display().to_string(), error))?;
        Self::parse(&contents)
    }

    /// Returns whether there are no rules.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Computes the effective attributes for a repository-relative path.
    pub fn for_path(&self, path: &[u8]) -> Attributes {
        let path_text = String::from_utf8_lossy(path);
        let mut attributes = Attributes::default();

        for rule in &self.rules {
            let relative = if rule.base.is_empty() {
                path_text.as_ref()
            } else {
                let prefix = format!("{}/", rule.base);
                let Some(relative) = path_text.strip_prefix(&prefix) else {
                    continue;
                };
                relative
            };
            let basename = relative.rsplit('/').next().unwrap_or(relative);
            let matched = if rule.basename_only {
                rule.matcher.is_match(basename)
            } else {
                rule.matcher.is_match(relative)
            };
            if !matched {
                continue;
            }
            for (name, value) in &rule.attributes {
                apply_attribute(&mut attributes, name, value);
            }
        }
        attributes
    }
}

fn collect_attribute_files(
    work_tree: &Path,
    directory: &Path,
    files: &mut Vec<std::path::PathBuf>,
) -> Result<()> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(Error::io(directory.display().to_string(), error)),
    };
    for entry in entries {
        let entry = entry.map_err(|error| Error::io(directory.display().to_string(), error))?;
        let path = entry.path();
        let metadata = entry
            .metadata()
            .map_err(|error| Error::io(path.display().to_string(), error))?;
        if metadata.is_dir() {
            if entry.file_name() == ".git" || entry.file_name() == ".cvx" {
                continue;
            }
            collect_attribute_files(work_tree, &path, files)?;
        } else if entry.file_name() == ".gitattributes" {
            let relative = path
                .strip_prefix(work_tree)
                .map_err(|_| Error::corrupt("attribute path escaped the worktree"))?;
            files.push(relative.to_path_buf());
        }
    }
    Ok(())
}

fn apply_attribute(attributes: &mut Attributes, name: &str, value: &AttrValue) {
    match name {
        "text" => match value {
            AttrValue::Set => {
                attributes.text = Some(true);
                attributes.auto = false;
            }
            AttrValue::Unset => {
                attributes.text = Some(false);
                attributes.auto = false;
            }
            AttrValue::Value(value) if value.eq_ignore_ascii_case("auto") => {
                attributes.text = None;
                attributes.auto = true;
            }
            AttrValue::Value(_) => {}
        },
        "eol" => {
            let eol = match value {
                AttrValue::Value(value) if value.eq_ignore_ascii_case("lf") => Some(Eol::Lf),
                AttrValue::Value(value) if value.eq_ignore_ascii_case("crlf") => Some(Eol::Crlf),
                _ => None,
            };
            if let Some(eol) = eol {
                attributes.eol = Some(eol);
                if attributes.text.is_none() {
                    attributes.text = Some(true);
                    attributes.auto = false;
                }
            }
        }
        "binary" => match value {
            AttrValue::Set => {
                attributes.binary = true;
                attributes.text = Some(false);
            }
            AttrValue::Unset => attributes.binary = false,
            AttrValue::Value(_) => {}
        },
        "working-tree-encoding" => {
            if let AttrValue::Value(value) = value {
                attributes.encoding = Some(value.clone());
            }
        }
        _ => {}
    }
}

fn looks_binary(content: &[u8]) -> bool {
    content.iter().take(8000).any(|byte| *byte == 0)
}

fn normalize_crlf(content: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(content.len());
    let mut index = 0;
    while index < content.len() {
        if content[index] == b'\r' && content.get(index + 1) == Some(&b'\n') {
            index += 1;
        }
        output.push(content[index]);
        index += 1;
    }
    output
}

fn to_crlf(content: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(content.len() + content.len() / 16);
    for (index, byte) in content.iter().enumerate() {
        if *byte == b'\n' && (index == 0 || content[index - 1] != b'\r') {
            output.push(b'\r');
        }
        output.push(*byte);
    }
    output
}

/// Parses `core.autocrlf`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoCrlf {
    /// Disabled.
    False,
    /// Convert in both directions.
    True,
    /// Convert only when adding to the index.
    Input,
}

impl AutoCrlf {
    /// Parses a Git configuration value.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "true" | "yes" | "on" | "1" => Some(Self::True),
            "input" => Some(Self::Input),
            "false" | "no" | "off" | "0" | "" => Some(Self::False),
            _ => None,
        }
    }
}

/// Applies automatic CRLF rules on top of explicit attributes.
pub fn effective_attributes(attributes: &Attributes, autocrlf: AutoCrlf) -> Attributes {
    if attributes.text == Some(false) || attributes.binary {
        return attributes.clone();
    }
    if attributes.text.is_some() || attributes.auto || attributes.eol.is_some() {
        return attributes.clone();
    }
    match autocrlf {
        AutoCrlf::False => attributes.clone(),
        AutoCrlf::True => Attributes {
            text: Some(true),
            auto: false,
            eol: Some(Eol::Crlf),
            ..attributes.clone()
        },
        AutoCrlf::Input => Attributes {
            text: Some(true),
            auto: false,
            eol: Some(Eol::Lf),
            ..attributes.clone()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_basename_and_path_patterns() {
        let set =
            AttributeSet::parse("*.txt text eol=lf\nsrc/*.bin binary\ndocs/**/*.md text=auto\n")
                .expect("parse");

        let txt = set.for_path(b"src/notes.txt");
        assert_eq!(txt.text, Some(true));
        assert_eq!(txt.eol, Some(Eol::Lf));

        let binary = set.for_path(b"src/image.bin");
        assert!(binary.binary);
        assert_eq!(binary.text, Some(false));

        let docs = set.for_path(b"docs/guide/intro.md");
        assert!(docs.auto);
    }

    #[test]
    fn later_rules_override_earlier_rules() {
        let set = AttributeSet::parse("*.txt text\n*.txt -text\n").expect("parse");
        let attributes = set.for_path(b"file.txt");
        assert_eq!(attributes.text, Some(false));
        assert!(!attributes.normalize_for_index(b"a\r\nb\r\n"));
    }

    #[test]
    fn normalizes_crlf_for_index_and_worktree() {
        let set = AttributeSet::parse("*.txt text eol=crlf\n").expect("parse");
        let attributes = set.for_path(b"file.txt");
        assert_eq!(attributes.to_index_content(b"a\r\nb\r\n"), b"a\nb\n");
        assert_eq!(attributes.to_worktree_content(b"a\nb\n"), b"a\r\nb\r\n");
    }

    #[test]
    fn binary_auto_detection() {
        let set = AttributeSet::parse("*.dat text=auto\n").expect("parse");
        let attributes = set.for_path(b"file.dat");
        assert!(attributes.normalize_for_index(b"text\r\n"));
        assert!(!attributes.normalize_for_index(b"text\0binary"));
    }

    #[test]
    fn nested_files_override_root_rules() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp.path().join(".gitattributes"), "*.txt text\n").expect("root");
        std::fs::create_dir(temp.path().join("sub")).expect("mkdir");
        std::fs::write(temp.path().join("sub/.gitattributes"), "*.txt -text\n").expect("nested");

        let set = AttributeSet::load(temp.path()).expect("load");
        assert_eq!(set.for_path(b"root.txt").text, Some(true));
        assert_eq!(set.for_path(b"sub/file.txt").text, Some(false));
    }

    #[test]
    fn parses_autocrlf() {
        assert_eq!(AutoCrlf::parse("true"), Some(AutoCrlf::True));
        assert_eq!(AutoCrlf::parse("input"), Some(AutoCrlf::Input));
        assert_eq!(AutoCrlf::parse("false"), Some(AutoCrlf::False));
        assert_eq!(AutoCrlf::parse("maybe"), None);

        let default = Attributes::default();
        assert_eq!(
            effective_attributes(&default, AutoCrlf::True).eol,
            Some(Eol::Crlf)
        );
        assert_eq!(
            effective_attributes(&default, AutoCrlf::Input).eol,
            Some(Eol::Lf)
        );
        assert_eq!(effective_attributes(&default, AutoCrlf::False).eol, None);
    }
}
