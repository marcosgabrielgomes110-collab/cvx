use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;

fn cvx() -> Command {
    Command::cargo_bin("cvx").expect("cvx binary")
}

#[test]
fn github_help_lists_extended_workflows() {
    let output = cvx()
        .args(["github", "--help"])
        .output()
        .expect("github help");
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);

    for command in [
        "login",
        "scopes",
        "graphql",
        "collaborator",
        "milestone",
        "checks",
    ] {
        assert!(text.contains(command), "missing {command} in:\n{text}");
    }
}

#[test]
fn oauth_login_options_are_documented() {
    cvx()
        .args(["github", "login", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--oauth"))
        .stdout(predicate::str::contains("--client-id"))
        .stdout(predicate::str::contains("--scope"));
}

#[test]
fn review_thread_commands_are_documented() {
    cvx()
        .args(["github", "pr", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("threads"))
        .stdout(predicate::str::contains("resolve"))
        .stdout(predicate::str::contains("unresolve"));
}

#[test]
fn required_check_commands_are_documented() {
    cvx()
        .args(["github", "checks", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("required"))
        .stdout(predicate::str::contains("require"));
}

#[test]
fn release_asset_commands_are_documented() {
    cvx()
        .args(["github", "release", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("assets"))
        .stdout(predicate::str::contains("upload"))
        .stdout(predicate::str::contains("delete-asset"));
}

#[test]
fn issue_assignee_and_milestone_options_are_documented() {
    cvx()
        .args(["github", "issue", "create", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--assignee"))
        .stdout(predicate::str::contains("--milestone"));

    cvx()
        .args(["github", "issue", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("assign"))
        .stdout(predicate::str::contains("unassign"));
}

#[test]
fn graphql_and_scope_commands_are_documented() {
    cvx()
        .args(["github", "graphql", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--query"))
        .stdout(predicate::str::contains("--variables"));
    cvx()
        .args(["github", "scopes", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--require"));
}
