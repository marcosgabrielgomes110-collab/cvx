## Summary

Describe the user-visible change and why it is needed.

## Compatibility impact

- [ ] New on-disk format behavior is covered by `git fsck`/`git cat-file` tests.
- [ ] Unsupported cases fail explicitly.
- [ ] `docs/compatibility.md` is updated when support changes.
- [ ] No runtime dependency on the `git` executable was introduced.

## Tests

List the commands and scenarios you ran.

```console
$ cargo fmt --all --check
$ cargo clippy --workspace --all-targets -- -D warnings
$ cargo test --workspace --all-targets
```

## Screenshots or terminal output

Include output for new or changed commands when useful.

## Checklist

- [ ] The change is focused and documented.
- [ ] `CHANGELOG.md` was updated for notable changes.
- [ ] Public APIs have documentation.
- [ ] Security-sensitive paths were considered (traversal, symlinks, locks,
      credentials).
