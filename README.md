# CVX 🦀

**Own your history. Connect your code.**

CVX (Code Version eXchange) is a Git-compatible version control system written
in Rust. It reads and writes the Git repository format, so existing history and
tooling keep working, but the engine does not shell out to the `git` executable.

> **Status: local core alpha.** `cvx init`, `add`, `commit`, `status` and `log`
> are implemented. Network operations, checkout, merges, tags creation and the
> GitHub integration are not available yet. Commands are announced here as they
> become usable, never before.

## Design decisions

- **Git-compatible storage.** CVX does not invent a new object format. A new
  repository stores the standard Git layout inside `.cvx/`.
- **CVX-branded control directory.** New repositories use `.cvx/`; existing
  Git repositories in `.git/` are opened directly in compatibility mode.
- **No runtime Git dependency.** The official Git binary is used only as a
  compatibility oracle in tests. Production code never invokes it.
- **Engine written in Rust.** Parsing, hashing, object storage, index, refs,
  packfiles and commit traversal are implemented in this workspace, with
  primitive crates for SHA-1, zlib, byte strings and command-line parsing.
- **Loose writes, packed reads.** CVX writes loose objects and reads both loose
  objects and version-2 packfiles, including delta chains.
- **SHA-1 first.** Repositories declaring SHA-256 are detected and rejected
  explicitly instead of being opened with mixed identifier widths.
- **Unsupported means unsupported.** Features outside the current milestone
  fail with a clear error; they are never silently simulated.

## Quick start

```console
$ cargo build --workspace
$ ./target/debug/cvx init /tmp/demo
Initialized empty CVX repository in /tmp/demo/.cvx/
$ cd /tmp/demo
$ printf 'hello\n' > hello.txt
$ cvx add .
$ cvx status
On branch main

No commits yet

Changes to be committed:
  (use "cvx add <file>..." to update what will be committed)
        new file:     hello.txt

$ cvx commit -m "feat: first commit"
[main (root-commit) 1c2d3e4] feat: first commit

$ cvx log --oneline
1c2d3e4 feat: first commit
```

The same repository can be inspected with Git:

```console
$ git --git-dir=.cvx --work-tree=. status
$ git --git-dir=.cvx --work-tree=. fsck --strict
$ git --git-dir=.cvx --work-tree=. log
```

The `.cvx/` directory is hidden from Git's status by `.cvx/info/exclude`, so a
CVX worktree stays clean when inspected with the Git CLI.

To create a standard `.git/` repository instead:

```console
$ cvx init --storage git
```

Existing Git repositories are detected automatically:

```console
$ cvx init
Detected existing Git repository in /path/to/repo/.git/; CVX will use it in compatibility mode
```

Existing `.git/` repositories can be read, modified and committed to directly,
including repositories whose objects have been packed by `git gc`.

## Commands

| Command | Notes |
| --- | --- |
| `cvx init [DIR]` | `--initial-branch`, `--storage cvx\|git`, `--quiet` |
| `cvx add <PATH>...` | `-A/--all`, `-u/--update`, `-f/--force` |
| `cvx commit -m <MSG>` | Repeat `-m` for paragraphs; `--allow-empty` |
| `cvx status` | `--porcelain`, `--short`, `--untracked-files=no\|normal\|all` |
| `cvx log [REVISION]` | `--oneline`, `-n/--max-count` |
| `cvx version` | Engine and repository format information |

## Workspace

The workspace contains the full crate map from the architecture:

| Area | Crates |
| --- | --- |
| Engine | `cvx-core`, `cvx-object`, `cvx-odb`, `cvx-pack`, `cvx-index`, `cvx-ref`, `cvx-config`, `cvx-ignore`, `cvx-attributes`, `cvx-revision`, `cvx-worktree`, `cvx-diff`, `cvx-merge` |
| Orchestration | `cvx-repository`, `cvx-cli` |
| Network and GitHub | `cvx-protocol`, `cvx-transport`, `cvx-credential`, `cvx-github-api`, `cvx-github-workflow` |
| Testing | `cvx-testkit` |

Crates outside the current milestone contain documentation but no public API
yet. They will be filled as the corresponding phases start.

## Build and test

```console
$ cargo build --workspace
$ cargo test --workspace
$ cargo fmt --all --check
$ cargo clippy --workspace --all-targets -- -D warnings
```

The differential tests require a `git` binary in `PATH`; the CVX binary itself
does not.

## Compatibility

See [docs/compatibility.md](docs/compatibility.md) for the current matrix and
[docs/architecture/overview.md](docs/architecture/overview.md) for the design.

Implemented compatibility includes loose objects, version-2 packfiles with
delta chains, index versions 2 through 4 (writing version 2, or 3 when extended
flags are needed), loose and packed refs, reflogs, annotated tag peeling,
`.gitignore` rules and worktree status.

Not implemented yet: clone/fetch/push, checkout, branch creation, merge, diff,
tags creation, SHA-256 repositories, reftable, submodules, LFS, hooks, commit
signing, `.gitattributes`, CRLF normalization and binary pack writing.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
