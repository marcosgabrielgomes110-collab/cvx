# Changelog

All notable changes to CVX are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

_Post-0.6.0 work._

## [0.6.0] - 2026-10-22

### Added

- Offset-delta pack writing, with bounded delta chains, and a full `gc
  --prune` repack that removes superseded packs while respecting `.keep`.
- Bare repository support: `cvx init --bare`, `cvx clone --bare`, bare
  discovery, fetch/push, log, tag, fsck and gc.
- Crash-recovery coverage for stash and hard reset, including failpoints around
  stash publication.

### Changed

- `gc` now includes index entries, reflogs, stash refs and control references
  in the reachable set before pruning.
- Bare worktree operations fail explicitly instead of touching repository
  internals.

## [0.5.0] - 2026-10-22

### Added

- `cvx fsck` for references, objects, trees, tags, index entries and dangling
  objects.
- `cvx gc` with optional `--prune` to consolidate loose objects into packs.
- `cvx reset --soft|--mixed|--hard` with `ORIG_HEAD` and reflog updates.
- Revision ancestry suffixes such as `HEAD~2` and `main^`.
- `cvx cherry-pick` and `cvx revert` for clean single-parent commits.
- `cvx stash push|list|apply|pop|drop` for tracked changes.

### Changed

- `git fsck --strict` remains green after reset, cherry-pick, revert, stash and
  garbage collection.

## [0.4.0] - 2026-10-21

### Added

- `.gitattributes` parsing and matching for `text`, `text=auto`, `-text`,
  `eol=lf`, `eol=crlf` and `binary`, including nested attribute files.
- CRLF normalization on add/checkout and `core.autocrlf` support.
- Repository-level benchmarks for 10k files and opt-in 100k files.
- Local SHA-256 repositories via `cvx init --object-format sha256`, including
  loose objects, packs, pack indexes, the index file, refs and local clones.
- Release checksum verification and deterministic build timestamp.

### Changed

- `cvx add` no longer rejects `core.autocrlf`.

## [0.3.1] - 2026-10-21

### Added

- Property-based fuzz tests for pack, index, object, config and protocol
  parsers.
- Crash failpoints for commit, index, ref and pack publication, with recovery
  tests.
- Criterion benchmarks for pack write/ingest, index serialize/parse and diff
  rendering.

## [0.3.0] - 2026-10-19

### Added

- GitHub OAuth device flow with configurable client id and scopes.
- `cvx github scopes` with required-scope checks using GitHub's OAuth headers.
- Repository collaborator listing, add, remove and effective permission lookup.
- Pull-request review-thread listing through GraphQL, plus resolve and unresolve.
- Required status-check inspection and replacement for protected branches.
- Issue and pull-request assignees, including add and remove operations.
- Milestone listing, creation and closing, plus milestone assignment on issues.
- Generic `cvx github graphql` queries with inline or file-based variables.
- Release asset listing, upload, clobber protection and deletion.
- Release creation with automatic asset uploads.
- GitHub CLI help coverage and OAuth/device-flow parser tests.
- The CVX repository now dogfoods `.cvx/` and publishes with `cvx push`.

### Changed

- GitHub issue creation accepts assignees and milestones.
- Version bumped to 0.3.0.

## [0.2.1] - 2026-10-19

### Added

- GitHub pull-request reviews: create approvals, change requests and comments,
  and list reviews.
- GitHub check-run listing for a commit, ref or local `HEAD`.
- GitHub labels: create, list, delete, add to and remove from issues/PRs.
- `cvx github login|status|logout` with keychain/file credential reuse.
- GitHub REST client for repositories, issues, pull requests, releases and
  workflow runs.
- `cvx github repo create|delete|show`, including repository creation and push.
- `cvx github issue create|list|close`.
- `cvx github pr create|list|merge`.
- `cvx github release create|list` and `cvx github workflows`.

### Fixed

- Windows builds no longer import Unix-only credential helpers.
- Remote configuration values are quoted and escaped, fixing Windows paths and
  URLs with spaces.
- SCP-like URL detection no longer mistakes Windows absolute or extended paths
  for SSH remotes.

## [0.2.0] - 2026-10-19

### Added

- Version-2 pack writing with complete objects and matching version-2 indexes.
- Object enumeration and commit/tree/tag reachability.
- Local-path and `file://` transport: `cvx clone`, `cvx fetch` and `cvx push`.
- `cvx remote` listing and `cvx remote add`, plus `refs/remotes/*` tracking
  branches, `FETCH_HEAD` and remote `HEAD` symbols.
- Protocol v2 pkt-line framing, capability/response parsing and
  `ls-refs`/`fetch` request builders in `cvx-protocol`.
- HTTPS smart transport with protocol v2 clone/fetch, receive-pack push and
  pack ingestion.
- SSH transport through the system OpenSSH client, including `ssh-agent` and
  `CVX_SSH_KEY`.
- OS keychain credential backend with a user-only (`0600`) file fallback.
- `cvx credential fill|approve|reject` Git credential-helper operations.
- Shallow clones with `--depth` and partial clones with `--filter=blob:none`
  plus lazy blob fetch.
- `cvx credential store|list|remove` with a user-only (`0600`) credential file
  outside the repository; tokens are read from stdin and never printed.
- `cvx merge` with fast-forward and three-way merges, including merge commits
  with two parents.
- Conflict markers in the worktree, index stages 1/2/3 and precise porcelain
  conflict codes (`UU`, `AA`, `DU`, `UD`, `AU`, `UA`).
- `cvx merge --abort` and `cvx merge --continue`, backed by `MERGE_HEAD`,
  `MERGE_MSG` and `ORIG_HEAD`.
- `cvx tag` with lightweight and annotated tags, listing and force updates.
- `cvx log --decorate` for local branches and tags.
- Rename detection in `cvx status` and `cvx diff`.
- `cvx diff` for worktree/index, `--cached`, revision-to-revision, pathspecs
  and `--stat` output.
- `cvx restore` with staged and worktree modes, `--source` revisions and
  untracked-file protection.
- `cvx checkout -- <paths>` as a focused worktree restore alias.
- `cvx branch` list/create/delete with merged-branch checks.
- `cvx switch` with branch creation (`-c`), detached HEAD and `--force`.
- Safe worktree checkout with symlink-ancestor protection.
- Mermaid architecture, workflow and object-model diagrams.
- Community profile files: contributing guide, security policy, code of
  conduct, issue and pull-request templates, CODEOWNERS and Dependabot.
- Release workflow for tagged builds on Linux, macOS and Windows.
- Benchmark and performance documentation.

### Changed

- `cvx status` now reports staged renames and precise unmerged status codes
  (`UU`, `AA`, `DU`, `UD`, `AU`, `UA`).
- `cvx status` now ignores executable-bit differences when
  `core.filemode=false`.
- The README now documents the complete local workflow and architecture.

## [0.1.0] - 2026-10-18

### Added

- Initial Cargo workspace with the full CVX crate map.
- `.cvx/` control directory with Git-compatible storage.
- Compatibility mode for existing `.git/` repositories.
- `cvx init` with `--initial-branch`, `--storage cvx|git` and `--quiet`.
- `cvx add` with pathspecs, `.gitignore`, modes, symlinks and index locking.
- `cvx commit` with tree building, identities, reflogs and atomic ref updates.
- `cvx status` with porcelain/short/human output and untracked-file modes.
- `cvx log` with `--oneline`, `-n`, annotated-tag peeling and commit walks.
- SHA-1 object hashing, loose objects, packfile v2/v3 reads and delta chains.
- Index versions 2 through 4 on input, version-2/3 writes.
- Loose refs, packed refs, detached HEAD and compare-and-swap ref updates.
- Differential tests against the official Git binary.
- MIT/Apache-2.0 licensing and CI for Linux, macOS and Windows.
