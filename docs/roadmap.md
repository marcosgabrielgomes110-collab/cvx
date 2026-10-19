# Roadmap

CVX ships in explicit compatibility slices. A feature is only announced when it
is implemented, covered by tests and documented in the compatibility matrix.

| Phase | Scope | Status |
| --- | --- | --- |
| 0 | Specification, architecture, compatibility contract | Complete |
| 1 | Objects, index, refs, commits, status, log, pack reads | Complete |
| 2A | Diff, restore/checkout, branches, switch | Complete |
| 2B | Merge, conflicts, tags creation, rename detection | Complete |
| 3 | Pack writing, clone/fetch/push, SSH/HTTPS, credentials | Next |
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

- pack writing and object indexing;
- Git protocol v2 discovery and negotiation;
- clone, fetch and push over HTTPS and SSH;
- credential helpers and OS keychain integration;
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
