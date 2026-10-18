use std::path::{Path, PathBuf};

use cvx_core::{Error, Result};

/// A single parsed configuration entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Section name, normalized to lowercase.
    pub section: String,
    /// Optional subsection, preserving its original case.
    pub subsection: Option<String>,
    /// Variable name, normalized to lowercase.
    pub name: String,
    /// Parsed value.
    pub value: String,
    /// File the entry came from.
    pub source: PathBuf,
    /// One-based line number where the entry starts.
    pub line: usize,
}

impl Entry {
    /// Returns the full dotted key, for example `remote.origin.url`.
    pub fn key(&self) -> String {
        match &self.subsection {
            Some(subsection) => format!("{}.{}.{}", self.section, subsection, self.name),
            None => format!("{}.{}", self.section, self.name),
        }
    }

    fn matches(&self, section: &str, subsection: Option<&str>, name: &str) -> bool {
        self.section.eq_ignore_ascii_case(section)
            && self.name.eq_ignore_ascii_case(name)
            && self.subsection.as_deref() == subsection
    }
}

/// Parsed Git configuration.
///
/// Entries retain their original order. Lookups return the last matching
/// value, which is how Git resolves repeated keys.
#[derive(Debug, Clone, Default)]
pub struct Config {
    entries: Vec<Entry>,
}

impl Config {
    /// Creates an empty configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses configuration text without following includes.
    pub fn parse_str(input: &str, source: impl Into<PathBuf>) -> Result<Self> {
        let source = source.into();
        let entries = parse_entries(input, &source)?;
        Ok(Self { entries })
    }

    /// Returns all parsed entries in order.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Returns the last value for a dotted key.
    pub fn get(&self, key: &str) -> Option<&str> {
        let (section, subsection, name) = split_key(key)?;
        self.entries
            .iter()
            .rev()
            .find(|entry| entry.matches(section, subsection, name))
            .map(|entry| entry.value.as_str())
    }

    /// Returns the last entry for a dotted key.
    pub fn get_entry(&self, key: &str) -> Option<&Entry> {
        let (section, subsection, name) = split_key(key)?;
        self.entries
            .iter()
            .rev()
            .find(|entry| entry.matches(section, subsection, name))
    }

    /// Reads a boolean value, if present.
    pub fn get_bool(&self, key: &str) -> Result<Option<bool>> {
        let Some(entry) = self.get_entry(key) else {
            return Ok(None);
        };
        parse_bool(&entry.value).map(Some).ok_or_else(|| {
            Error::config(
                &entry.source,
                format!(
                    "line {}: invalid boolean value for {}: {}",
                    entry.line,
                    entry.key(),
                    entry.value
                ),
            )
        })
    }

    pub(crate) fn push(&mut self, entry: Entry) {
        self.entries.push(entry);
    }
}

fn split_key(key: &str) -> Option<(&str, Option<&str>, &str)> {
    let key = key.trim();
    let first = key.find('.')?;
    let last = key.rfind('.')?;
    let section = &key[..first];
    let name = &key[last + 1..];
    if section.is_empty() || name.is_empty() {
        return None;
    }
    let subsection = if first == last {
        None
    } else {
        Some(&key[first + 1..last])
    };
    Some((section, subsection, name))
}

pub(crate) fn parse_entries(input: &str, source: &Path) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();
    let mut section: Option<(String, Option<String>)> = None;

    for (line_number, line) in logical_lines(input) {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }

        if trimmed.starts_with('[') {
            section = Some(parse_section(trimmed, source, line_number)?);
            continue;
        }

        let Some((section_name, subsection)) = section.as_ref() else {
            return Err(parse_error(
                source,
                line_number,
                "variable outside of a section",
            ));
        };

        let (name, value) = parse_variable(trimmed, source, line_number)?;
        entries.push(Entry {
            section: section_name.clone(),
            subsection: subsection.clone(),
            name,
            value,
            source: source.to_path_buf(),
            line: line_number,
        });
    }

    Ok(entries)
}

/// Joins backslash-newline continuations while preserving the first line
/// number of each logical line.
fn logical_lines(input: &str) -> Vec<(usize, String)> {
    let mut lines = Vec::new();
    let mut pending: Option<(usize, String)> = None;

    for (index, raw) in input.lines().enumerate() {
        let line_number = index + 1;
        let (start, mut line) = pending
            .take()
            .unwrap_or_else(|| (line_number, String::new()));
        line.push_str(raw);

        if has_odd_trailing_backslashes(&line) {
            line.pop();
            pending = Some((start, line));
        } else {
            lines.push((start, line));
        }
    }

    if let Some(pending) = pending {
        lines.push(pending);
    }

    lines
}

fn has_odd_trailing_backslashes(line: &str) -> bool {
    line.bytes().rev().take_while(|byte| *byte == b'\\').count() % 2 == 1
}

fn parse_section(
    line: &str,
    source: &Path,
    line_number: usize,
) -> Result<(String, Option<String>)> {
    let mut in_quotes = false;
    let mut escaped = false;
    let mut closing = None;

    for (index, character) in line.char_indices().skip(1) {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' if in_quotes => escaped = true,
            '"' => in_quotes = !in_quotes,
            ']' if !in_quotes => {
                closing = Some(index);
                break;
            }
            _ => {}
        }
    }

    let Some(closing) = closing else {
        return Err(parse_error(
            source,
            line_number,
            "unterminated section header",
        ));
    };

    let trailing = &line[closing + 1..];
    if !trailing.trim().is_empty()
        && !trailing.trim_start().starts_with('#')
        && !trailing.trim_start().starts_with(';')
    {
        return Err(parse_error(
            source,
            line_number,
            "unexpected text after section header",
        ));
    }

    let inner = line[1..closing].trim();
    let (section, subsection) = if let Some(quote) = inner.find('"') {
        let section = inner[..quote].trim();
        let (subsection, consumed) = parse_quoted(&inner[quote..], source, line_number)?;
        if !inner[quote + consumed..].trim().is_empty() {
            return Err(parse_error(
                source,
                line_number,
                "unexpected text after subsection",
            ));
        }
        (section, Some(subsection))
    } else {
        (inner, None)
    };

    if section.is_empty() || !section.chars().all(is_section_character) {
        return Err(parse_error(source, line_number, "invalid section name"));
    }

    Ok((section.to_ascii_lowercase(), subsection))
}

fn is_section_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '-' | '.' | '_')
}

fn parse_variable(line: &str, source: &Path, line_number: usize) -> Result<(String, String)> {
    let key_end = line
        .find(|character: char| character.is_whitespace() || character == '=')
        .unwrap_or(line.len());
    let key = &line[..key_end];

    if key.is_empty() || !key.chars().all(is_key_character) {
        return Err(parse_error(source, line_number, "invalid variable name"));
    }

    let rest = line[key_end..].trim_start();
    let value = if let Some(after_equals) = rest.strip_prefix('=') {
        parse_value(after_equals.trim_start(), source, line_number)?
    } else if rest.is_empty() || rest.starts_with('#') || rest.starts_with(';') {
        "true".to_owned()
    } else {
        return Err(parse_error(
            source,
            line_number,
            "expected '=' after variable name",
        ));
    };

    Ok((key.to_ascii_lowercase(), value))
}

fn is_key_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
}

fn parse_value(value: &str, source: &Path, line_number: usize) -> Result<String> {
    if value.starts_with('"') {
        let (parsed, consumed) = parse_quoted(value, source, line_number)?;
        let trailing = value[consumed..].trim_start();
        if !trailing.is_empty() && !trailing.starts_with('#') && !trailing.starts_with(';') {
            return Err(parse_error(
                source,
                line_number,
                "unexpected text after quoted value",
            ));
        }
        return Ok(parsed);
    }

    let end = value.find(['#', ';']).unwrap_or(value.len());
    Ok(value[..end].trim_end().to_owned())
}

/// Parses a quoted string starting at the first byte and returns the decoded
/// value plus the number of bytes consumed.
fn parse_quoted(input: &str, source: &Path, line_number: usize) -> Result<(String, usize)> {
    if !input.starts_with('"') {
        return Err(parse_error(source, line_number, "expected quoted string"));
    }

    let mut result = String::new();
    let mut characters = input.char_indices().skip(1);

    while let Some((index, character)) = characters.next() {
        match character {
            '"' => return Ok((result, index + 1)),
            '\\' => {
                let Some((_, escaped)) = characters.next() else {
                    return Err(parse_error(
                        source,
                        line_number,
                        "unterminated escape sequence",
                    ));
                };
                match escaped {
                    'n' => result.push('\n'),
                    't' => result.push('\t'),
                    'b' => result.push('\u{0008}'),
                    '\\' => result.push('\\'),
                    '"' => result.push('"'),
                    other => {
                        return Err(parse_error(
                            source,
                            line_number,
                            format!("invalid escape sequence \\{other}"),
                        ));
                    }
                }
            }
            other => result.push(other),
        }
    }

    Err(parse_error(
        source,
        line_number,
        "unterminated quoted string",
    ))
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" | "" => Some(false),
        _ => None,
    }
}

fn parse_error(source: &Path, line: usize, message: impl Into<String>) -> Error {
    Error::config(source, format!("line {line}: {}", message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> Config {
        Config::parse_str(input, "test-config").expect("parse")
    }

    #[test]
    fn parses_sections_and_values() {
        let config = parse(
            r#"
                [user]
                    name = Jane Doe
                    email = jane@example.com
                [init]
                    defaultBranch = trunk
                [remote "origin"]
                    url = git@github.com:jane/repo.git
            "#,
        );

        assert_eq!(config.get("user.name"), Some("Jane Doe"));
        assert_eq!(config.get("user.email"), Some("jane@example.com"));
        assert_eq!(config.get("init.defaultBranch"), Some("trunk"));
        assert_eq!(
            config.get("remote.origin.url"),
            Some("git@github.com:jane/repo.git")
        );
    }

    #[test]
    fn section_and_variable_names_are_case_insensitive() {
        let config = parse("[User]\n Name = Jane\n");
        assert_eq!(config.get("user.name"), Some("Jane"));
        assert_eq!(config.get("USER.NAME"), Some("Jane"));
    }

    #[test]
    fn subsection_is_case_sensitive() {
        let config = parse("[remote \"Origin\"]\n url = one\n[remote \"origin\"]\n url = two\n");
        assert_eq!(config.get("remote.Origin.url"), Some("one"));
        assert_eq!(config.get("remote.origin.url"), Some("two"));
    }

    #[test]
    fn last_value_wins() {
        let config = parse("[user]\n name = first\n name = second\n");
        assert_eq!(config.get("user.name"), Some("second"));
    }

    #[test]
    fn supports_comments_quotes_and_escapes() {
        let config = parse(
            "[core]\n\
             # comment\n\
             path = \"/tmp/a b\" ; trailing\n\
             escaped = \"line\\nnext\\tend\"\n\
             bare\n",
        );

        assert_eq!(config.get("core.path"), Some("/tmp/a b"));
        assert_eq!(config.get("core.escaped"), Some("line\nnext\tend"));
        assert_eq!(config.get("core.bare"), Some("true"));
    }

    #[test]
    fn supports_line_continuations() {
        let config = parse("[user]\n name = Jane\\\n Doe\n");
        assert_eq!(config.get("user.name"), Some("Jane Doe"));
    }

    #[test]
    fn reads_booleans() {
        let config = parse("[core]\n filemode = true\n bare = no\n");
        assert_eq!(config.get_bool("core.filemode").unwrap(), Some(true));
        assert_eq!(config.get_bool("core.bare").unwrap(), Some(false));
        assert_eq!(config.get_bool("core.missing").unwrap(), None);
    }

    #[test]
    fn rejects_invalid_syntax() {
        assert!(Config::parse_str("name = value\n", "test").is_err());
        assert!(Config::parse_str("[user\n name = value\n", "test").is_err());
        assert!(Config::parse_str("[user]\n name = \"unterminated\n", "test").is_err());
        assert!(Config::parse_str("[user]\n name = \"bad\\q\"\n", "test").is_err());
    }
}
