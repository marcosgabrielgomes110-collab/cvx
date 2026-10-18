# Compatibility matrix

The local core is deliberately narrow. A feature marked **No** produces an
explicit error or is documented as out of scope; it is never silently ignored.

## Repository format

| Feature | Read | Write | Notes |
| --- | --- | --- | --- |
| `.cvx/` control directory | Yes | Yes | Git-compatible internals |
| `.git/` control directory | Yes | Yes | Compatibility mode |
| Loose objects (SHA-1) | Yes | Yes | Hash verified on read |
| Pack v2/v3 | Yes | No | Loose writes; pack writing comes with transport |
| Pack index v2 | Yes | No | Version-1 indexes are rejected |
| Delta chains (`ofs-delta`, `ref-delta`) | Yes | No | Depth and output size are bounded |
| Multi-pack index (MIDX) | No | No | Individual `.idx` files are used |
| `objects/info/alternates` | No | No | Rejected as unsupported |
| `packed-refs` | Yes | No | Loose refs take precedence; `git pack-refs` works |
| Index v2 | Yes | Yes | Default write format |
| Index v3/v4 | Yes | No | Read path; writes v2, or v3 when extended flags require it |
| Reflogs | Yes | Yes | Written for commits; `core.logAllRefUpdates` honored |
| SHA-256 repositories | No | No | Detected and rejected |
| Reftable storage | No | No | Detected and rejected |
| Bare repositories | No | No | Rejected during discovery |
| Linked worktrees | No | No | Rejected during discovery |
| Submodules | No | No | Nested repositories are out of scope |
| Git LFS | No | No | Pointer files are treated as ordinary blobs |

## Local operations

| Operation | Status |
| --- | --- |
| `cvx init` | Implemented |
| `cvx add` | Implemented |
| `cvx commit` | Implemented |
| `cvx status` | Implemented |
| `cvx log` | Implemented |
| `cvx version` | Implemented |
| `cvx diff` | Not implemented |
| `cvx checkout` / `switch` | Not implemented |
| `cvx branch` / `merge` | Not implemented |
| Tags creation | Not implemented |
| Annotated tag reading/peeling | Implemented |
| Rename detection in `status` | Not implemented; reports delete + add |
| `.gitignore` | Implemented for worktree walks and `add .` |
| `.git/info/exclude` | Read in compatibility mode |
| `.cvx/info/exclude` custom patterns | Not read; the control directory is skipped explicitly |
| Explicitly added ignored files | Added; Git would require `-f` |
| `.gitattributes` | Not implemented |
| CRLF / `core.autocrlf` normalization | Not implemented; `cvx add` rejects enabled autocrlf |
| Nested repositories / submodules | `cvx add` rejects them explicitly |
| Non-UTF-8 paths | Supported on Unix; rejected on Windows |
| Hooks execution | Not implemented |
| Commit signing | Not implemented |

## Interoperability

| Feature | Status |
| --- | --- |
| Open existing Git repository | Yes, compatibility mode |
| Git reads CVX repository | Yes, with `--git-dir=.cvx` |
| `git fsck` on CVX repository | Yes |
| Read repositories packed by `git gc` | Yes |
| Commit to a packed Git repository | Yes |
| Clone / fetch / push | Not implemented |
| GitHub API and authentication | Not implemented |
| Shallow and partial clones | Not implemented |
| Replace refs and grafts | Not implemented |

## Test oracle

The official `git` binary is used only by tests and CI. It is not invoked by
the CVX binary. Differential tests create repositories with both tools, operate
with one and validate with the other, including packed repositories created by
`git gc`, version-4 indexes, detached heads and packed refs.
