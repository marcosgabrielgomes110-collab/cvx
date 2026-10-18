use std::fmt::Write;

use cvx_core::{Error, Result};
use cvx_repository::{Head, Repository, Status, StatusKind, StatusOptions};

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

fn print_porcelain(status: &Status) {
    let mut tracked: std::collections::BTreeMap<Vec<u8>, (char, char)> =
        std::collections::BTreeMap::new();

    for path in &status.conflicts {
        tracked.insert(path.clone(), ('U', 'U'));
    }
    for entry in &status.staged {
        tracked.entry(entry.path.clone()).or_insert((' ', ' ')).0 = status_code(entry.kind);
    }
    for entry in &status.unstaged {
        tracked.entry(entry.path.clone()).or_insert((' ', ' ')).1 = status_code(entry.kind);
    }

    for (path, (index, worktree)) in tracked {
        println!("{index}{worktree} {}", display_path(&path, false));
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
        for path in &status.conflicts {
            println!("        both modified:   {}", display_path(path, false));
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
                display_path(&entry.path, false)
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
                display_path(&entry.path, false)
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
    }
}

fn label(kind: StatusKind) -> &'static str {
    match kind {
        StatusKind::Added => "new file",
        StatusKind::Modified => "modified",
        StatusKind::Deleted => "deleted",
        StatusKind::TypeChanged => "typechange",
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
