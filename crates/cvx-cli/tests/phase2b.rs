mod common;

use assert_cmd::prelude::*;
use common::{TestEnv, normalize, stdout};
use predicates::prelude::*;

fn commit(env: &TestEnv, message: &str) {
    env.cvx().args(["commit", "-m", message]).assert().success();
}

fn create_branch(env: &TestEnv, branch: &str) {
    env.cvx().args(["branch", branch]).assert().success();
    env.cvx().args(["switch", branch]).assert().success();
}

fn switch(env: &TestEnv, branch: &str) {
    env.cvx().args(["switch", branch]).assert().success();
}

#[test]
fn merge_fast_forwards_a_branch() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "base\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "base");

    create_branch(&env, "feature");
    env.write("file.txt", "feature\n");
    env.write("new.txt", "new\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "feature");
    switch(&env, "main");

    env.cvx()
        .args(["merge", "feature"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Fast-forward"));

    assert_eq!(env.read("file.txt"), "feature\n");
    assert_eq!(env.read("new.txt"), "new\n");
    assert_eq!(
        normalize(&env.cvx().args(["status", "--porcelain"]).output().unwrap()),
        ""
    );
    env.assert_git_cvx_ok(&["fsck", "--strict"]);
}

#[test]
fn merge_creates_a_two_parent_commit() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "base\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "base");

    create_branch(&env, "feature");
    env.write("file.txt", "feature\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "feature");
    switch(&env, "main");
    env.write("other.txt", "main\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "main");

    env.cvx()
        .args(["merge", "feature"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Merge branch 'feature'"));

    assert_eq!(env.read("file.txt"), "feature\n");
    assert_eq!(env.read("other.txt"), "main\n");

    let parents = stdout(&env.assert_git_cvx_ok(&["rev-list", "--parents", "-n", "1", "HEAD"]));
    assert_eq!(parents.split_whitespace().count(), 3);
    env.assert_git_cvx_ok(&["fsck", "--strict"]);
    assert_eq!(
        normalize(&env.cvx().args(["status", "--porcelain"]).output().unwrap()),
        ""
    );
}

#[test]
fn merge_combines_non_overlapping_edits_in_one_file() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "one\ntwo\nthree\nfour\nfive\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "base");

    create_branch(&env, "feature");
    env.write("file.txt", "one\nFEATURE\nthree\nfour\nfive\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "feature edit");
    switch(&env, "main");
    env.write("file.txt", "one\ntwo\nthree\nfour\nMAIN\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "main edit");

    env.cvx().args(["merge", "feature"]).assert().success();
    assert_eq!(env.read("file.txt"), "one\nFEATURE\nthree\nfour\nMAIN\n");
    assert_eq!(
        normalize(&env.cvx().args(["status", "--porcelain"]).output().unwrap()),
        ""
    );
    let parents = stdout(&env.assert_git_cvx_ok(&["rev-list", "--parents", "-n", "1", "HEAD"]));
    assert_eq!(parents.split_whitespace().count(), 3);
    env.assert_git_cvx_ok(&["fsck", "--strict"]);
}

#[test]
fn merge_conflicts_write_markers_and_index_stages() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "one\ntwo\nthree\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "base");

    create_branch(&env, "feature");
    env.write("file.txt", "one\nTHEIRS\nthree\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "theirs");
    switch(&env, "main");
    env.write("file.txt", "one\nOURS\nthree\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "ours");

    env.cvx()
        .args(["merge", "feature"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("Automatic merge failed"));

    let content = env.read("file.txt");
    assert!(content.contains("<<<<<<< HEAD"));
    assert!(content.contains("OURS"));
    assert!(content.contains("======="));
    assert!(content.contains("THEIRS"));
    assert!(content.contains(">>>>>>> feature"));

    let status = env.cvx().args(["status", "--porcelain"]).output().unwrap();
    assert!(stdout(&status).contains("UU file.txt"));

    let stages = stdout(&env.assert_git_cvx_ok(&["ls-files", "--stage", "file.txt"]));
    assert!(stages.lines().any(|line| line.contains(" 1\tfile.txt")));
    assert!(stages.lines().any(|line| line.contains(" 2\tfile.txt")));
    assert!(stages.lines().any(|line| line.contains(" 3\tfile.txt")));

    env.write("file.txt", "one\nRESOLVED\nthree\n");
    env.cvx().args(["add", "file.txt"]).assert().success();
    env.cvx()
        .args(["merge", "--continue"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Merge branch 'feature'"));

    let stages = stdout(&env.assert_git_cvx_ok(&["ls-files", "--stage", "file.txt"]));
    assert_eq!(stages.lines().count(), 1);
    assert!(stages.contains(" 0\tfile.txt"));

    let parents = stdout(&env.assert_git_cvx_ok(&["rev-list", "--parents", "-n", "1", "HEAD"]));
    assert_eq!(parents.split_whitespace().count(), 3);
    assert!(!env.path(".cvx/MERGE_HEAD").exists());
    env.assert_git_cvx_ok(&["fsck", "--strict"]);
}

#[test]
fn commit_concludes_a_merge_after_conflicts() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "one\ntwo\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "base");

    create_branch(&env, "feature");
    env.write("file.txt", "one\nTHEIRS\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "theirs");
    switch(&env, "main");
    env.write("file.txt", "one\nOURS\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "ours");

    env.cvx().args(["merge", "feature"]).assert().failure();
    env.write("file.txt", "one\nMERGED\n");
    env.cvx().args(["add", "file.txt"]).assert().success();
    env.cvx()
        .args(["commit", "-m", "merge resolved"])
        .assert()
        .success();

    let parents = stdout(&env.assert_git_cvx_ok(&["rev-list", "--parents", "-n", "1", "HEAD"]));
    assert_eq!(parents.split_whitespace().count(), 3);
    assert!(!env.path(".cvx/MERGE_HEAD").exists());
    assert!(!env.path(".cvx/MERGE_MSG").exists());
    env.assert_git_cvx_ok(&["fsck", "--strict"]);
}

#[test]
fn directory_file_conflicts_fail_explicitly_without_writing() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("base.txt", "base\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "base");

    create_branch(&env, "feature");
    env.write("path/child.txt", "feature\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "feature directory");
    switch(&env, "main");
    env.write("path", "main file\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "main file");

    env.cvx()
        .args(["merge", "feature"])
        .assert()
        .failure()
        .code(128)
        .stderr(predicate::str::contains("directory/file conflicts"));
    assert!(!env.path(".cvx/MERGE_HEAD").exists());
    assert_eq!(env.read("path"), "main file\n");
    assert_eq!(
        normalize(&env.cvx().args(["status", "--porcelain"]).output().unwrap()),
        ""
    );
}

#[test]
fn binary_conflicts_keep_our_content_and_stage_all_sides() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    std::fs::write(env.path("image.bin"), b"\0base").expect("write");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "base");

    create_branch(&env, "feature");
    std::fs::write(env.path("image.bin"), b"\0theirs").expect("write");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "theirs");
    switch(&env, "main");
    std::fs::write(env.path("image.bin"), b"\0ours").expect("write");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "ours");

    env.cvx().args(["merge", "feature"]).assert().failure();
    assert_eq!(
        std::fs::read(env.path("image.bin")).expect("read"),
        b"\0ours"
    );
    let status = env.cvx().args(["status", "--porcelain"]).output().unwrap();
    assert!(stdout(&status).contains("UU image.bin"));
    let stages = stdout(&env.assert_git_cvx_ok(&["ls-files", "--stage", "image.bin"]));
    assert_eq!(stages.lines().count(), 3);
}

#[test]
fn merge_abort_restores_the_pre_merge_state() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "one\ntwo\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "base");

    create_branch(&env, "feature");
    env.write("file.txt", "one\nTHEIRS\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "theirs");
    switch(&env, "main");
    env.write("file.txt", "one\nOURS\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "ours");

    let before = stdout(&env.assert_git_cvx_ok(&["rev-parse", "HEAD"]))
        .trim()
        .to_owned();
    env.cvx().args(["merge", "feature"]).assert().failure();

    env.cvx()
        .args(["merge", "--abort"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Merge aborted"));

    let after = stdout(&env.assert_git_cvx_ok(&["rev-parse", "HEAD"]))
        .trim()
        .to_owned();
    assert_eq!(before, after);
    assert_eq!(env.read("file.txt"), "one\nOURS\n");
    assert!(!env.path(".cvx/MERGE_HEAD").exists());
    assert_eq!(
        normalize(&env.cvx().args(["status", "--porcelain"]).output().unwrap()),
        ""
    );
    env.assert_git_cvx_ok(&["fsck", "--strict"]);
}

#[test]
fn merge_refuses_to_overwrite_local_changes() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "base\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "base");
    create_branch(&env, "feature");
    env.write("file.txt", "feature\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "feature");
    switch(&env, "main");
    env.write("other.txt", "main\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "main");
    env.write("file.txt", "dirty\n");

    env.cvx()
        .args(["merge", "feature"])
        .assert()
        .failure()
        .code(128)
        .stderr(predicate::str::contains("local changes"));
}

#[test]
fn add_add_conflict_uses_stages_two_and_three() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("base.txt", "base\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "base");

    create_branch(&env, "feature");
    env.write("collision.txt", "theirs\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "theirs");
    switch(&env, "main");
    env.write("collision.txt", "ours\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "ours");

    env.cvx().args(["merge", "feature"]).assert().failure();
    let status = env.cvx().args(["status", "--porcelain"]).output().unwrap();
    assert!(stdout(&status).contains("AA collision.txt"));

    let stages = stdout(&env.assert_git_cvx_ok(&["ls-files", "--stage", "collision.txt"]));
    assert!(
        !stages
            .lines()
            .any(|line| line.contains(" 1\tcollision.txt"))
    );
    assert!(
        stages
            .lines()
            .any(|line| line.contains(" 2\tcollision.txt"))
    );
    assert!(
        stages
            .lines()
            .any(|line| line.contains(" 3\tcollision.txt"))
    );
}

#[test]
fn modify_delete_conflict_reports_deleted_by_them() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "base\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "base");

    create_branch(&env, "feature");
    std::fs::remove_file(env.path("file.txt")).expect("remove");
    env.cvx().args(["add", "-A"]).assert().success();
    commit(&env, "delete");
    switch(&env, "main");
    env.write("file.txt", "modified\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "modify");

    env.cvx().args(["merge", "feature"]).assert().failure();
    let status = env.cvx().args(["status", "--porcelain"]).output().unwrap();
    assert!(stdout(&status).contains("UD file.txt"));
    let human = env.cvx().args(["status"]).output().unwrap();
    assert!(stdout(&human).contains("deleted by them"));
}

#[test]
fn modify_delete_conflict_reports_deleted_by_us() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "base\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "base");

    create_branch(&env, "feature");
    env.write("file.txt", "theirs modified\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "modify");
    switch(&env, "main");
    std::fs::remove_file(env.path("file.txt")).expect("remove");
    env.cvx().args(["add", "-A"]).assert().success();
    commit(&env, "delete");

    env.cvx().args(["merge", "feature"]).assert().failure();
    let status = env.cvx().args(["status", "--porcelain"]).output().unwrap();
    assert!(stdout(&status).contains("DU file.txt"));
    assert_eq!(env.read("file.txt"), "theirs modified\n");
    let human = env.cvx().args(["status"]).output().unwrap();
    assert!(stdout(&human).contains("deleted by us"));
}

#[test]
fn merge_refuses_untracked_collisions() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("base.txt", "base\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "base");

    create_branch(&env, "feature");
    env.write("new.txt", "from feature\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "feature");
    switch(&env, "main");
    env.write("other.txt", "main\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "main");
    env.write("new.txt", "untracked\n");

    env.cvx()
        .args(["merge", "feature"])
        .assert()
        .failure()
        .code(128)
        .stderr(predicate::str::contains("untracked file"));
    assert!(!env.path(".cvx/MERGE_HEAD").exists());
}

#[test]
fn tags_are_git_compatible_and_decorate_log() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "content\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "first");

    env.cvx().args(["tag", "light"]).assert().success();
    env.cvx()
        .args(["tag", "-a", "v1", "-m", "release 1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created annotated tag v1"));

    let tags = stdout(&env.cvx().args(["tag", "-l"]).output().unwrap());
    assert!(tags.contains("light"));
    assert!(tags.contains("v1"));

    assert_eq!(
        stdout(&env.assert_git_cvx_ok(&["cat-file", "-t", "v1"])).trim(),
        "tag"
    );
    assert!(stdout(&env.assert_git_cvx_ok(&["cat-file", "-p", "v1"])).contains("release 1"));
    let head = stdout(&env.assert_git_cvx_ok(&["rev-parse", "HEAD"]));
    let peeled = stdout(&env.assert_git_cvx_ok(&["rev-parse", "v1^{commit}"]));
    assert_eq!(head, peeled);

    let log = env
        .cvx()
        .args(["log", "--oneline", "--decorate"])
        .output()
        .unwrap();
    let log = stdout(&log);
    assert!(log.contains("HEAD -> main"));
    assert!(log.contains("tag: v1"));
    assert!(log.contains("tag: light"));

    let annotated_log = env.cvx().args(["log", "--oneline", "v1"]).output().unwrap();
    assert!(stdout(&annotated_log).contains("first"));
    env.assert_git_cvx_ok(&["fsck", "--strict"]);
}

#[test]
fn merge_reports_already_up_to_date() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "content\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "base");

    env.cvx()
        .args(["merge", "main"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Already up to date"));
}

#[test]
fn tag_duplicates_require_force() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "content\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "first");

    env.cvx().args(["tag", "v1"]).assert().success();
    env.cvx()
        .args(["tag", "v1"])
        .assert()
        .failure()
        .code(128)
        .stderr(predicate::str::contains("already exists"));

    env.write("file.txt", "second\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "second");
    env.cvx().args(["tag", "-f", "v1"]).assert().success();

    let head = stdout(&env.assert_git_cvx_ok(&["rev-parse", "HEAD"]));
    let tag = stdout(&env.assert_git_cvx_ok(&["rev-parse", "v1^{commit}"]));
    assert_eq!(head, tag);
}

#[test]
fn switch_refuses_during_a_merge() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("file.txt", "one\ntwo\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "base");

    create_branch(&env, "feature");
    env.write("file.txt", "one\nTHEIRS\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "theirs");
    switch(&env, "main");
    env.write("file.txt", "one\nOURS\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "ours");

    env.cvx().args(["merge", "feature"]).assert().failure();
    env.cvx()
        .args(["switch", "feature"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("merge is in progress"));
}

#[test]
fn status_and_diff_detect_renames() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();
    env.write("old.txt", "one\ntwo\nthree\nfour\n");
    env.cvx().args(["add", "."]).assert().success();
    commit(&env, "base");

    std::fs::rename(env.path("old.txt"), env.path("new.txt")).expect("rename");
    env.write("new.txt", "one\ntwo\nthree\nFOUR\n");
    env.cvx().args(["add", "-A"]).assert().success();

    let cvx_status = env.cvx().args(["status", "--porcelain"]).output().unwrap();
    let git_status = env.git_cvx(&["status", "--porcelain=v1"]);
    assert_eq!(normalize(&cvx_status), normalize(&git_status));
    assert!(stdout(&cvx_status).contains("R  old.txt -> new.txt"));

    let diff = env.cvx().args(["diff", "--cached"]).output().unwrap();
    let diff = stdout(&diff);
    assert!(diff.contains("rename from old.txt"));
    assert!(diff.contains("rename to new.txt"));
}
