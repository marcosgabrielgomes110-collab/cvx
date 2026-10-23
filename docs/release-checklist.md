# Release checklist

## Before a release candidate

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings`
- [ ] `cargo test --workspace --locked`
- [ ] `git --git-dir=.cvx --work-tree=. fsck --strict`
- [ ] `CVX_SOAK_ITERATIONS=200 CVX_SOAK_JOBS=4 scripts/soak.sh`
- [ ] opt-in large-repository benchmark with a memory ceiling
- [ ] SHA-256 HTTP clone/push fixture
- [ ] SHA-256 SSH fixture
- [ ] `cargo-audit` / `cargo-deny`
- [ ] SBOM generated and inspected
- [ ] release artifacts signed and checksums verified
- [ ] reproducible build comparison across toolchains
- [ ] installers and migration notes tested
- [ ] security findings and known limitations reviewed

## Release candidate gate

- no open P0 data-loss or security issue;
- no known regression in the documented Git subset;
- CI green on Linux, macOS and Windows;
- rollback procedure exercised;
- release notes list supported and unsupported behavior.

## Stable release gate

- release candidate has run against real repositories for the agreed soak
  period;
- signed artifacts, SBOM and provenance are published;
- support and security-response policy is frozen;
- a post-release monitoring window is scheduled.
