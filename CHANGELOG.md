# Changelog

All notable changes to CVX are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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
