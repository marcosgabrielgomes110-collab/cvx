mod common;

use assert_cmd::prelude::*;
use common::{TestEnv, normalize, stdout};
use predicates::prelude::*;

#[test]
fn interrupted_commit_leaves_head_unchanged_and_can_retry() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "content\n");
    env.cvx().args(["add", "."]).assert().success();

    env.cvx()
        .env("CVX_FAILPOINT", "commit.before-ref-update")
        .args(["commit", "-m", "interrupted"])
        .assert()
        .failure()
        .code(128)
        .stderr(predicate::str::contains("injected failure"));

    let head = env.git_cvx(&["rev-parse", "--verify", "HEAD"]);
    assert!(!head.status.success(), "HEAD must remain unborn");
    let status = env.cvx().args(["status", "--porcelain"]).output().unwrap();
    assert_eq!(normalize(&status), "A  file.txt");

    env.cvx().args(["commit", "-m", "retry"]).assert().success();
    env.assert_git_cvx_ok(&["fsck", "--strict"]);
}

#[test]
fn interrupted_index_publish_removes_the_lock() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "content\n");

    env.cvx()
        .env("CVX_FAILPOINT", "index.before-publish")
        .args(["add", "."])
        .assert()
        .failure()
        .code(128)
        .stderr(predicate::str::contains("injected failure"));

    assert!(!env.path(".cvx/index.lock").exists());
    let status = env.cvx().args(["status", "--porcelain"]).output().unwrap();
    assert!(stdout(&status).contains("?? file.txt"));

    env.cvx().args(["add", "."]).assert().success();
    env.cvx()
        .args(["commit", "-m", "after retry"])
        .assert()
        .success();
    env.assert_git_cvx_ok(&["fsck", "--strict"]);
}

#[test]
fn interrupted_ref_publish_removes_the_lock_and_can_retry() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "content\n");
    env.cvx().args(["add", "."]).assert().success();

    env.cvx()
        .env("CVX_FAILPOINT", "ref.before-publish")
        .args(["commit", "-m", "interrupted"])
        .assert()
        .failure()
        .code(128)
        .stderr(predicate::str::contains("injected failure"));

    assert!(!env.path(".cvx/refs/heads/main.lock").exists());
    assert!(!env.path(".cvx/refs/heads/main").exists());

    env.cvx().args(["commit", "-m", "retry"]).assert().success();
    env.assert_git_cvx_ok(&["fsck", "--strict"]);
    assert!(stdout(&env.cvx().args(["log", "--oneline"]).output().unwrap()).contains("retry"));
}

#[test]
fn orphan_pack_without_index_is_ignored() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "content\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx()
        .args(["commit", "-m", "source"])
        .assert()
        .success();

    let destination = env.path("..").join("interrupted-clone");
    env.cvx()
        .env("CVX_FAILPOINT", "pack.before-index")
        .args(["clone", ".", destination.to_str().expect("utf8 path")])
        .assert()
        .failure()
        .code(128)
        .stderr(predicate::str::contains("injected failure"));

    let pack_dir = destination.join(".cvx/objects/pack");
    let entries: Vec<_> = std::fs::read_dir(&pack_dir)
        .expect("pack dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .collect();
    assert!(
        entries
            .iter()
            .any(|path| path.extension().is_some_and(|ext| ext == "pack"))
    );
    assert!(
        !entries
            .iter()
            .any(|path| path.extension().is_some_and(|ext| ext == "idx"))
    );

    let status = env
        .cvx_in(&destination)
        .args(["status", "--porcelain"])
        .output()
        .expect("status");
    assert_eq!(normalize(&status), "");
}
