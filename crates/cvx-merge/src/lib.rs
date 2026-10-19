//! Three-way merge and conflict representation.
//!
//! This crate implements the content-level merge used by `cvx merge`. The
//! repository layer decides which paths need merging and writes the resulting
//! blobs, index stages and worktree files.

#![forbid(unsafe_code)]

use cvx_diff::is_binary;
use similar::{DiffOp, TextDiff};

/// Labels shown in conflict markers.
#[derive(Debug, Clone, Copy)]
pub struct MergeLabels<'a> {
    /// Label for the current branch.
    pub ours: &'a str,
    /// Label for the branch being merged.
    pub theirs: &'a str,
}

impl Default for MergeLabels<'_> {
    fn default() -> Self {
        Self {
            ours: "HEAD",
            theirs: "theirs",
        }
    }
}

/// Result of merging one file's content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMerge {
    /// Merged or conflict-marked content.
    pub content: Vec<u8>,
    /// Whether the merge completed without conflicts.
    pub clean: bool,
}

/// Merges three versions of a file.
///
/// Identical changes on both sides are merged without a conflict. Binary and
/// non-UTF-8 content cannot be merged line by line and is reported as a
/// conflict while keeping our version.
pub fn merge_file(base: &[u8], ours: &[u8], theirs: &[u8], labels: MergeLabels<'_>) -> FileMerge {
    if ours == theirs {
        return FileMerge {
            content: ours.to_vec(),
            clean: true,
        };
    }
    if ours == base {
        return FileMerge {
            content: theirs.to_vec(),
            clean: true,
        };
    }
    if theirs == base {
        return FileMerge {
            content: ours.to_vec(),
            clean: true,
        };
    }

    if is_binary(base) || is_binary(ours) || is_binary(theirs) {
        return FileMerge {
            content: ours.to_vec(),
            clean: false,
        };
    }

    let (Ok(base), Ok(ours), Ok(theirs)) = (
        std::str::from_utf8(base),
        std::str::from_utf8(ours),
        std::str::from_utf8(theirs),
    ) else {
        return FileMerge {
            content: ours.to_vec(),
            clean: false,
        };
    };

    merge_text(base, ours, theirs, labels)
}

/// Merges a file mode.
///
/// Returns `None` when both sides changed the mode differently.
pub fn merge_mode(base: u32, ours: u32, theirs: u32) -> Option<u32> {
    if ours == theirs || theirs == base {
        Some(ours)
    } else if ours == base {
        Some(theirs)
    } else {
        None
    }
}

/// Builds conflict markers for two versions without a common base.
///
/// This is used for add/add conflicts, where Git always reports a conflict.
pub fn conflict_markers(ours: &[u8], theirs: &[u8], labels: MergeLabels<'_>) -> Vec<u8> {
    let mut output = String::new();
    let ours_text = String::from_utf8_lossy(ours);
    let theirs_text = String::from_utf8_lossy(theirs);
    output.push_str("<<<<<<< ");
    output.push_str(labels.ours);
    output.push('\n');
    output.push_str(&ours_text);
    if !ours_text.is_empty() && !ours_text.ends_with('\n') {
        output.push('\n');
    }
    output.push_str("=======\n");
    output.push_str(&theirs_text);
    if !theirs_text.is_empty() && !theirs_text.ends_with('\n') {
        output.push('\n');
    }
    output.push_str(">>>>>>> ");
    output.push_str(labels.theirs);
    output.push('\n');
    output.into_bytes()
}

#[derive(Debug, Clone)]
struct Hunk {
    start: usize,
    end: usize,
    replacement: Vec<String>,
}

fn merge_text(base: &str, ours: &str, theirs: &str, labels: MergeLabels<'_>) -> FileMerge {
    let base_lines = split_lines(base);
    let ours_lines = split_lines(ours);
    let theirs_lines = split_lines(theirs);

    let ours_hunks = hunks(&base_lines, &ours_lines);
    let theirs_hunks = hunks(&base_lines, &theirs_lines);

    let mut output = String::new();
    let mut clean = true;
    let mut position = 0usize;
    let mut ours_index = 0usize;
    let mut theirs_index = 0usize;

    while ours_index < ours_hunks.len() || theirs_index < theirs_hunks.len() {
        let take_ours = match (ours_hunks.get(ours_index), theirs_hunks.get(theirs_index)) {
            (Some(ours), Some(theirs)) => ours.start <= theirs.start,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => break,
        };

        let first = if take_ours {
            let hunk = ours_hunks[ours_index].clone();
            ours_index += 1;
            hunk
        } else {
            let hunk = theirs_hunks[theirs_index].clone();
            theirs_index += 1;
            hunk
        };

        let mut start = first.start;
        let mut end = first.end;
        let mut cluster_ours = Vec::new();
        let mut cluster_theirs = Vec::new();
        if take_ours {
            cluster_ours.push(first);
        } else {
            cluster_theirs.push(first);
        }

        loop {
            let mut expanded = false;
            while let Some(hunk) = ours_hunks.get(ours_index) {
                if overlaps(start, end, hunk) {
                    start = start.min(hunk.start);
                    end = end.max(hunk.end);
                    cluster_ours.push(ours_hunks[ours_index].clone());
                    ours_index += 1;
                    expanded = true;
                } else {
                    break;
                }
            }
            while let Some(hunk) = theirs_hunks.get(theirs_index) {
                if overlaps(start, end, hunk) {
                    start = start.min(hunk.start);
                    end = end.max(hunk.end);
                    cluster_theirs.push(theirs_hunks[theirs_index].clone());
                    theirs_index += 1;
                    expanded = true;
                } else {
                    break;
                }
            }
            if !expanded {
                break;
            }
        }

        if start > position {
            output.push_str(&base_lines[position..start].concat());
        }

        let ours_region = apply_hunks(&base_lines, start, end, &cluster_ours);
        let theirs_region = apply_hunks(&base_lines, start, end, &cluster_theirs);

        if cluster_theirs.is_empty() || ours_region == theirs_region {
            output.push_str(&ours_region.concat());
        } else if cluster_ours.is_empty() {
            output.push_str(&theirs_region.concat());
        } else {
            clean = false;
            push_conflict(&mut output, &ours_region, &theirs_region, labels);
        }

        position = end;
    }

    if position < base_lines.len() {
        output.push_str(&base_lines[position..].concat());
    }

    FileMerge {
        content: output.into_bytes(),
        clean,
    }
}

fn split_lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

fn hunks(base: &[&str], side: &[&str]) -> Vec<Hunk> {
    let diff = TextDiff::from_slices(base, side);
    let mut hunks = Vec::new();

    for operation in diff.ops() {
        let hunk = match *operation {
            DiffOp::Equal { .. } => continue,
            DiffOp::Delete {
                old_index, old_len, ..
            } => Hunk {
                start: old_index,
                end: old_index + old_len,
                replacement: Vec::new(),
            },
            DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => Hunk {
                start: old_index,
                end: old_index,
                replacement: side[new_index..new_index + new_len]
                    .iter()
                    .map(|line| (*line).to_owned())
                    .collect(),
            },
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => Hunk {
                start: old_index,
                end: old_index + old_len,
                replacement: side[new_index..new_index + new_len]
                    .iter()
                    .map(|line| (*line).to_owned())
                    .collect(),
            },
        };
        hunks.push(hunk);
    }

    hunks
}

fn overlaps(start: usize, end: usize, hunk: &Hunk) -> bool {
    if start == end && hunk.start == hunk.end {
        start == hunk.start
    } else if start == end {
        start >= hunk.start && start < hunk.end
    } else if hunk.start == hunk.end {
        hunk.start >= start && hunk.start < end
    } else {
        start < hunk.end && hunk.start < end
    }
}

fn apply_hunks(base: &[&str], start: usize, end: usize, hunks: &[Hunk]) -> Vec<String> {
    let mut output = Vec::new();
    let mut position = start;

    for hunk in hunks {
        if hunk.start > position {
            output.extend(
                base[position..hunk.start]
                    .iter()
                    .map(|line| (*line).to_owned()),
            );
        }
        output.extend(hunk.replacement.iter().cloned());
        position = hunk.end;
    }
    if position < end {
        output.extend(base[position..end].iter().map(|line| (*line).to_owned()));
    }

    output
}

fn push_conflict(output: &mut String, ours: &[String], theirs: &[String], labels: MergeLabels<'_>) {
    output.push_str("<<<<<<< ");
    output.push_str(labels.ours);
    output.push('\n');
    push_region(output, ours);
    output.push_str("=======\n");
    push_region(output, theirs);
    output.push_str(">>>>>>> ");
    output.push_str(labels.theirs);
    output.push('\n');
}

fn push_region(output: &mut String, lines: &[String]) {
    for line in lines {
        output.push_str(line);
    }
    if lines.last().is_some_and(|line| !line.ends_with('\n')) {
        output.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn merge(base: &str, ours: &str, theirs: &str) -> FileMerge {
        merge_file(
            base.as_bytes(),
            ours.as_bytes(),
            theirs.as_bytes(),
            MergeLabels {
                ours: "HEAD",
                theirs: "feature",
            },
        )
    }

    #[test]
    fn merges_non_overlapping_changes() {
        let result = merge(
            "one\ntwo\nthree\n",
            "ONE\ntwo\nthree\n",
            "one\ntwo\nTHREE\n",
        );
        assert!(result.clean);
        assert_eq!(result.content, b"ONE\ntwo\nTHREE\n");
    }

    #[test]
    fn accepts_identical_changes_on_both_sides() {
        let result = merge("one\ntwo\n", "one\nTWO\n", "one\nTWO\n");
        assert!(result.clean);
        assert_eq!(result.content, b"one\nTWO\n");
    }

    #[test]
    fn reports_overlapping_changes() {
        let result = merge(
            "one\ntwo\nthree\n",
            "one\nOURS\nthree\n",
            "one\nTHEIRS\nthree\n",
        );
        assert!(!result.clean);
        let content = String::from_utf8(result.content).expect("utf8");
        assert!(content.contains("<<<<<<< HEAD"));
        assert!(content.contains("OURS"));
        assert!(content.contains("======="));
        assert!(content.contains("THEIRS"));
        assert!(content.contains(">>>>>>> feature"));
    }

    #[test]
    fn merges_insertions_in_different_places() {
        let result = merge("a\nb\n", "a\nours\nb\n", "a\nb\ntheirs\n");
        assert!(result.clean);
        assert_eq!(result.content, b"a\nours\nb\ntheirs\n");
    }

    #[test]
    fn reports_binary_conflicts() {
        let result = merge_file(b"base\0", b"ours\0", b"theirs\0", MergeLabels::default());
        assert!(!result.clean);
        assert_eq!(result.content, b"ours\0");
    }

    #[test]
    fn merges_modes() {
        assert_eq!(merge_mode(0o100_644, 0o100_755, 0o100_644), Some(0o100_755));
        assert_eq!(merge_mode(0o100_644, 0o100_644, 0o100_644), Some(0o100_644));
        assert_eq!(merge_mode(0o100_644, 0o100_755, 0o100_600), None);
    }
}
