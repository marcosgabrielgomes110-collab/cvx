mod common;

use assert_cmd::prelude::*;
use common::{TestEnv, normalize, stdout};
use predicates::prelude::*;

#[test]
fn bare_init_has_no_worktree_and_is_git_compatible() {
    let env = TestEnv::new();
    env.cvx()
        .args(["init", "--bare", "bare.git"])
        .assert()
        .success()
        .stdout(predicate::str::contains("bare CVX repository"));

    let bare = env.path("bare.git");
    let check = env.git(&["--git-dir=bare.git", "rev-parse", "--is-bare-repository"]);
    assert_eq!(stdout(&check).trim(), "true");
    env.assert_git_ok(&["--git-dir=bare.git", "fsck", "--strict"]);

    env.cvx_in(&bare).arg("fsck").assert().success();
    env.cvx_in(&bare)
        .arg("status")
        .assert()
        .failure()
        .stderr(predicate::str::contains("must be run in a work tree"));
}

#[test]
fn bare_clone_and_push_work() {
    let env = TestEnv::new();
    env.cvx().arg("init").assert().success();
    env.write("a.txt", "base\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx().args(["commit", "-m", "base"]).assert().success();

    let destination = env.path("..").join("bare.git");
    env.cvx()
        .args(["clone", "--bare", ".", destination.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Cloned bare repository"));
    let parent = env.path("..");
    let git_check = env.git_in(&parent, &["--git-dir=bare.git", "log", "--oneline"]);
    assert!(git_check.status.success());
    let git_fsck = env.git_in(&parent, &["--git-dir=bare.git", "fsck", "--strict"]);
    assert!(git_fsck.status.success());

    let bare = destination.clone();
    let log = env
        .cvx_in(&bare)
        .args(["log", "--oneline"])
        .output()
        .unwrap();
    assert!(stdout(&log).contains("base"));
    env.cvx_in(&bare).args(["gc", "--prune"]).assert().success();
    env.cvx_in(&bare).arg("fsck").assert().success();

    env.write("a.txt", "second\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx()
        .args(["commit", "-m", "second"])
        .assert()
        .success();
    env.cvx()
        .args(["remote", "add", "origin", destination.to_str().unwrap()])
        .assert()
        .success();
    env.cvx()
        .args(["push", "origin", "main"])
        .assert()
        .success();
    let git_fsck = env.git_in(&parent, &["--git-dir=bare.git", "fsck", "--strict"]);
    assert!(git_fsck.status.success());
    let log = env.git_in(&parent, &["--git-dir=bare.git", "log", "--oneline"]);
    assert!(stdout(&log).contains("second"));

    env.cvx().args(["fetch", "origin"]).assert().success();
    let status = env.cvx().args(["status", "--porcelain"]).output().unwrap();
    assert_eq!(normalize(&status), "");
}
