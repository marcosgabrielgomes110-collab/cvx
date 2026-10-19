//! Diff computation between trees, commits and the worktree.
//!
//! The first implementation renders unified diffs and `--stat` summaries for
//! text files, detects binary content and reports file-mode changes. Rename
//! detection and three-way merge are separate layers.

#![forbid(unsafe_code)]

use std::fmt::Write;

use cvx_object::{MODE_FILE, ObjectId};
use similar::{ChangeTag, TextDiff};

/// One side of a changed file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffSide {
    /// File mode.
    pub mode: u32,
    /// Blob identifier.
    pub oid: ObjectId,
    /// Blob content.
    pub content: Vec<u8>,
}

/// A file that differs between two snapshots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffFile {
    /// Path shown in the diff.
    pub path: Vec<u8>,
    /// Old side, or `None` for added files.
    pub old: Option<DiffSide>,
    /// New side, or `None` for deleted files.
    pub new: Option<DiffSide>,
}

/// Rendering options.
#[derive(Debug, Clone)]
pub struct DiffOptions {
    /// Number of context lines.
    pub context: usize,
    /// Render a stat summary instead of hunks.
    pub stat: bool,
}

impl Default for DiffOptions {
    fn default() -> Self {
        Self {
            context: 3,
            stat: false,
        }
    }
}

/// Renders a list of changed files.
pub fn render(files: &[DiffFile], options: &DiffOptions) -> String {
    if options.stat {
        return render_stat(files);
    }

    let mut output = String::new();
    for file in files {
        render_file(&mut output, file, options.context);
    }
    output
}

/// Returns whether content should be treated as binary.
pub fn is_binary(content: &[u8]) -> bool {
    content.iter().take(8000).any(|byte| *byte == 0)
}

fn render_file(output: &mut String, file: &DiffFile, context: usize) {
    let path = display_path(&file.path);
    let old_path = if file.old.is_some() {
        format!("a/{path}")
    } else {
        "/dev/null".to_owned()
    };
    let new_path = if file.new.is_some() {
        format!("b/{path}")
    } else {
        "/dev/null".to_owned()
    };

    let _ = writeln!(output, "diff --git a/{path} b/{path}");

    match (&file.old, &file.new) {
        (None, Some(new)) => {
            let _ = writeln!(output, "new file mode {:06o}", new.mode);
        }
        (Some(old), None) => {
            let _ = writeln!(output, "deleted file mode {:06o}", old.mode);
        }
        (Some(old), Some(new)) if old.mode != new.mode => {
            let _ = writeln!(output, "old mode {:06o}", old.mode);
            let _ = writeln!(output, "new mode {:06o}", new.mode);
        }
        _ => {}
    }

    let old_oid = file.old.as_ref().map_or(ObjectId::ZERO, |side| side.oid);
    let new_oid = file.new.as_ref().map_or(ObjectId::ZERO, |side| side.oid);
    if file.old.is_none() || file.new.is_none() {
        let _ = writeln!(output, "index {}..{}", old_oid.short(7), new_oid.short(7));
    } else if old_oid != new_oid {
        let mode = file
            .new
            .as_ref()
            .or(file.old.as_ref())
            .map_or(MODE_FILE, |side| side.mode);
        let _ = writeln!(
            output,
            "index {}..{} {:06o}",
            old_oid.short(7),
            new_oid.short(7),
            mode
        );
    }

    let old_content = file.old.as_ref().map_or(&[][..], |side| &side.content);
    let new_content = file.new.as_ref().map_or(&[][..], |side| &side.content);

    if is_binary(old_content) || is_binary(new_content) {
        let _ = writeln!(output, "Binary files {old_path} and {new_path} differ");
        return;
    }

    let old_text = String::from_utf8_lossy(old_content);
    let new_text = String::from_utf8_lossy(new_content);
    if old_text == new_text {
        return;
    }

    let diff = TextDiff::from_lines(old_text.as_ref(), new_text.as_ref());
    let mut unified = diff.unified_diff();
    let unified = unified.context_radius(context).header(&old_path, &new_path);
    let _ = write!(output, "{unified}");
}

fn render_stat(files: &[DiffFile]) -> String {
    let mut output = String::new();
    let mut files_changed = 0usize;
    let mut insertions = 0usize;
    let mut deletions = 0usize;

    for file in files {
        let path = display_path(&file.path);
        let old_content = file.old.as_ref().map_or(&[][..], |side| &side.content);
        let new_content = file.new.as_ref().map_or(&[][..], |side| &side.content);

        if is_binary(old_content) || is_binary(new_content) {
            let _ = writeln!(output, " {path} | Bin");
        } else {
            let old_text = String::from_utf8_lossy(old_content);
            let new_text = String::from_utf8_lossy(new_content);
            let diff = TextDiff::from_lines(old_text.as_ref(), new_text.as_ref());
            let mut added = 0usize;
            let mut removed = 0usize;
            for change in diff.iter_all_changes() {
                match change.tag() {
                    ChangeTag::Insert => added += 1,
                    ChangeTag::Delete => removed += 1,
                    ChangeTag::Equal => {}
                }
            }
            let total = added + removed;
            let bar = format!("{}{}", "+".repeat(added), "-".repeat(removed));
            let _ = writeln!(output, " {path} | {total} {bar}");
            insertions += added;
            deletions += removed;
        }
        files_changed += 1;
    }

    let _ = writeln!(
        output,
        " {files_changed} file{} changed, {insertions} insertion{}(+), {deletions} deletion{}(-)",
        plural(files_changed),
        plural(insertions),
        plural(deletions)
    );
    output
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

fn display_path(path: &[u8]) -> String {
    let text = String::from_utf8_lossy(path);
    if text
        .chars()
        .any(|character| character.is_whitespace() || matches!(character, '"' | '\\'))
    {
        let mut escaped = String::new();
        for character in text.chars() {
            match character {
                '"' => escaped.push_str("\\\""),
                '\\' => escaped.push_str("\\\\"),
                '\n' => escaped.push_str("\\n"),
                '\t' => escaped.push_str("\\t"),
                character => escaped.push(character),
            }
        }
        format!("\"{escaped}\"")
    } else {
        text.into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oid(byte: u8) -> ObjectId {
        ObjectId::from_bytes([byte; 20])
    }

    fn side(content: &[u8]) -> DiffSide {
        DiffSide {
            mode: MODE_FILE,
            oid: oid(1),
            content: content.to_vec(),
        }
    }

    #[test]
    fn renders_modified_file() {
        let file = DiffFile {
            path: b"hello.txt".to_vec(),
            old: Some(side(b"hello\n")),
            new: Some(side(b"hello world\n")),
        };
        let output = render(&[file], &DiffOptions::default());

        assert!(output.contains("diff --git a/hello.txt b/hello.txt"));
        assert!(output.contains("--- a/hello.txt"));
        assert!(output.contains("+++ b/hello.txt"));
        assert!(output.contains("-hello"));
        assert!(output.contains("+hello world"));
    }

    #[test]
    fn renders_added_and_deleted_files() {
        let added = DiffFile {
            path: b"new.txt".to_vec(),
            old: None,
            new: Some(side(b"new\n")),
        };
        let deleted = DiffFile {
            path: b"old.txt".to_vec(),
            old: Some(side(b"old\n")),
            new: None,
        };

        let output = render(&[added, deleted], &DiffOptions::default());
        assert!(output.contains("new file mode 100644"));
        assert!(output.contains("--- /dev/null"));
        assert!(output.contains("deleted file mode 100644"));
        assert!(output.contains("+++ /dev/null"));
    }

    #[test]
    fn reports_mode_changes_without_content() {
        let file = DiffFile {
            path: b"script.sh".to_vec(),
            old: Some(DiffSide {
                mode: MODE_FILE,
                oid: oid(2),
                content: b"#!/bin/sh\n".to_vec(),
            }),
            new: Some(DiffSide {
                mode: 0o100_755,
                oid: oid(2),
                content: b"#!/bin/sh\n".to_vec(),
            }),
        };
        let output = render(&[file], &DiffOptions::default());
        assert!(output.contains("old mode 100644"));
        assert!(output.contains("new mode 100755"));
        assert!(!output.contains("@@"));
    }

    #[test]
    fn detects_binary_content() {
        let file = DiffFile {
            path: b"image.bin".to_vec(),
            old: Some(side(b"abc\0def")),
            new: Some(side(b"abc\0xyz")),
        };
        let output = render(&[file], &DiffOptions::default());
        assert!(output.contains("Binary files a/image.bin and b/image.bin differ"));
    }

    #[test]
    fn renders_stat_summary() {
        let file = DiffFile {
            path: b"file.txt".to_vec(),
            old: Some(side(b"one\ntwo\n")),
            new: Some(side(b"one\nthree\n")),
        };
        let output = render(
            &[file],
            &DiffOptions {
                context: 3,
                stat: true,
            },
        );
        assert!(output.contains("file.txt |"));
        assert!(output.contains("1 file changed"));
        assert!(output.contains("insertion"));
        assert!(output.contains("deletion"));
    }
}
