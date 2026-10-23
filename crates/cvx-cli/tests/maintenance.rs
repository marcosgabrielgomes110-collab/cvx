mod common;

use assert_cmd::prelude::*;
use common::{TestEnv, normalize, stderr, stdout};
use predicates::prelude::*;

fn init_with_two_commits(env: &TestEnv) {
    env.cvx().arg("init").assert().success();
    env.write("a.txt", "one\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx().args(["commit", "-m", "one"]).assert().success();
    env.write("a.txt", "two\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx().args(["commit", "-m", "two"]).assert().success();
}

#[test]
fn reset_modes_update_head_index_and_worktree() {
    let env = TestEnv::new();
    init_with_two_commits(&env);

    env.cvx()
        .args(["reset", "--soft", "HEAD~1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Reset to"));
    assert_eq!(env.read("a.txt"), "two\n");
    let status = env.cvx().args(["status", "--porcelain"]).output().unwrap();
    assert_eq!(normalize(&status), "M  a.txt");

    env.cvx().args(["reset", "HEAD"]).assert().success();
    assert_eq!(env.read("a.txt"), "two\n");
    let status = env.cvx().args(["status", "--porcelain"]).output().unwrap();
    assert_eq!(stdout(&status).trim_end(), " M a.txt");

    env.cvx()
        .args(["reset", "--hard", "HEAD"])
        .assert()
        .success()
        .stdout(predicate::str::contains("HEAD is now at"));
    assert_eq!(env.read("a.txt"), "one\n");
    let status = env.cvx().args(["status", "--porcelain"]).output().unwrap();
    assert_eq!(normalize(&status), "");

    env.assert_git_cvx_ok(&["fsck", "--strict"]);
}

#[test]
fn fsck_reports_a_missing_referenced_object() {
    let env = TestEnv::new();
    env.cvx().arg("init").assert().success();
    env.write("a.txt", "content\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx()
        .args(["commit", "-m", "initial"])
        .assert()
        .success();

    std::fs::write(
        env.path(".cvx/refs/heads/main"),
        format!("{}\n", "0".repeat(40)),
    )
    .unwrap();

    env.cvx()
        .arg("fsck")
        .assert()
        .failure()
        .stderr(predicate::str::contains("object not found"));
}

#[test]
fn gc_packs_loose_objects_and_prunes_unreachable_ones() {
    let env = TestEnv::new();
    init_with_two_commits(&env);

    let gc = env.cvx().arg("gc").output().unwrap();
    assert!(gc.status.success(), "{}", stderr(&gc));
    assert!(stdout(&gc).contains("Packed"));

    let objects = env.path(".cvx/objects");
    let mut loose_files = 0;
    for directory in std::fs::read_dir(&objects).unwrap() {
        let directory = directory.unwrap();
        if directory.file_name().to_string_lossy().len() == 2 {
            for entry in std::fs::read_dir(directory.path()).unwrap() {
                let entry = entry.unwrap();
                if entry.path().is_file() {
                    loose_files += 1;
                }
            }
        }
    }
    assert_eq!(loose_files, 0);

    let fsck = env.cvx().arg("fsck").output().unwrap();
    assert!(fsck.status.success(), "{}", stderr(&fsck));
    assert!(stdout(&fsck).contains("Repository is healthy"));

    env.cvx()
        .args(["reset", "--soft", "HEAD~1"])
        .assert()
        .success();
    let prune = env.cvx().args(["gc", "--prune"]).output().unwrap();
    assert!(prune.status.success(), "{}", stderr(&prune));
    assert!(stdout(&prune).contains("Pruned"));

    let fsck = env.cvx().arg("fsck").output().unwrap();
    assert!(fsck.status.success(), "{}", stderr(&fsck));
    assert!(stdout(&fsck).contains("Repository is healthy"));
    let packs = std::fs::read_dir(env.path(".cvx/objects/pack"))
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "pack"))
        .count();
    assert_eq!(packs, 1, "gc --prune must leave one compact pack");
    env.assert_git_cvx_ok(&["fsck", "--strict"]);
}

#[test]
fn cherry_pick_and_revert_apply_clean_changes() {
    let env = TestEnv::new();
    env.cvx().arg("init").assert().success();
    env.write("a.txt", "base\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx().args(["commit", "-m", "base"]).assert().success();
    env.cvx().args(["branch", "feature"]).assert().success();
    env.cvx().args(["switch", "feature"]).assert().success();
    env.write("b.txt", "feature\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx()
        .args(["commit", "-m", "feature"])
        .assert()
        .success();
    let feature = stdout(&env.assert_git_cvx_ok(&["rev-parse", "HEAD"]))
        .trim()
        .to_owned();

    env.cvx().args(["switch", "main"]).assert().success();
    env.cvx()
        .args(["cherry-pick", &feature])
        .assert()
        .success()
        .stdout(predicate::str::contains("feature"));
    assert_eq!(env.read("b.txt"), "feature\n");
    let status = env.cvx().args(["status", "--porcelain"]).output().unwrap();
    assert_eq!(normalize(&status), "");

    env.cvx()
        .args(["revert", "HEAD"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Revert"));
    assert!(!env.path("b.txt").exists());
    let status = env.cvx().args(["status", "--porcelain"]).output().unwrap();
    assert_eq!(normalize(&status), "");
    env.assert_git_cvx_ok(&["fsck", "--strict"]);
}

#[test]
fn cherry_pick_conflict_leaves_repository_untouched() {
    let env = TestEnv::new();
    env.cvx().arg("init").assert().success();
    env.write("a.txt", "base\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx().args(["commit", "-m", "base"]).assert().success();
    env.cvx().args(["branch", "feature"]).assert().success();
    env.cvx().args(["switch", "feature"]).assert().success();
    env.write("a.txt", "feature\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx()
        .args(["commit", "-m", "feature"])
        .assert()
        .success();
    let feature = stdout(&env.assert_git_cvx_ok(&["rev-parse", "HEAD"]))
        .trim()
        .to_owned();

    env.cvx().args(["switch", "main"]).assert().success();
    env.write("a.txt", "main\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx().args(["commit", "-m", "main"]).assert().success();
    let before = stdout(&env.assert_git_cvx_ok(&["rev-parse", "HEAD"]))
        .trim()
        .to_owned();

    env.cvx()
        .args(["cherry-pick", &feature])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot cherry-pick cleanly"));
    let after = stdout(&env.assert_git_cvx_ok(&["rev-parse", "HEAD"]))
        .trim()
        .to_owned();
    assert_eq!(before, after);
    assert_eq!(env.read("a.txt"), "main\n");
    let status = env.cvx().args(["status", "--porcelain"]).output().unwrap();
    assert_eq!(normalize(&status), "");
}

#[test]
fn stash_saves_lists_applies_and_drops_changes() {
    let env = TestEnv::new();
    env.cvx().arg("init").assert().success();
    env.write("a.txt", "one\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx().args(["commit", "-m", "base"]).assert().success();

    env.write("a.txt", "two\n");
    env.write("b.txt", "new\n");
    env.cvx().args(["add", "b.txt"]).assert().success();
    env.cvx()
        .args(["stash", "push", "-m", "work in progress"])
        .assert()
        .success()
        .stdout(predicate::str::contains("work in progress"));
    assert_eq!(env.read("a.txt"), "one\n");
    assert!(!env.path("b.txt").exists());
    let status = env.cvx().args(["status", "--porcelain"]).output().unwrap();
    assert_eq!(normalize(&status), "");

    let list = env.cvx().args(["stash", "list"]).output().unwrap();
    assert!(stdout(&list).contains("stash@{0}"));
    assert!(stdout(&list).contains("work in progress"));
    let git_list = env.assert_git_cvx_ok(&["stash", "list"]);
    assert!(stdout(&git_list).contains("work in progress"));
    let git_show = env.assert_git_cvx_ok(&["stash", "show", "--stat"]);
    assert!(stdout(&git_show).contains("a.txt"));

    env.cvx().args(["stash", "apply"]).assert().success();
    assert_eq!(env.read("a.txt"), "two\n");
    assert_eq!(env.read("b.txt"), "new\n");
    let status = env.cvx().args(["status", "--porcelain"]).output().unwrap();
    let status = stdout(&status);
    assert!(status.contains("M a.txt"));
    assert!(status.contains("?? b.txt"));
    let list = env.cvx().args(["stash", "list"]).output().unwrap();
    assert!(stdout(&list).contains("stash@{0}"));

    env.cvx().args(["stash", "drop"]).assert().success();
    let list = env.cvx().args(["stash", "list"]).output().unwrap();
    assert_eq!(normalize(&list), "");
    env.assert_git_cvx_ok(&["fsck", "--strict"]);
}

#[test]
fn stash_pop_restores_changes_and_no_changes_is_an_error() {
    let env = TestEnv::new();
    env.cvx().arg("init").assert().success();
    env.write("a.txt", "one\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx().args(["commit", "-m", "base"]).assert().success();

    env.cvx()
        .args(["stash", "push"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no local changes"));

    env.write("a.txt", "two\n");
    env.cvx()
        .args(["stash", "push", "-m", "first"])
        .assert()
        .success();
    env.write("a.txt", "three\n");
    env.cvx()
        .args(["stash", "push", "-m", "second"])
        .assert()
        .success();
    let list = env.cvx().args(["stash", "list"]).output().unwrap();
    let list = stdout(&list);
    let first = list.find("second").unwrap();
    let second = list.find("first").unwrap();
    assert!(first < second, "newest stash must be listed first: {list}");

    env.cvx().args(["stash", "pop"]).assert().success();
    assert_eq!(env.read("a.txt"), "three\n");
    let list = env.cvx().args(["stash", "list"]).output().unwrap();
    assert!(stdout(&list).contains("first"));
    assert!(!stdout(&list).contains("second"));
}

#[test]
fn reset_pathspec_updates_only_selected_index_entries() {
    let env = TestEnv::new();
    env.cvx().arg("init").assert().success();
    env.write("a.txt", "one\n");
    env.write("b.txt", "one\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx().args(["commit", "-m", "base"]).assert().success();
    env.write("a.txt", "two\n");
    env.write("b.txt", "two\n");
    env.cvx().args(["add", "."]).assert().success();

    env.cvx()
        .args(["reset", "--", "a.txt"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Reset 1 index paths"));
    let status = env.cvx().args(["status", "--porcelain"]).output().unwrap();
    assert_eq!(stdout(&status).trim_end(), " M a.txt\nM  b.txt");

    env.cvx()
        .args(["reset", "--hard", "--", "a.txt"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("pathspecs"));
    env.cvx()
        .args(["reset", "--", "missing.txt"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("did not match"));
}

#[test]
fn fsck_repair_rebuilds_a_corrupt_or_missing_index() {
    let env = TestEnv::new();
    env.cvx().arg("init").assert().success();
    env.write("a.txt", "content\n");
    env.cvx().args(["add", "."]).assert().success();
    env.cvx().args(["commit", "-m", "base"]).assert().success();

    std::fs::write(env.path(".cvx/index"), b"not an index").unwrap();
    env.cvx().arg("fsck").assert().failure();
    env.cvx()
        .arg("fsck")
        .arg("--repair")
        .assert()
        .success()
        .stdout(predicate::str::contains("Repaired"));
    let status = env.cvx().args(["status", "--porcelain"]).output().unwrap();
    assert_eq!(normalize(&status), "");

    std::fs::remove_file(env.path(".cvx/index")).unwrap();
    env.cvx().arg("fsck").arg("--repair").assert().success();
    let status = env.cvx().args(["status", "--porcelain"]).output().unwrap();
    assert_eq!(normalize(&status), "");
    env.assert_git_cvx_ok(&["fsck", "--strict"]);
}

#[test]
fn fsck_repair_removes_stale_locks() {
    let env = TestEnv::new();
    env.cvx().arg("init").assert().success();
    env.write("a.txt", "content\n");
    env.cvx().args(["add", "."]).assert().success();
    std::fs::write(env.path(".cvx/index.lock"), b"").unwrap();

    env.cvx()
        .env("CVX_STALE_LOCK_AGE_SECS", "0")
        .args(["fsck", "--repair"])
        .assert()
        .success()
        .stdout(predicate::str::contains("removed stale lock"));
    assert!(!env.path(".cvx/index.lock").exists());
}
