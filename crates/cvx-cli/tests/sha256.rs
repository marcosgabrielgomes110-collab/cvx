mod common;

use assert_cmd::prelude::*;
use common::{TestEnv, normalize, stdout};

#[test]
fn sha256_repository_supports_local_workflow_and_clone() {
    let env = TestEnv::new();
    env.cvx()
        .args(["init", "--object-format", "sha256"])
        .assert()
        .success();
    env.write("file.txt", "sha256 content\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx()
        .args(["commit", "-m", "sha256 initial"])
        .assert()
        .success();
    env.cvx()
        .args(["tag", "-a", "v1", "-m", "release"])
        .assert()
        .success();
    env.cvx().args(["branch", "feature"]).assert().success();

    let log = env.cvx().args(["log", "--oneline"]).output().unwrap();
    let log = stdout(&log);
    assert!(log.contains("sha256 initial"));
    let oid = log.split_whitespace().next().expect("short oid");
    assert_eq!(oid.len(), 7);

    let full = stdout(&env.assert_git_cvx_ok(&["rev-parse", "HEAD"]));
    assert_eq!(full.trim().len(), 64);

    let fsck = env.assert_git_cvx_ok(&["fsck", "--strict"]);
    assert!(!String::from_utf8_lossy(&fsck.stderr).contains("error"));
    let git_log = env.assert_git_cvx_ok(&["log", "--format=%s"]);
    assert!(stdout(&git_log).contains("sha256 initial"));
    let git_status = env.assert_git_cvx_ok(&["status", "--porcelain=v1"]);
    assert_eq!(stdout(&git_status), "");

    let destination = env.path("..").join("sha256-clone");
    env.cvx()
        .args(["clone", ".", destination.to_str().expect("utf8")])
        .assert()
        .success();
    let status = env
        .cvx_in(&destination)
        .args(["status", "--porcelain"])
        .output()
        .expect("status");
    assert_eq!(normalize(&status), "");
    let clone_log = env
        .cvx_in(&destination)
        .args(["log", "--oneline", "--decorate"])
        .output()
        .expect("log");
    assert!(stdout(&clone_log).contains("tag: v1"));
    env.git_in(
        &destination,
        &["--git-dir=.cvx", "--work-tree=.", "fsck", "--strict"],
    );
}
