# Compatibility matrix

The first milestone is deliberately narrow. A feature marked **No** produces an
explicit error or is documented as out of scope; it is never silently ignored.

## Repository format

| Feature | Read | Write | Notes |
| --- | --- | --- | --- |
| `.cvx/` control directory | Yes | Yes | Git-compatible internals |
| `.git/` control directory | Yes | Yes | Compatibility mode |
| Loose objects (SHA-1) | Yes | Yes | Hash verified on read |
| Pack v2 + `.idx` v2 | Yes | No | Includes delta chains |
| `packed-refs` | Yes | No | Loose refs take precedence |
| Index v2 | Yes | Yes | Default write format |
| Index v3/v4 | Yes | No | Path compression on read |
| Reflogs | Planned | Planned | Added with `commit` |
| SHA-256 repositories | No | No | Detected and rejected |
| Reftable storage | No | No | Detected and rejected |
| Bare repositories | No | No | Rejected during discovery |
| Linked worktrees | No | No | Rejected during discovery |

## Local operations

| Operation | Status |
| --- | --- |
| `cvx init` | Implemented |
| `cvx version` | Implemented |
| `cvx add` | Planned, next |
| `cvx commit` | Planned |
| `cvx status` | Planned |
| `cvx log` | Planned |
| `cvx diff` | Out of scope for v0.1 |
| `cvx checkout` / `switch` | Out of scope for v0.1 |
| `cvx branch` / `merge` | Out of scope for v0.1 |
| Tags creation | Out of scope for v0.1 |

## Interoperability

| Feature | Status |
| --- | --- |
| Open existing Git repository | Yes, compatibility mode |
| Git reads CVX repository | Yes, with `--git-dir=.cvx` |
| `git fsck` on CVX repository | Yes |
| Clone / fetch / push | Out of scope for v0.1 |
| GitHub API and authentication | Out of scope for v0.1 |
| `.gitignore` | Planned with `add` |
| `.gitattributes` | Out of scope for v0.1 |
| Submodules | Out of scope |
| Git LFS | Out of scope |
| Hooks execution | Out of scope |
| Commit signing | Out of scope |

## Test oracle

The official `git` binary is used only by tests and CI. It is not invoked by
the CVX binary. Differential tests create repositories with both tools, operate
with one and validate with the other, including packed repositories created by
`git gc`.
