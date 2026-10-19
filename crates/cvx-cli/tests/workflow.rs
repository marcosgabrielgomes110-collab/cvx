mod common;

use assert_cmd::prelude::*;
use common::{TestEnv, normalize, stdout};
use predicates::prelude::*;

#[test]
fn full_workflow_is_readable_by_git() {
    let env = TestEnv::new();

    env.cvx().args(["init"]).assert().success();
    env.write("hello.txt", "hello\n");
    env.write("src/main.rs", "fn main() {}\n");
    env.write(".gitignore", "*.log\n");
    env.write("debug.log", "ignored\n");

    env.cvx().args(["add", "."]).assert().success();

    let cvx_status = env
        .cvx()
        .args(["status", "--porcelain", "--untracked-files", "all"])
        .output()
        .expect("cvx status");
    let git_status = env.git_cvx(&[
        "-c",
        "status.renames=false",
        "status",
        "--porcelain=v1",
        "-uall",
    ]);
    assert_eq!(normalize(&cvx_status), normalize(&git_status));

    env.cvx()
        .args(["commit", "-m", "initial commit"])
        .assert()
        .success()
        .stdout(predicate::str::contains("initial commit"));

    env.assert_git_cvx_ok(&["fsck", "--strict"]);
    let log = env.assert_git_cvx_ok(&["log", "--format=%s"]);
    assert!(stdout(&log).contains("initial commit"));

    let index = env.assert_git_cvx_ok(&["ls-files", "--stage"]);
    let index = stdout(&index);
    assert!(index.contains("100644"));
    assert!(index.contains("hello.txt"));
    assert!(index.contains("src/main.rs"));
    assert!(!index.contains("debug.log"));

    let status = env.assert_git_cvx_ok(&["status", "--porcelain=v1"]);
    assert_eq!(stdout(&status), "");

    let log = env
        .cvx()
        .args(["log", "--oneline"])
        .output()
        .expect("cvx log");
    assert!(stdout(&log).contains("initial commit"));
}

#[test]
fn status_matches_git_for_modifications_and_untracked_files() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("tracked.txt", "tracked\n");
    env.write("deleted.txt", "deleted\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx()
        .args(["commit", "-m", "initial"])
        .assert()
        .success();

    env.write("tracked.txt", "modified\n");
    env.remove("deleted.txt");
    env.write("untracked/a.txt", "a\n");
    env.write("untracked/deep/b.txt", "b\n");

    let cvx_status = env
        .cvx()
        .args(["status", "--porcelain", "--untracked-files", "all"])
        .output()
        .expect("cvx status");
    let git_status = env.git_cvx(&[
        "-c",
        "status.renames=false",
        "status",
        "--porcelain=v1",
        "-uall",
    ]);
    assert_eq!(normalize(&cvx_status), normalize(&git_status));

    let collapsed = env
        .cvx()
        .args(["status", "--porcelain"])
        .output()
        .expect("cvx status");
    assert!(stdout(&collapsed).contains("?? untracked/"));

    let no_untracked = env
        .cvx()
        .args(["status", "--porcelain", "--untracked-files", "no"])
        .output()
        .expect("cvx status");
    assert!(!stdout(&no_untracked).contains("untracked"));
}

#[test]
fn add_all_without_pathspec_stages_deletions() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("keep.txt", "keep\n");
    env.write("remove.txt", "remove\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx()
        .args(["commit", "-m", "initial"])
        .assert()
        .success();

    env.remove("remove.txt");
    env.cvx().args(["add", "-A"]).assert().success();

    let cvx_status = env
        .cvx()
        .args(["status", "--porcelain"])
        .output()
        .expect("cvx status");
    let git_status = env.git_cvx(&["-c", "status.renames=false", "status", "--porcelain=v1"]);
    assert_eq!(normalize(&cvx_status), normalize(&git_status));
    assert!(stdout(&cvx_status).contains("D  remove.txt"));

    env.cvx()
        .args(["commit", "-m", "remove"])
        .assert()
        .success();
    env.assert_git_cvx_ok(&["fsck", "--strict"]);
    let log = env.assert_git_cvx_ok(&["log", "--format=%s"]);
    assert!(stdout(&log).contains("remove"));
}

#[test]
fn operates_on_an_existing_git_repository() {
    let env = TestEnv::new();
    env.assert_git_ok(&["init", "-q", "-b", "main"]);
    env.write("git.txt", "from git\n");
    env.assert_git_ok(&["add", "."]);
    env.assert_git_ok(&["commit", "-q", "-m", "git commit"]);

    let status = env
        .cvx()
        .args(["status", "--porcelain"])
        .output()
        .expect("cvx status");
    assert_eq!(normalize(&status), "");

    let log = env
        .cvx()
        .args(["log", "--oneline"])
        .output()
        .expect("cvx log");
    assert!(stdout(&log).contains("git commit"));

    env.write("git.txt", "changed by cvx\n");
    env.write("new/from-cvx.txt", "new\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx()
        .args(["commit", "-m", "cvx commit"])
        .assert()
        .success();

    env.assert_git_ok(&["fsck", "--strict"]);
    let log = env.assert_git_ok(&["log", "--format=%s"]);
    assert!(stdout(&log).contains("cvx commit"));
    assert!(stdout(&log).contains("git commit"));
}

#[test]
fn reads_a_packed_git_repository() {
    let env = TestEnv::new();
    env.assert_git_ok(&["init", "-q", "-b", "main"]);
    for index in 0..10 {
        env.write(
            &format!("file-{index:02}.txt"),
            &format!("content {index}\nshared shared shared\n"),
        );
    }
    env.assert_git_ok(&["add", "."]);
    env.assert_git_ok(&["commit", "-q", "-m", "packed commit"]);
    env.assert_git_ok(&["gc", "-q", "--aggressive"]);

    let status = env
        .cvx()
        .args(["status", "--porcelain"])
        .output()
        .expect("cvx status");
    assert_eq!(normalize(&status), "");

    let log = env
        .cvx()
        .args(["log", "--oneline"])
        .output()
        .expect("cvx log");
    assert!(stdout(&log).contains("packed commit"));

    env.write("file-00.txt", "changed after packing\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx()
        .args(["commit", "-m", "after pack"])
        .assert()
        .success();
    env.assert_git_ok(&["fsck", "--strict"]);
}

#[test]
fn reads_and_rewrites_a_version_four_index() {
    let env = TestEnv::new();
    env.assert_git_ok(&["init", "-q", "-b", "main"]);
    env.write("src/a.txt", "a\n");
    env.write("src/deep/b.txt", "b\n");
    env.assert_git_ok(&["add", "."]);
    env.assert_git_ok(&["update-index", "--index-version", "4"]);
    env.assert_git_ok(&["commit", "-q", "-m", "index v4"]);

    env.write("src/a.txt", "changed\n");
    let status = env
        .cvx()
        .args(["status", "--porcelain"])
        .output()
        .expect("cvx status");
    assert!(stdout(&status).contains("src/a.txt"));

    env.cvx().args(["add", "src/a.txt"]).assert().success();
    env.cvx()
        .args(["commit", "-m", "from v4"])
        .assert()
        .success();
    env.assert_git_ok(&["fsck", "--strict"]);

    let version = env.assert_git_ok(&["update-index", "--show-index-version"]);
    assert_eq!(stdout(&version).trim(), "2");
}

#[test]
fn resolves_annotated_tags() {
    let env = TestEnv::new();
    env.assert_git_ok(&["init", "-q", "-b", "main"]);
    env.write("file.txt", "content\n");
    env.assert_git_ok(&["add", "."]);
    env.assert_git_ok(&["commit", "-q", "-m", "tagged"]);
    env.assert_git_ok(&["tag", "-a", "v1", "-m", "release"]);

    let log = env
        .cvx()
        .args(["log", "--oneline", "v1"])
        .output()
        .expect("cvx log");
    assert!(stdout(&log).contains("tagged"));
}

#[test]
fn commit_without_changes_exits_one_and_allow_empty_works() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "content\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx().args(["commit", "-m", "first"]).assert().success();

    env.cvx()
        .args(["commit", "-m", "again"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("nothing to commit"));

    env.cvx()
        .args(["commit", "--allow-empty", "-m", "empty"])
        .assert()
        .success();

    let log = env.assert_git_cvx_ok(&["rev-list", "--count", "HEAD"]);
    assert_eq!(stdout(&log).trim(), "2");
}

#[test]
fn add_rejects_nested_repositories() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("nested/file.txt", "nested\n");
    env.assert_git_ok(&["init", "-q", "nested"]);

    env.cvx()
        .args(["add", "."])
        .assert()
        .failure()
        .code(128)
        .stderr(predicate::str::contains("nested Git repositories"));
}

#[test]
fn add_rejects_unsupported_crlf_normalization() {
    let env =
        TestEnv::with_global_config("[init]\n\tdefaultBranch = main\n[core]\n\tautocrlf = true\n");
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "content\n");

    env.cvx()
        .args(["add", "."])
        .assert()
        .failure()
        .code(128)
        .stderr(predicate::str::contains("core.autocrlf"));
}

#[test]
fn commit_requires_an_identity() {
    let env = TestEnv::with_global_config("[init]\n\tdefaultBranch = main\n");
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "content\n");
    env.cvx().args(["add", "."]).assert().success();

    env.cvx()
        .args(["commit", "-m", "no identity"])
        .assert()
        .failure()
        .code(128)
        .stderr(predicate::str::contains("author identity unknown"));
}

#[cfg(unix)]
#[test]
fn tracks_executable_files_and_symlinks() {
    use std::os::unix::fs::PermissionsExt;

    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("script.sh", "#!/bin/sh\n");
    let script = env.path("script.sh");
    let mut permissions = std::fs::metadata(&script).expect("metadata").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).expect("chmod");
    std::os::unix::fs::symlink("script.sh", env.path("link")).expect("symlink");

    env.cvx().args(["add", "."]).assert().success();
    env.cvx().args(["commit", "-m", "modes"]).assert().success();

    let index = env.assert_git_cvx_ok(&["ls-files", "--stage"]);
    let index = stdout(&index);
    assert!(index.contains("100755") && index.contains("script.sh"));
    assert!(index.contains("120000") && index.contains("link"));
    env.assert_git_cvx_ok(&["fsck", "--strict"]);
}

#[test]
fn add_from_subdirectory_uses_repository_root() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("src/a.txt", "a\n");
    env.write("other/b.txt", "b\n");

    env.cvx()
        .current_dir(env.path("src"))
        .args(["add", "a.txt"])
        .assert()
        .success();
    env.cvx()
        .args(["commit", "-m", "subdir"])
        .assert()
        .success();

    let files = env.assert_git_cvx_ok(&["ls-files"]);
    let files = stdout(&files);
    assert!(files.contains("src/a.txt"));
    assert!(!files.contains("other/b.txt"));

    let status = env
        .cvx()
        .args(["status", "--porcelain"])
        .output()
        .expect("cvx status");
    assert!(stdout(&status).contains("?? other/"));
}

#[test]
fn add_update_only_stages_tracked_files() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("tracked.txt", "one\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx()
        .args(["commit", "-m", "initial"])
        .assert()
        .success();

    env.write("tracked.txt", "two\n");
    env.write("new.txt", "new\n");
    env.cvx().args(["add", "-u"]).assert().success();

    let status = env
        .cvx()
        .args(["status", "--porcelain"])
        .output()
        .expect("cvx status");
    let status = stdout(&status);
    assert!(status.contains("M  tracked.txt"));
    assert!(status.contains("?? new.txt"));
}

#[test]
fn stages_deletions_with_update() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("tracked.txt", "one\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx()
        .args(["commit", "-m", "initial"])
        .assert()
        .success();

    env.remove("tracked.txt");
    env.cvx().args(["add", "-u"]).assert().success();

    let status = env
        .cvx()
        .args(["status", "--porcelain"])
        .output()
        .expect("cvx status");
    assert!(stdout(&status).contains("D  tracked.txt"));
}

#[test]
fn commits_on_a_detached_head() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "one\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx().args(["commit", "-m", "first"]).assert().success();

    let first = stdout(&env.assert_git_cvx_ok(&["rev-parse", "HEAD"]))
        .trim()
        .to_owned();
    std::fs::write(env.path(".cvx/HEAD"), format!("{first}\n")).expect("detach");

    env.write("file.txt", "two\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx()
        .args(["commit", "-m", "detached"])
        .assert()
        .success();

    let head = stdout(&env.assert_git_cvx_ok(&["rev-parse", "HEAD"]))
        .trim()
        .to_owned();
    let branch = stdout(&env.assert_git_cvx_ok(&["rev-parse", "main"]))
        .trim()
        .to_owned();
    assert_ne!(head, first);
    assert_eq!(branch, first);
    env.assert_git_cvx_ok(&["fsck", "--strict"]);

    let reflog = env.read(".cvx/logs/HEAD");
    assert!(reflog.contains("detached"));
}

#[test]
fn works_with_packed_refs() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "one\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx().args(["commit", "-m", "first"]).assert().success();

    env.assert_git_cvx_ok(&["pack-refs", "--all"]);
    assert!(!env.path(".cvx/refs/heads/main").exists());

    let log = env
        .cvx()
        .args(["log", "--oneline"])
        .output()
        .expect("cvx log");
    assert!(stdout(&log).contains("first"));

    env.write("file.txt", "two\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx()
        .args(["commit", "-m", "second"])
        .assert()
        .success();
    env.assert_git_cvx_ok(&["fsck", "--strict"]);
}

#[test]
fn diff_matches_git_for_worktree_and_index() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("a.txt", "one\ntwo\nthree\n");
    env.write("b.txt", "keep\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx()
        .args(["commit", "-m", "initial"])
        .assert()
        .success();

    env.write("a.txt", "one\nCHANGED\nthree\n");
    env.write("new.txt", "new\n");
    env.cvx().args(["add", "new.txt"]).assert().success();
    env.remove("b.txt");

    let cvx_diff = env.cvx().args(["diff"]).output().expect("cvx diff");
    let git_diff = env.git_cvx(&["diff", "--no-ext-diff", "--no-color"]);
    assert_eq!(normalize(&cvx_diff), normalize(&git_diff));

    let cvx_cached = env
        .cvx()
        .args(["diff", "--cached"])
        .output()
        .expect("cvx diff --cached");
    let git_cached = env.git_cvx(&["diff", "--cached", "--no-ext-diff", "--no-color"]);
    assert_eq!(normalize(&cvx_cached), normalize(&git_cached));

    let stat = env
        .cvx()
        .args(["diff", "--stat"])
        .output()
        .expect("cvx diff --stat");
    assert!(stdout(&stat).contains("a.txt"));
    assert!(stdout(&stat).contains("files changed"));
}

#[test]
fn restore_and_checkout_paths() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "one\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx()
        .args(["commit", "-m", "initial"])
        .assert()
        .success();

    env.write("file.txt", "changed\n");
    env.cvx().args(["restore", "file.txt"]).assert().success();
    assert_eq!(env.read("file.txt"), "one\n");
    assert_eq!(
        normalize(&env.cvx().args(["status", "--porcelain"]).output().unwrap()),
        ""
    );

    env.write("file.txt", "staged\n");
    env.cvx().args(["add", "file.txt"]).assert().success();
    env.cvx()
        .args(["restore", "--staged", "file.txt"])
        .assert()
        .success();
    let status = env
        .cvx()
        .args(["status", "--porcelain"])
        .output()
        .expect("status");
    assert!(stdout(&status).contains(" M file.txt"));

    env.cvx()
        .args(["checkout", "--", "file.txt"])
        .assert()
        .success();
    assert_eq!(env.read("file.txt"), "one\n");
    assert_eq!(
        normalize(&env.cvx().args(["status", "--porcelain"]).output().unwrap()),
        ""
    );

    env.remove("file.txt");
    env.cvx()
        .args([
            "restore",
            "--source",
            "HEAD",
            "--staged",
            "--worktree",
            "file.txt",
        ])
        .assert()
        .success();
    assert_eq!(env.read("file.txt"), "one\n");
    env.assert_git_cvx_ok(&["fsck", "--strict"]);
}

#[test]
fn restore_refuses_to_overwrite_untracked_file() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "one\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx()
        .args(["commit", "-m", "initial"])
        .assert()
        .success();

    env.remove("file.txt");
    env.cvx().args(["add", "-A"]).assert().success();
    env.write("file.txt", "untracked\n");

    env.cvx()
        .args(["restore", "--source", "HEAD", "--worktree", "file.txt"])
        .assert()
        .failure()
        .code(128)
        .stderr(predicate::str::contains("untracked file"));

    env.cvx()
        .args([
            "restore",
            "--source",
            "HEAD",
            "--worktree",
            "--force",
            "file.txt",
        ])
        .assert()
        .success();
    assert_eq!(env.read("file.txt"), "one\n");
}

#[test]
fn branch_and_switch_workflow() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "main\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx().args(["commit", "-m", "main"]).assert().success();

    env.cvx().args(["branch", "feature"]).assert().success();
    let list = env.cvx().args(["branch"]).output().expect("branch list");
    let list = stdout(&list);
    assert!(list.contains("* main"));
    assert!(list.contains("feature"));

    env.cvx()
        .args(["switch", "feature"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Switched to branch 'feature'"));
    assert!(stdout(&env.cvx().args(["branch"]).output().unwrap()).contains("* feature"));

    env.write("file.txt", "feature\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx()
        .args(["commit", "-m", "feature"])
        .assert()
        .success();

    env.cvx().args(["switch", "main"]).assert().success();
    assert_eq!(env.read("file.txt"), "main\n");

    env.cvx()
        .args(["branch", "-d", "feature"])
        .assert()
        .failure()
        .code(128)
        .stderr(predicate::str::contains("not fully merged"));
    env.cvx()
        .args(["branch", "-D", "feature"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Deleted branch feature"));
    assert!(!stdout(&env.cvx().args(["branch"]).output().unwrap()).contains("feature"));

    env.assert_git_cvx_ok(&["fsck", "--strict"]);
}

#[test]
fn switch_creates_branches_and_detaches_head() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "content\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx()
        .args(["commit", "-m", "initial"])
        .assert()
        .success();

    env.cvx()
        .args(["switch", "-c", "work"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Switched to a new branch 'work'"));
    assert!(stdout(&env.cvx().args(["branch"]).output().unwrap()).contains("* work"));

    env.cvx()
        .args(["branch", "-d", "work"])
        .assert()
        .failure()
        .code(128)
        .stderr(predicate::str::contains("currently checked out"));

    env.cvx()
        .args(["switch", "--detach", "HEAD"])
        .assert()
        .success()
        .stdout(predicate::str::contains("HEAD is now at"));
    let status = env.cvx().args(["status"]).output().expect("status");
    assert!(stdout(&status).contains("HEAD detached"));

    env.assert_git_cvx_ok(&["fsck", "--strict"]);
}

#[test]
fn switch_refuses_dirty_worktree_and_untracked_collisions() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "main\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx().args(["commit", "-m", "main"]).assert().success();

    env.cvx().args(["switch", "-c", "other"]).assert().success();
    env.write("file.txt", "other\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx().args(["commit", "-m", "other"]).assert().success();
    env.cvx().args(["switch", "main"]).assert().success();

    env.write("file.txt", "dirty\n");
    env.cvx()
        .args(["switch", "other"])
        .assert()
        .failure()
        .code(128)
        .stderr(predicate::str::contains("local changes"));
    env.cvx()
        .args(["switch", "--force", "other"])
        .assert()
        .success();
    assert_eq!(env.read("file.txt"), "other\n");

    env.cvx().args(["switch", "main"]).assert().success();
    env.cvx()
        .args(["switch", "-c", "collision"])
        .assert()
        .success();
    env.write("collision.txt", "tracked\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx()
        .args(["commit", "-m", "collision"])
        .assert()
        .success();
    env.cvx().args(["switch", "main"]).assert().success();

    env.write("collision.txt", "untracked\n");
    env.cvx()
        .args(["switch", "collision"])
        .assert()
        .failure()
        .code(128)
        .stderr(predicate::str::contains("untracked file"));
    env.remove("collision.txt");
    env.cvx().args(["switch", "collision"]).assert().success();
    assert_eq!(env.read("collision.txt"), "tracked\n");
}

#[test]
fn log_limit_returns_the_newest_commit() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "one\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx().args(["commit", "-m", "first"]).assert().success();
    env.write("file.txt", "two\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx()
        .args(["commit", "-m", "second"])
        .assert()
        .success();

    let log = env
        .cvx()
        .args(["log", "--oneline", "-n", "1"])
        .output()
        .expect("cvx log");
    let log = stdout(&log);
    assert!(log.contains("second"));
    assert!(!log.contains("first"));
}

#[test]
fn help_lists_only_implemented_commands() {
    let env = TestEnv::new();
    let output = env.cvx().args(["--help"]).output().expect("help");
    let text = stdout(&output);
    assert!(text.contains("init"));
    assert!(text.contains("add"));
    assert!(text.contains("commit"));
    assert!(text.contains("status"));
    assert!(text.contains("log"));
    assert!(text.contains("diff"));
    assert!(text.contains("restore"));
    assert!(text.contains("checkout"));
    assert!(text.contains("branch"));
    assert!(text.contains("switch"));
    assert!(!text.contains("clone"));
    assert!(!text.contains("push"));
}
