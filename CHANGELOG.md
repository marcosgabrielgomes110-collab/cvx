# Changelog

All notable changes to CVX are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

_Phase 4 GitHub integration is in progress._

### Added

- `cvx github login|status|logout` with keychain/file credential reuse.
- GitHub REST client for repositories, issues, pull requests, releases and
  workflow runs.
- `cvx github repo create|delete|show`, including repository creation and push.
- `cvx github issue create|list|close`.
- `cvx github pr create|list|merge`.
- `cvx github release create|list` and `cvx github workflows`.

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
