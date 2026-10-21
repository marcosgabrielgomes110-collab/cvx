# Roadmap

CVX ships in explicit compatibility slices. A feature is only announced when it
is implemented, covered by tests and documented in the compatibility matrix.

| Phase | Scope | Status |
| --- | --- | --- |
| 0 | Specification, architecture, compatibility contract | Complete |
| 1 | Objects, index, refs, commits, status, log, pack reads | Complete |
| 2A | Diff, restore/checkout, branches, switch | Complete |
| 2B | Merge, conflicts, tags creation, rename detection | Complete |
| 3 | Pack writing, clone/fetch/push, SSH/HTTPS, credentials | Complete |
| 4 | GitHub API, OAuth, collaborators, reviews, checks, milestones, GraphQL, release assets | Complete — v0.3.0 |
| 5 | Fuzzing, crash recovery, benchmarks, release hardening | In progress |

## Phase 2B — merge and history workflows

- three-way merge with base/ours/theirs;
- conflict markers and index stages 1/2/3;
- precise conflict status codes;
- merge commits with two parents;
- `merge --abort` and `merge --continue`;
- annotated and lightweight tag creation;
- `log --decorate`;
- rename detection in status and diff.

## Phase 3 — transport

Implemented:

- pack writing, pack ingestion and version-2 index generation;
- object enumeration and reachability;
- clone, fetch, push and remotes over local paths, `file://`, `https://`,
  `ssh://` and SCP-like URLs;
- protocol v2 pkt-line framing, capability/response parsing and request builders;
- receive-pack push with report-status parsing;
- HTTPS basic authentication and SSH through the system OpenSSH client;
- OS keychain credentials with a user-only `0600` file fallback;
- Git credential-helper `fill`, `approve` and `reject`;
- shallow clones (`--depth`) and partial clones (`--filter=blob:none`) with
  lazy blob fetch.

Known limitations:

- shallow fetch updates/`--unshallow` are not implemented;
- partial clones fetch missing blobs one at a time;
- `--depth` and `--filter` are rejected for local-path clones;
- SSH requires an OpenSSH client, key or agent outside CVX.

## Phase 4 — GitHub

Complete in v0.3.0:

- token and OAuth device-flow authentication with keychain/file storage;
- OAuth scope inspection and required-scope checks;
- repository creation, deletion and inspection;
- collaborator listing, add/remove and effective permission lookup;
- pull request creation, listing, reviews, review threads and merge;
- review-thread resolution and unresolution through GraphQL;
- check-run listing and required status-check inspection/replacement;
- issue creation, listing, closing, assignees and milestones;
- repository label creation, listing, deletion and issue/PR assignment;
- release creation, listing, asset upload and deletion;
- workflow-run visibility;
- generic GraphQL queries with inline or file-based variables.

Known limitations:

- GraphQL review threads return the first 100 threads and comments;
- required status checks require branch protection to be enabled already;
- release uploads read the asset into memory before sending;
- fine-grained tokens may not expose OAuth scope headers.

## Phase 5 — release quality

Implemented:

- property-based fuzz tests for pack, index, object, config and protocol
  parsers;
- crash failpoints and recovery tests for commit, index, ref and pack
  publication;
- Criterion benchmarks for pack, index, diff and repository operations,
  including an opt-in 100k-file scenario;
- `.gitattributes` text/eol/binary matching with nested files;
- CRLF normalization for `core.autocrlf=true`, `input` and `false`;
- checksum verification and deterministic timestamp configuration in the
  release workflow.

Remaining:

- SHA-256 repository support;
- release signing and fully reproducible artifacts;
- working-tree encodings and diff/merge attribute drivers.

## Definition of done

A phase is done when:

1. the user-facing commands are implemented;
2. `git fsck` and differential tests pass;
3. supported and unsupported cases are documented;
4. CI is green on Linux, macOS and Windows;
5. no new runtime dependency on the `git` executable was introduced.
