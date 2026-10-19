mod common;

use assert_cmd::prelude::*;
use common::{TestEnv, normalize, stdout};
use predicates::prelude::*;

fn commit(env: &TestEnv, message: &str) {
    env.cvx().args(["commit", "-m", message]).assert().success();
}

fn git_cvx_in(env: &TestEnv, directory: &std::path::Path, args: &[&str]) -> std::process::Output {
    let mut full = vec!["--git-dir=.cvx", "--work-tree=."];
    full.extend_from_slice(args);
    let output = env.git_in(directory, &full);
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn clone_copies_history_refs_tags_and_worktree() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "first\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "first");
    env.cvx().args(["branch", "feature"]).assert().success();
    env.cvx()
        .args(["tag", "-a", "v1", "-m", "release"])
        .assert()
        .success();

    let destination = env.path("..").join("clone");
    env.cvx()
        .args(["clone", ".", destination.to_str().expect("utf8 path")])
        .assert()
        .success()
        .stdout(predicate::str::contains("Checked out branch 'main'"));

    assert_eq!(
        std::fs::read_to_string(destination.join("file.txt")).expect("file"),
        "first\n"
    );
    let status = env
        .cvx_in(&destination)
        .args(["status", "--porcelain"])
        .output()
        .expect("status");
    assert_eq!(normalize(&status), "");

    let log = env
        .cvx_in(&destination)
        .args(["log", "--oneline", "--decorate"])
        .output()
        .expect("log");
    let log = stdout(&log);
    assert!(log.contains("HEAD -> main"));
    assert!(log.contains("tag: v1"));

    let feature = env
        .cvx_in(&destination)
        .args(["log", "--oneline", "origin/feature"])
        .output()
        .expect("remote branch");
    assert!(stdout(&feature).contains("first"));

    let remotes = env
        .cvx_in(&destination)
        .args(["remote"])
        .output()
        .expect("remote");
    assert!(stdout(&remotes).contains("origin"));
    git_cvx_in(&env, &destination, &["fsck", "--strict"]);
}

#[test]
fn clone_reads_a_packed_git_source_and_can_use_git_storage() {
    let env = TestEnv::new();
    env.assert_git_ok(&["init", "-q", "-b", "main"]);
    env.write("file.txt", "packed\n");
    env.assert_git_ok(&["add", "."]);
    env.assert_git_ok(&["commit", "-q", "-m", "packed source"]);
    env.assert_git_ok(&["gc", "-q", "--aggressive"]);

    let destination = env.path("..").join("packed-clone");
    env.cvx()
        .args([
            "clone",
            "--storage",
            "git",
            ".",
            destination.to_str().expect("utf8 path"),
        ])
        .assert()
        .success();

    assert!(destination.join(".git").is_dir());
    assert_eq!(
        std::fs::read_to_string(destination.join("file.txt")).expect("file"),
        "packed\n"
    );
    let log = env
        .cvx_in(&destination)
        .args(["log", "--oneline"])
        .output()
        .expect("log");
    assert!(stdout(&log).contains("packed source"));
    let status = env.git_in(&destination, &["status", "--porcelain=v1"]);
    assert_eq!(normalize(&status), "");
    env.git_in(&destination, &["fsck", "--strict"]);
}

#[test]
fn fetch_copies_new_objects_and_updates_tracking_refs() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "first\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "first");

    let destination = env.path("..").join("clone");
    env.cvx()
        .args(["clone", ".", destination.to_str().expect("utf8 path")])
        .assert()
        .success();

    env.write("file.txt", "second\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "second");

    env.cvx_in(&destination)
        .args(["fetch", "origin"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Fetched"));

    let tracking = env
        .cvx_in(&destination)
        .args(["log", "--oneline", "origin/main"])
        .output()
        .expect("log");
    assert!(stdout(&tracking).contains("second"));

    env.cvx_in(&destination)
        .args(["merge", "origin/main"])
        .assert()
        .success();
    assert_eq!(
        std::fs::read_to_string(destination.join("file.txt")).expect("file"),
        "second\n"
    );
    git_cvx_in(&env, &destination, &["fsck", "--strict"]);
}

#[test]
fn push_updates_the_remote_branch_and_worktree() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "first\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "first");

    let destination = env.path("..").join("clone");
    env.cvx()
        .args(["clone", ".", destination.to_str().expect("utf8 path")])
        .assert()
        .success();

    std::fs::write(destination.join("file.txt"), "pushed\n").expect("write");
    env.cvx_in(&destination)
        .args(["add", "."])
        .assert()
        .success();
    env.cvx_in(&destination)
        .args(["commit", "-m", "pushed"])
        .assert()
        .success();
    env.cvx_in(&destination)
        .args(["push", "origin", "main"])
        .assert()
        .success()
        .stdout(predicate::str::contains("objects"));

    assert_eq!(env.read("file.txt"), "pushed\n");
    let status = env.cvx().args(["status", "--porcelain"]).output().unwrap();
    assert_eq!(normalize(&status), "");
    let log = env.assert_git_cvx_ok(&["log", "--format=%s", "-n", "1"]);
    assert!(stdout(&log).contains("pushed"));
    env.assert_git_cvx_ok(&["fsck", "--strict"]);
}

#[test]
fn push_rejects_non_fast_forward_without_force() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "first\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "first");

    let destination = env.path("..").join("clone");
    env.cvx()
        .args(["clone", ".", destination.to_str().expect("utf8 path")])
        .assert()
        .success();

    env.write("file.txt", "remote\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "remote");

    std::fs::write(destination.join("other.txt"), "local\n").expect("write");
    env.cvx_in(&destination)
        .args(["add", "."])
        .assert()
        .success();
    env.cvx_in(&destination)
        .args(["commit", "-m", "local"])
        .assert()
        .success();

    env.cvx_in(&destination)
        .args(["push", "origin", "main"])
        .assert()
        .failure()
        .code(128)
        .stderr(predicate::str::contains("not a fast-forward"));

    env.cvx_in(&destination)
        .args(["push", "--force", "origin", "main"])
        .assert()
        .success();
    assert_eq!(env.read("file.txt"), "first\n");
    assert_eq!(env.read("other.txt"), "local\n");
    env.assert_git_cvx_ok(&["fsck", "--strict"]);
}

#[test]
fn remote_add_configures_a_local_remote() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "content\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "first");

    let other = env.path("..").join("other");
    std::fs::create_dir(&other).expect("other");
    env.cvx_in(&other).args(["init"]).assert().success();

    env.cvx()
        .args(["remote", "add", "upstream", other.to_str().expect("utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("Added remote 'upstream'"));

    let remotes = env
        .cvx()
        .args(["remote", "list"])
        .output()
        .expect("remote list");
    let remotes = stdout(&remotes);
    assert!(remotes.contains("upstream"));
    assert!(remotes.contains("other"));
}
