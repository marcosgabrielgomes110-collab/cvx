use std::fmt::Write;

use cvx_core::{Error, Result};
use cvx_repository::{Head, Repository, Status, StatusEntry, StatusKind, StatusOptions};

use crate::cli::{StatusArgs, UntrackedArg};

/// Executes `cvx status`.
pub fn run(args: StatusArgs) -> Result<()> {
    let cwd = std::env::current_dir().map_err(|error| Error::io("current directory", error))?;
    let repository = Repository::discover(&cwd)?;
    let status = repository.status(StatusOptions {
        untracked: args.untracked_files != UntrackedArg::No,
        collapse_untracked: args.untracked_files != UntrackedArg::All,
    })?;

    if args.porcelain || args.short {
        print_porcelain(&status);
    } else {
        print_human(&status);
    }

    Ok(())
}

struct PorcelainEntry {
    index: char,
    worktree: char,
    old_path: Option<Vec<u8>>,
}

impl Default for PorcelainEntry {
    fn default() -> Self {
        Self {
            index: ' ',
            worktree: ' ',
            old_path: None,
        }
    }
}

fn print_porcelain(status: &Status) {
    let mut tracked: std::collections::BTreeMap<Vec<u8>, PorcelainEntry> =
        std::collections::BTreeMap::new();

    for conflict in &status.conflicts {
        let mut code = conflict.kind.code().chars();
        tracked.insert(
            conflict.path.clone(),
            PorcelainEntry {
                index: code.next().unwrap_or('U'),
                worktree: code.next().unwrap_or('U'),
                old_path: None,
            },
        );
    }
    for entry in &status.staged {
        let tracked_entry = tracked.entry(entry.path.clone()).or_default();
        tracked_entry.index = status_code(entry.kind);
        if let Some(old_path) = &entry.old_path {
            tracked_entry.old_path = Some(old_path.clone());
        }
    }
    for entry in &status.unstaged {
        tracked.entry(entry.path.clone()).or_default().worktree = status_code(entry.kind);
    }

    for (path, entry) in tracked {
        let display = match entry.old_path {
            Some(old_path) => format!(
                "{} -> {}",
                display_path(&old_path, false),
                display_path(&path, false)
            ),
            None => display_path(&path, false),
        };
        println!("{}{} {display}", entry.index, entry.worktree);
    }
    for entry in &status.untracked {
        println!("?? {}", display_path(&entry.path, entry.is_dir));
    }
}

fn print_human(status: &Status) {
    match &status.head {
        Head::Symbolic { target, .. } => {
            let branch = target.strip_prefix("refs/heads/").unwrap_or(target);
            println!("On branch {branch}");
        }
        Head::Detached(oid) => {
            println!("HEAD detached at {}", oid.short(7));
        }
    }

    if status.unborn {
        println!();
        println!("No commits yet");
    }

    if !status.conflicts.is_empty() {
        println!();
        println!("Unmerged paths:");
        println!("  (fix conflicts and run \"cvx commit\")");
        for conflict in &status.conflicts {
            println!(
                "        {:<16}{}",
                format!("{}:", conflict.kind.label()),
                display_path(&conflict.path, false)
            );
        }
    }

    if !status.staged.is_empty() {
        println!();
        println!("Changes to be committed:");
        println!("  (use \"cvx add <file>...\" to update what will be committed)");
        for entry in &status.staged {
            println!(
                "        {:<14}{}",
                format!("{}:", label(entry.kind)),
                display_entry_path(entry)
            );
        }
    }

    if !status.unstaged.is_empty() {
        println!();
        println!("Changes not staged for commit:");
        println!("  (use \"cvx add <file>...\" to update what will be committed)");
        for entry in &status.unstaged {
            println!(
                "        {:<14}{}",
                format!("{}:", label(entry.kind)),
                display_entry_path(entry)
            );
        }
    }

    if !status.untracked.is_empty() {
        println!();
        println!("Untracked files:");
        println!("  (use \"cvx add <file>...\" to include in what will be committed)");
        for entry in &status.untracked {
            println!("        {}", display_path(&entry.path, entry.is_dir));
        }
    }

    if status.is_clean() {
        println!();
        if status.unborn {
            println!("nothing to commit (create/copy files and use \"cvx add\" to track)");
        } else {
            println!("nothing to commit, working tree clean");
        }
    }
}

fn status_code(kind: StatusKind) -> char {
    match kind {
        StatusKind::Added => 'A',
        StatusKind::Modified => 'M',
        StatusKind::Deleted => 'D',
        StatusKind::TypeChanged => 'T',
        StatusKind::Renamed => 'R',
    }
}

fn label(kind: StatusKind) -> &'static str {
    match kind {
        StatusKind::Added => "new file",
        StatusKind::Modified => "modified",
        StatusKind::Deleted => "deleted",
        StatusKind::TypeChanged => "typechange",
        StatusKind::Renamed => "renamed",
    }
}

fn display_entry_path(entry: &StatusEntry) -> String {
    match &entry.old_path {
        Some(old_path) => format!(
            "{} -> {}",
            display_path(old_path, false),
            display_path(&entry.path, false)
        ),
        None => display_path(&entry.path, false),
    }
}

fn display_path(path: &[u8], is_dir: bool) -> String {
    let text = String::from_utf8_lossy(path);
    let needs_quote = text.chars().any(|character| {
        character.is_whitespace() || matches!(character, '"' | '\\') || character.is_control()
    });

    let mut output = if needs_quote {
        let mut escaped = String::new();
        for character in text.chars() {
            match character {
                '"' => escaped.push_str("\\\""),
                '\\' => escaped.push_str("\\\\"),
                '\n' => escaped.push_str("\\n"),
                '\t' => escaped.push_str("\\t"),
                character if character.is_control() => {
                    let _ = write!(escaped, "\\{:03o}", character as u32);
                }
                character => escaped.push(character),
            }
        }
        format!("\"{escaped}\"")
    } else {
        text.into_owned()
    };

    if is_dir && !output.ends_with('/') {
        if output.ends_with('"') {
            output.pop();
            output.push('/');
            output.push('"');
        } else {
            output.push('/');
        }
    }

    output
}
