# Compatibility matrix

CVX prioritizes Git interoperability. A feature marked **No** produces an
explicit error or is documented as out of scope; it is never silently ignored.

## Repository format

| Feature | Read | Write | Notes |
| --- | --- | --- | --- |
| `.cvx/` control directory | Yes | Yes | Git-compatible internals |
| `.git/` control directory | Yes | Yes | Compatibility mode |
| Loose objects (SHA-1) | Yes | Yes | Hash verified on read |
| Pack v2/v3 | Yes | v2 | Writer emits complete objects, not deltas |
| Pack index v2 | Yes | Yes | Version-1 indexes are rejected |
| Delta chains (`ofs-delta`, `ref-delta`) | Yes | No | Depth and output size are bounded |
| Multi-pack index (MIDX) | No | No | Individual `.idx` files are used |
| `objects/info/alternates` | No | No | Rejected as unsupported |
| `packed-refs` | Yes | Yes | Loose refs take precedence; refs can be deleted from packed-refs |
| Index v2 | Yes | Yes | Default write format |
| Index v3/v4 | Yes | No | Read path; writes v2, or v3 when extended flags require it |
| Reflogs | Yes | Yes | Commits and branch switches |
| SHA-256 repositories | No | No | Detected and rejected |
| Reftable storage | No | No | Detected and rejected |
| Bare repositories | No | No | Rejected during discovery |
| Linked worktrees | No | No | Rejected during discovery |
| Submodules | No | No | Checkout and diff reject gitlinks |
| Git LFS | No | No | Pointer files are treated as ordinary blobs |

## Local operations

| Operation | Status | Notes |
| --- | --- | --- |
| `cvx init` | Implemented | `.cvx` or `--storage git` |
| `cvx add` | Implemented | Pathspecs, ignore rules, modes, symlinks |
| `cvx commit` | Implemented | Identities, reflogs, atomic refs |
| `cvx status` | Implemented | Porcelain/short/human output |
| `cvx log` | Implemented | Oneline, limits, annotated-tag peeling |
| `cvx diff` | Implemented | Worktree/index, `--cached`, revisions, pathspecs, `--stat` |
| `cvx restore` | Implemented | Staged/worktree, `--source`, untracked protection |
| `cvx checkout -- <paths>` | Implemented | Worktree restore from index |
| `cvx branch` | Implemented | List/create/delete, merged-branch check |
| `cvx switch` | Implemented | `-c`, detached HEAD, `--force` |
| `cvx merge` | Implemented | Fast-forward, three-way merge, two-parent commits |
| `cvx merge --abort` / `--continue` | Implemented | `MERGE_HEAD`, `MERGE_MSG` and `ORIG_HEAD` state |
| Merge conflicts | Implemented | Markers in the worktree and index stages 1/2/3 |
| Merge rename detection | No | Renames merge as independent delete and add paths |
| Directory/file merge conflicts | Limited | Rejected explicitly before writing |
| `cvx tag` | Implemented | Lightweight and annotated tags, list and force |
| `cvx log --decorate` | Implemented | Local branches and tags, peeled annotated tags |
| `cvx diff` rename detection | Implemented | Content similarity, 50% threshold |
| `cvx status` rename detection | Implemented | Staged renames (HEAD to index) |
| `cvx diff` non-UTF-8 text | Limited | Non-UTF-8 text is rendered lossily |
| `cvx diff` combined conflict diff | No | Diff with unmerged entries is rejected |
| `cvx reset` / `revert` / `cherry-pick` | Not implemented | Planned later |
| `.gitignore` | Implemented | Worktree walks and `add .` |
| `.git/info/exclude` | Read | Compatibility mode |
| `.cvx/info/exclude` custom patterns | Not read | Control directory is skipped explicitly |
| Explicitly added ignored files | Added | Git would require `-f` |
| `.gitattributes` | Not implemented | |
| CRLF / `core.autocrlf` normalization | Not implemented | `cvx add` rejects enabled autocrlf |
| Nested repositories / submodules | Rejected by `add` | |
| Non-UTF-8 paths | Supported on Unix | Rejected on Windows |
| Hooks execution | Not implemented | |
| Commit signing | Not implemented | |

## Worktree safety

- `restore`, `checkout` and `switch` refuse to follow symlinked ancestors.
- Untracked files are not overwritten unless `--force` is given.
- `switch` refuses tracked local changes unless `--force` is given.
- `merge` refuses staged, unstaged or unresolved changes and untracked-file
  collisions before writing anything.
- `merge --abort` restores `ORIG_HEAD`, the worktree and the index.
- Directories are never recursively deleted by checkout; a directory in the
  way is an explicit error.
- Worktree writes use temporary files and atomic replacement where possible.
- Index updates use `index.lock`; ref updates use compare-and-swap locks.

## Interoperability

| Feature | Status |
| --- | --- |
| Open existing Git repository | Yes, compatibility mode |
| Git reads CVX repository | Yes, with `--git-dir=.cvx` |
| `git fsck` on CVX repository | Yes |
| Read repositories packed by `git gc` | Yes |
| Commit to a packed Git repository | Yes |
| Switch branches in a packed repository | Yes |
| Merge commits and conflicted index stages | Yes |
| Annotated and lightweight tags | Yes |
| Pack writing | Yes | Complete objects and version-2 indexes |
| Pack ingestion | Yes | Self-contained packs; thin packs are rejected |
| Clone / fetch / push over local paths | Yes | `file://` and filesystem paths |
| Clone / fetch / push over HTTPS | Yes | Protocol v2 fetch, receive-pack push |
| Clone / fetch / push over SSH | Yes | System OpenSSH, keys and `ssh-agent` |
| HTTPS token authentication | Yes | Basic auth via env or credential store |
| OS keychain integration | Yes | Falls back to a file when unavailable |
| Credential file | Yes | User-only `0600`, outside the repository |
| Credential helper | Yes | `fill`, `approve`, `reject` |
| Shallow clone | Yes | `--depth` for network clones |
| Partial clone | Limited | `--filter=blob:none`; lazy fetch per missing object |
| Shallow fetch update / unshallow | Not implemented | |
| Partial clone with arbitrary filters | Limited | `blob:none` is the tested filter |
| GitHub token authentication | Yes | Keychain or `0600` credential file |
| GitHub repositories | Yes | Create, delete, show |
| GitHub issues | Yes | Create, list, close |
| GitHub pull requests | Yes | Create, list, merge |
| GitHub reviews and checks | Not implemented | |
| GitHub labels and milestones | Not implemented | |
| GitHub releases | Yes | Create, list |
| GitHub workflow runs | Yes | Read-only listing |
| GitHub OAuth / device flow | Not implemented | |
| GraphQL | Not implemented | |
| Replace refs and grafts | Not implemented | |

## Test oracle

The official `git` binary is used only by tests and CI. It is not invoked by
the CVX binary. Differential tests create repositories with both tools, operate
with one and validate with the other, including packed repositories created by
`git gc`, version-4 indexes, detached heads, packed refs and branch switching.
