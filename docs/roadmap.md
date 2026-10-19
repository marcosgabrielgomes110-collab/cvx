# Roadmap

CVX ships in explicit compatibility slices. A feature is only announced when it
is implemented, covered by tests and documented in the compatibility matrix.

| Phase | Scope | Status |
| --- | --- | --- |
| 0 | Specification, architecture, compatibility contract | Complete |
| 1 | Objects, index, refs, commits, status, log, pack reads | Complete |
| 2A | Diff, restore/checkout, branches, switch | Complete |
| 2B | Merge, conflicts, tags creation, rename detection | Complete |
| 3 | Pack writing, clone/fetch/push, SSH/HTTPS, credentials | In progress — HTTPS complete, SSH pending |
| 4 | GitHub API, auth, pull requests, issues, releases | Planned |
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
- clone, fetch, push and remotes over local paths, `file://` and `https://`;
- protocol v2 pkt-line framing, capability/response parsing and request builders;
- receive-pack push with report-status parsing;
- HTTPS basic authentication and secure user-only credential storage.

Remaining:

- SSH transport with keys and `ssh-agent`;
- OS keychain integration (the current store is a `0600` file);
- credential helper protocol compatibility;
- shallow and partial clone support where explicit.

## Phase 4 — GitHub

- authentication and token storage;
- repository creation and permissions;
- pull requests, reviews and checks;
- issues and labels;
- releases and workflow visibility.

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
