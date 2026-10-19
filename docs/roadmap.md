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
| 4 | GitHub API, auth, pull requests, issues, releases | In progress — core workflows implemented |
| 5 | Fuzzing, crash recovery, benchmarks, release hardening | Ongoing |

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

Implemented:

- token authentication through the existing keychain/file credential store;
- repository creation, deletion and inspection;
- pull request creation, listing, reviews and merge;
- check-run listing;
- issue creation, listing and closing;
- repository label creation, listing, deletion and issue/PR assignment;
- release creation and listing;
- workflow-run visibility.

Remaining:

- OAuth/device flow and scope management;
- repository permissions and collaborator management;
- review-thread resolution and required checks;
- milestones and assignees;
- GraphQL queries and richer release assets.

## Phase 5 — release quality

- fuzzing for pack, index, object and config parsers;
- crash failpoints and recovery tests;
- benchmarks on 10k/100k-file repositories;
- SHA-256 repository support;
- `.gitattributes` and line-ending normalization;
- release signing and reproducible artifacts.

## Definition of done

A phase is done when:

1. the user-facing commands are implemented;
2. `git fsck` and differential tests pass;
3. supported and unsupported cases are documented;
4. CI is green on Linux, macOS and Windows;
5. no new runtime dependency on the `git` executable was introduced.
