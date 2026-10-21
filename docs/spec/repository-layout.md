# Repository layout

## `.cvx/` directory

A CVX repository uses the Git repository layout under `.cvx/`:

```text
.cvx/
├── HEAD
├── config
├── description
├── cvx.toml            # CVX-specific metadata, ignored by Git
├── branches/
├── hooks/
├── info/
│   └── exclude         # includes /.cvx/ so Git status stays clean
├── objects/
│   ├── info/
│   └── pack/
└── refs/
    ├── heads/
    └── tags/
```

The files that Git understands are byte-for-byte compatible with Git's format.
The extra `cvx.toml` records the CVX repository format and object format; it is
not required to read the repository.

## Git interoperability

Git discovers `.git/` automatically. For `.cvx/`, pass the control directory
explicitly:

```console
$ git --git-dir=.cvx --work-tree=. status
$ git --git-dir=.cvx --work-tree=. fsck --strict
$ git --git-dir=.cvx --work-tree=. log
```

This is the same mechanism Git uses for bare repositories and separate git
directories, so no conversion or object rewriting is involved.

The default `.cvx/info/exclude` contains `/.cvx/`. That keeps the control
directory out of Git's untracked list when a CVX repository is inspected with
the Git CLI.

## `--storage git`

`cvx init --storage git` creates `.git/` instead of `.cvx/`. The contents are
the same; only the directory name differs. This mode is useful for tools that
cannot be told about a separate Git directory.

## Conversion plan

Conversion will be a safe directory rename with verification:

1. Refuse if both control directories exist and are non-empty.
2. Verify `HEAD`, `config` and `objects/` look valid.
3. Rename `.cvx` to `.git` (or the reverse).
4. Reopen and run integrity checks.

No object, index or reference data is transformed. The operation is planned for
a later milestone; it is not exposed as a command yet.

## Unsupported layouts

The first milestone rejects:

- bare repositories (`core.bare = true`);
- linked worktrees and submodule checkouts (`.git` is a file);
- reftable reference storage.
