# CVX architecture

## Core decision

CVX is a Git-compatible client, not a wrapper around the `git` executable. The
engine implements the repository formats and protocols directly in Rust. The
official Git binary is a test oracle, not a runtime dependency.

Two consequences shape the first milestone:

1. **Storage must be Git's storage.** New repositories use `.cvx/` as the
   control directory, but the contents are the Git repository format. Git can
   inspect them with `git --git-dir=.cvx --work-tree=. <command>`.
2. **Reading packfiles is mandatory early.** Real repositories are usually
   packed. `cvx status` and `cvx log` cannot work on a cloned or garbage-collected
   repository with only a loose-object reader.

## Control directories

| Situation | Control directory | Notes |
| --- | --- | --- |
| `cvx init` | `.cvx/` | Default; Git-compatible contents |
| `cvx init --storage git` | `.git/` | Strict Git interoperability |
| Existing Git repository | `.git/` | Opened in compatibility mode |
| Both `.cvx/` and `.git/` | `.cvx/` | CVX takes precedence during discovery |

A future `cvx convert` will switch between the two directory names without
rewriting objects, because both use the same format. Conversion is a rename,
not an import/export pipeline.

## Layers

```text
cvx-cli
   |
cvx-repository        use cases: init, add, commit, status, log,
   |                  diff, restore, checkout, branch, switch,
   |                  merge, tag, clone, remote, fetch, push
   |
   +-- cvx-revision   revision parsing, commit graph and merge base
   +-- cvx-worktree   worktree scan and status
   +-- cvx-diff       unified diff rendering and similarity
   +-- cvx-merge      three-way content merge and conflict markers
   +-- cvx-index      staging area
   +-- cvx-ref        refs, HEAD, packed-refs, reflogs
   +-- cvx-odb        loose objects + pack access
   |      |
   |      +-- cvx-pack
   +-- cvx-object     object model + hashing
   +-- cvx-config     Git config parser
   +-- cvx-core       errors, paths, fs helpers, ref-name validation
```

Dependency rules:

- `cvx-core` has no knowledge of objects, refs or network.
- `cvx-object` has no knowledge of repositories or GitHub.
- `cvx-pack` is a codec; `cvx-odb` composes loose and packed storage.
- `cvx-cli` orchestrates, but does not duplicate business rules.
- GitHub crates sit above the local engine and never become a dependency of it.

## Storage format choices

### Objects

- SHA-1 only in the first milestone.
- Loose objects for writes.
- Pack v2 and `.idx` v2 for reads, including `ofs-delta` and `ref-delta`.
- Object hashes are verified on read.
- Decompression and delta recursion are bounded.

### Index

- Write version 2.
- Read versions 2, 3 and 4.
- Preserve optional extensions when safe; reject required extensions that are
  not understood (for example split index).
- Compare against `git ls-files --stage -z` in differential tests.

### Refs

- Loose refs, `packed-refs`, symbolic refs and detached `HEAD`.
- Compare-and-swap ref updates and reflogs arrive with `commit`.
- Ref names follow `git check-ref-format` rules.

### Configuration

- Reads the same system, global and local `gitconfig` files as Git.
- Supports includes, quoting, comments, booleans and subsections.
- Conditional includes are deferred until repository context is available.

## Reliability

- Objects are written through a temporary file and renamed into place.
- Index and ref updates use lock files; refs update atomically.
- Atomic writes fsync file contents and the parent directory on Unix.
- Unsupported formats fail before any write.
- Crash points will be exercised with test-only failpoints during the first
  milestone's hardening phase.

## Current implementation

The local workflow implemented in this workspace covers:

- `init`, `add`, `commit`, `status`, `log`, `diff`, `restore`, `checkout`,
  `branch`, `switch`, `merge` and `tag`;
- three-way merges, conflict markers, index stages 1/2/3, merge commits with
  two parents and `merge --abort`/`--continue`;
- annotated and lightweight tags, `log --decorate` and rename detection;
- version-2 pack writing and pack-index generation;
- local-path clone, fetch, push and remote configuration;
- protocol v2 pkt-line and request builders;
- loose object writes and verified reads;
- version-2 packfiles and indexes, including delta chains;
- index versions 2 through 4 on input, version-2 writes (or version 3 when
  extended flags require it);
- loose refs, packed refs, detached `HEAD`, compare-and-swap ref updates,
  branch deletion and reflogs;
- recursive tree building, commit parsing and committer-date history walks;
- unified diffs, diffstats, safe worktree checkout and branch switching;
- Git-compatible `.gitignore` matching through the `ignore` crate.

See [the visual architecture guide](../architecture.md) for Mermaid diagrams of
the system, crate graph, object model and workflows.

## Non-goals for the current milestone

Network transports, reset, revert, cherry-pick, submodules, LFS, reftable,
SHA-256 repositories, bare repositories, linked worktrees, `.gitattributes`,
CRLF normalization, hooks and commit signing. These are explicit errors or
documented gaps, not silent no-ops.
