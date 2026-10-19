# Contributing to CVX

Thanks for helping build CVX. The project is a Git-compatible version control
engine written in Rust. Compatibility is a product requirement, not an
implementation detail.

## Before you start

- Read [docs/architecture.md](docs/architecture.md) and
  [docs/compatibility.md](docs/compatibility.md).
- For a large change, open an issue first so the scope and compatibility
  impact can be discussed.
- Security issues must follow [SECURITY.md](SECURITY.md), not public issues.

## Development environment

Requirements:

- Rust stable (edition 2024; see `rust-toolchain.toml`);
- Git 2.30 or newer for differential tests only;
- Linux, macOS or Windows.

```console
$ git clone https://github.com/marcosgabrielgomes110-collab/cvx.git
$ cd cvx
$ cargo build --workspace
$ cargo test --workspace
```

Useful checks:

```console
$ cargo fmt --all --check
$ cargo clippy --workspace --all-targets -- -D warnings
$ cargo test --workspace --all-targets
$ cargo doc --workspace --no-deps
```

## Compatibility rules

- CVX must read and write the Git repository format for supported features.
- The official `git` binary is a test oracle, never a runtime dependency.
- New commands or options must not be announced in the README until they are
  implemented and covered by tests.
- Unsupported features must fail explicitly. Do not silently ignore a format
  or option that changes on-disk semantics.
- Every path that writes to the worktree must protect against traversal,
  symlinked ancestors and control-directory writes.

## Tests

- Unit tests belong next to the code they cover.
- CLI and differential tests live in `crates/cvx-cli/tests/`.
- Cross-crate compatibility fixtures belong in `tests/compatibility/`.
- When changing an on-disk format, add a test that validates the result with
  `git fsck`, `git cat-file`, `git status` or `git log`.
- When changing network-facing code later, test both success and failure paths.

## Style

- Follow `rustfmt` and `clippy`.
- Prefer small, named functions over deeply nested blocks.
- Keep crate boundaries meaningful: the CLI orchestrates, the engine decides.
- Use explicit errors (`cvx_core::Error`) with actionable messages.
- Document public APIs and format invariants.
- Avoid adding dependencies that implement the Git engine for us. Primitive
  crates for hashing, compression, diffing, CLI parsing and byte strings are
  welcome when they reduce risk.

## Commits

Use short, imperative subjects, optionally with a conventional prefix:

```text
feat: add cvx diff for index and worktree
fix: protect checkout against symlinked ancestors
docs: add Mermaid architecture diagrams
test: cover packed refs after branch deletion
chore: update dependencies
```

Keep commits focused. Explain compatibility or security implications in the
body when relevant.

## Pull requests

- Describe the user-visible change and the compatibility impact.
- List the tests you ran.
- Update `CHANGELOG.md` for notable changes.
- Update the compatibility matrix when support changes.
- Keep the CI matrix green on Linux, macOS and Windows.

## License

By contributing, you agree that your contribution is licensed under the
project's dual MIT/Apache-2.0 terms.
