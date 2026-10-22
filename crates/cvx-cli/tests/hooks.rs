#![cfg(unix)]

mod common;

use std::os::unix::fs::PermissionsExt;

use assert_cmd::prelude::*;
use common::{TestEnv, stdout};
use predicates::prelude::*;

fn write_hook(env: &TestEnv, name: &str, contents: &str) {
    let path = env.path(".cvx/hooks").join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, contents).unwrap();
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).unwrap();
}

#[test]
fn commit_hooks_can_decline_and_rewrite_the_message() {
    let env = TestEnv::new();
    env.cvx().arg("init").assert().success();
    env.write("file.txt", "content\n");
    env.cvx().args(["add", "."]).assert().success();

    write_hook(&env, "pre-commit", "#!/bin/sh\nexit 1\n");
    env.cvx()
        .args(["commit", "-m", "initial"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("pre-commit hook declined"));
    let head = env.git_cvx(&["rev-parse", "--verify", "HEAD"]);
    assert!(!head.status.success(), "HEAD must remain unborn");

    write_hook(&env, "pre-commit", "#!/bin/sh\nexit 0\n");
    write_hook(
        &env,
        "commit-msg",
        "#!/bin/sh\nprintf 'message from hook\\n' > \"$1\"\n",
    );
    write_hook(
        &env,
        "post-commit",
        "#!/bin/sh\nprintf 'ran\\n' > post-commit.txt\n",
    );
    env.cvx()
        .args(["commit", "-m", "initial"])
        .assert()
        .success();
    let log = env.assert_git_cvx_ok(&["log", "--format=%s"]);
    assert!(stdout(&log).contains("message from hook"));
    assert!(env.path("post-commit.txt").is_file());
}

#[test]
fn pre_push_hook_can_decline_before_objects_are_sent() {
    let env = TestEnv::new();
    env.cvx().arg("init").assert().success();
    env.write("file.txt", "content\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx()
        .args(["commit", "-m", "initial"])
        .assert()
        .success();

    let remote = env.path("..").join("remote.git");
    env.cvx()
        .args(["init", "--bare", remote.to_str().unwrap()])
        .assert()
        .success();
    env.cvx()
        .args(["remote", "add", "origin", remote.to_str().unwrap()])
        .assert()
        .success();

    write_hook(&env, "pre-push", "#!/bin/sh\nexit 1\n");
    env.cvx()
        .args(["push", "origin", "main"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("pre-push hook declined"));

    write_hook(&env, "pre-push", "#!/bin/sh\ncat >/dev/null\nexit 0\n");
    env.cvx()
        .args(["push", "origin", "main"])
        .assert()
        .success();
    let log = env.git_in(
        &env.path(".."),
        &["--git-dir=remote.git", "log", "--oneline"],
    );
    assert!(stdout(&log).contains("initial"));
}
