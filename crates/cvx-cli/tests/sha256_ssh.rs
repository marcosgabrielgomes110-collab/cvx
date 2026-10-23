#![cfg(unix)]

mod common;

use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use assert_cmd::prelude::*;
use common::{TestEnv, stdout};

fn run_git(directory: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(directory)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn fake_ssh(directory: &std::path::Path) -> std::path::PathBuf {
    let path = directory.join("fake-ssh");
    std::fs::write(
        &path,
        "#!/usr/bin/env bash\nlast=\"${@: -1}\"\nexec bash -c \"$last\"\n",
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).unwrap();
    path
}

#[test]
#[ignore = "requires a real SSH server with SHA-256 support; GitHub does not support SHA-256 repositories"]
fn sha256_clone_and_push_over_ssh_transport() {
    let env = TestEnv::new();
    let temp = env.path("..");
    let remote = temp.join("ssh-remote.git");
    let work = temp.join("ssh-source");
    run_git(&temp, &["init", "--bare", "--object-format=sha256", "ssh-remote.git"]);
    run_git(&temp, &["init", "--object-format=sha256", "ssh-source"]);
    run_git(&work, &["config", "user.name", "Test User"]);
    run_git(&work, &["config", "user.email", "test@example.com"]);
    std::fs::write(work.join("file.txt"), "sha256 over ssh\n").unwrap();
    run_git(&work, &["add", "."]);
    run_git(&work, &["commit", "-m", "sha256 ssh base"]);
    run_git(&work, &["branch", "-M", "main"]);
    run_git(&work, &["push", remote.to_str().unwrap(), "main"]);

    let ssh = fake_ssh(&temp);
    let url = format!("ssh://git@localhost{}", remote.display());
    let clone = temp.join("ssh-clone");
    env.cvx_in(&temp)
        .env("CVX_SSH", &ssh)
        .args(["clone", &url, clone.to_str().unwrap()])
        .assert()
        .success();
    let log = env.cvx_in(&clone).args(["log", "--oneline"]).output().unwrap();
    assert!(stdout(&log).contains("sha256 ssh base"));
    let fsck = Command::new("git")
        .args(["--git-dir=.cvx", "--work-tree=.", "fsck", "--strict"])
        .current_dir(&clone)
        .output()
        .unwrap();
    assert!(fsck.status.success(), "{}", String::from_utf8_lossy(&fsck.stderr));

    let repo = temp.join("ssh-pusher");
    env.cvx_in(&temp)
        .args(["init", "--object-format", "sha256", "ssh-pusher"])
        .assert()
        .success();
    std::fs::write(repo.join("pushed.txt"), "ssh push\n").unwrap();
    env.cvx_in(&repo).args(["add", "."]).assert().success();
    env.cvx_in(&repo)
        .args(["commit", "-m", "sha256 ssh push"])
        .assert()
        .success();
    env.cvx_in(&repo)
        .env("CVX_SSH", &ssh)
        .args(["remote", "add", "origin", &url])
        .assert()
        .success();
    env.cvx_in(&repo)
        .env("CVX_SSH", &ssh)
        .args(["push", "origin", "main"])
        .assert()
        .success();

    let log = Command::new("git")
        .args(["--git-dir=ssh-remote.git", "log", "main", "--oneline"])
        .current_dir(&temp)
        .output()
        .unwrap();
    assert!(stdout(&log).contains("sha256 ssh push"));
}
