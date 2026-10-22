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

#[test]
fn interrupted_stash_keeps_the_saved_entry_and_can_be_applied_later() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "base\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx().args(["commit", "-m", "base"]).assert().success();
    env.write("file.txt", "changed\n");

    env.cvx()
        .env("CVX_FAILPOINT", "stash.before-reset")
        .args(["stash", "push", "-m", "interrupted"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("injected failure"));
    assert_eq!(env.read("file.txt"), "changed\n");
    let list = env.cvx().args(["stash", "list"]).output().unwrap();
    assert!(stdout(&list).contains("interrupted"));

    env.cvx()
        .args(["reset", "--hard", "HEAD"])
        .assert()
        .success();
    env.cvx().args(["stash", "apply"]).assert().success();
    assert_eq!(env.read("file.txt"), "changed\n");
    env.assert_git_cvx_ok(&["fsck", "--strict"]);
}

#[test]
fn interrupted_hard_reset_can_be_retried() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "one\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx().args(["commit", "-m", "one"]).assert().success();
    env.write("file.txt", "two\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx().args(["commit", "-m", "two"]).assert().success();
    let before = stdout(&env.assert_git_cvx_ok(&["rev-parse", "HEAD"]))
        .trim()
        .to_owned();

    env.cvx()
        .env("CVX_FAILPOINT", "index.before-publish")
        .args(["reset", "--hard", "HEAD~1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("injected failure"));
    let after = stdout(&env.assert_git_cvx_ok(&["rev-parse", "HEAD"]))
        .trim()
        .to_owned();
    assert_eq!(before, after);

    env.cvx()
        .args(["reset", "--hard", "HEAD~1"])
        .assert()
        .success();
    assert_eq!(env.read("file.txt"), "one\n");
    let status = env.cvx().args(["status", "--porcelain"]).output().unwrap();
    assert_eq!(normalize(&status), "");
    env.assert_git_cvx_ok(&["fsck", "--strict"]);
}

#[test]
fn interrupted_gc_before_deletion_keeps_the_repository_usable() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "one\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx().args(["commit", "-m", "one"]).assert().success();

    env.cvx()
        .env("CVX_FAILPOINT", "gc.before-prune")
        .args(["gc", "--prune"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("injected failure"));

    let status = env.cvx().args(["status", "--porcelain"]).output().unwrap();
    assert_eq!(normalize(&status), "");
    env.cvx().arg("fsck").assert().success();
    env.assert_git_cvx_ok(&["fsck", "--strict"]);

    env.cvx().args(["gc", "--prune"]).assert().success();
    env.cvx().arg("fsck").assert().success();
}

#[test]
fn concurrent_add_processes_leave_a_consistent_index() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    for index in 0..20 {
        env.write(&format!("file-{index}.txt"), "content\n");
    }

    let mut first = env.cvx();
    first.args(["add", "."]);
    let mut second = env.cvx();
    second.args(["add", "."]);
    let mut first = first.spawn().expect("spawn first add");
    let mut second = second.spawn().expect("spawn second add");
    let first_status = first.wait().expect("wait first");
    let second_status = second.wait().expect("wait second");
    assert!(first_status.success() || second_status.success());

    let status = env.cvx().args(["status", "--porcelain"]).output().unwrap();
    let output = stdout(&status);
    assert!(output.contains("file-0.txt"));
    env.cvx().arg("fsck").assert().success();
    env.assert_git_cvx_ok(&["fsck", "--strict"]);
}

#[test]
fn index_lock_timeout_reports_stale_lock_diagnostics() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "content\n");
    std::fs::write(env.path(".cvx/index.lock"), b"").unwrap();

    env.cvx()
        .env("CVX_LOCK_TIMEOUT_MS", "100")
        .args(["add", "."])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "remove the lock if no process is active",
        ));

    std::fs::remove_file(env.path(".cvx/index.lock")).unwrap();
    env.cvx().args(["add", "."]).assert().success();
}
