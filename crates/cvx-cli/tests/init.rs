use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

use assert_cmd::prelude::*;
use predicates::prelude::*;
use tempfile::TempDir;

struct TestEnv {
    _temp: TempDir,
    repo: PathBuf,
    global_config: PathBuf,
    home: PathBuf,
    xdg: PathBuf,
}

impl TestEnv {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let repo = temp.path().join("repo");
        let home = temp.path().join("home");
        let xdg = temp.path().join("xdg");
        fs::create_dir(&repo).expect("create repo");
        fs::create_dir(&home).expect("create home");
        fs::create_dir(&xdg).expect("create xdg");
        let global_config = temp.path().join("global.gitconfig");
        fs::write(&global_config, "").expect("global config");

        Self {
            _temp: temp,
            repo,
            global_config,
            home,
            xdg,
        }
    }

    fn set_global_config(&self, contents: &str) {
        fs::write(&self.global_config, contents).expect("global config");
    }

    fn cvx(&self) -> Command {
        let mut command = Command::cargo_bin("cvx").expect("cvx binary");
        command
            .current_dir(&self.repo)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", &self.global_config)
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", &self.xdg);
        command
    }

    fn git(&self, args: &[&str]) -> Output {
        Command::new("git")
            .args(args)
            .current_dir(&self.repo)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", &self.global_config)
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", &self.xdg)
            .output()
            .expect("run git")
    }

    fn assert_git_ok(&self, args: &[&str]) -> Output {
        let output = self.git(args);
        assert!(
            output.status.success(),
            "git {:?} failed\nstdout:\n{}\nstderr:\n{}",
            args,
            stdout(&output),
            stderr(&output)
        );
        output
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn init_creates_cvx_repository_readable_by_git() {
    let env = TestEnv::new();

    env.cvx()
        .args(["init"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized empty CVX repository"));

    assert_eq!(
        fs::read_to_string(env.repo.join(".cvx/HEAD")).expect("HEAD"),
        "ref: refs/heads/main\n"
    );
    assert!(env.repo.join(".cvx/objects/pack").is_dir());
    assert!(env.repo.join(".cvx/refs/heads").is_dir());
    assert!(env.repo.join(".cvx/cvx.toml").is_file());

    let config = fs::read_to_string(env.repo.join(".cvx/config")).expect("config");
    assert!(config.contains("repositoryformatversion = 0"));
    assert!(config.contains("logallrefupdates = true"));

    let status = env.assert_git_ok(&[
        "--git-dir",
        ".cvx",
        "--work-tree",
        ".",
        "status",
        "--porcelain=v1",
    ]);
    assert_eq!(
        stdout(&status),
        "",
        ".cvx must be hidden from Git's status via info/exclude"
    );

    let fsck = env.assert_git_ok(&["--git-dir", ".cvx", "--work-tree", ".", "fsck", "--strict"]);
    assert!(
        !stderr(&fsck).contains("error"),
        "git fsck reported an error: {}",
        stderr(&fsck)
    );
}

#[test]
fn init_with_git_storage_is_native_git() {
    let env = TestEnv::new();

    env.cvx()
        .args(["init", "--storage", "git"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized empty Git repository"));

    assert!(env.repo.join(".git/HEAD").is_file());
    assert!(!env.repo.join(".cvx").exists());
    assert_eq!(
        fs::read_to_string(env.repo.join(".git/HEAD")).expect("HEAD"),
        "ref: refs/heads/main\n"
    );

    let status = env.assert_git_ok(&["status", "--porcelain=v1"]);
    assert_eq!(stdout(&status), "");
}

#[test]
fn init_honors_initial_branch() {
    let env = TestEnv::new();

    env.cvx()
        .args(["init", "--initial-branch", "trunk"])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(env.repo.join(".cvx/HEAD")).expect("HEAD"),
        "ref: refs/heads/trunk\n"
    );
}

#[test]
fn init_honors_configured_default_branch() {
    let env = TestEnv::new();
    env.set_global_config("[init]\n defaultBranch = develop\n");

    env.cvx().args(["init"]).assert().success();

    assert_eq!(
        fs::read_to_string(env.repo.join(".cvx/HEAD")).expect("HEAD"),
        "ref: refs/heads/develop\n"
    );
}

#[test]
fn reinitialization_preserves_head() {
    let env = TestEnv::new();

    env.cvx().args(["init"]).assert().success();
    fs::write(env.repo.join(".cvx/HEAD"), "ref: refs/heads/custom\n").expect("custom HEAD");

    env.cvx()
        .args(["init"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Reinitialized existing CVX repository",
        ));

    assert_eq!(
        fs::read_to_string(env.repo.join(".cvx/HEAD")).expect("HEAD"),
        "ref: refs/heads/custom\n"
    );
}

#[test]
fn init_detects_existing_git_repository() {
    let env = TestEnv::new();
    env.assert_git_ok(&["init", "-b", "main"]);

    env.cvx()
        .args(["init"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Detected existing Git repository"));

    assert!(!env.repo.join(".cvx").exists());
    assert!(env.repo.join(".git/HEAD").is_file());
}

#[test]
fn init_rejects_invalid_branch_name() {
    let env = TestEnv::new();

    env.cvx()
        .args(["init", "--initial-branch", "bad..name"])
        .assert()
        .failure()
        .code(128)
        .stderr(predicate::str::contains("invalid reference name"));

    assert!(!env.repo.join(".cvx").exists());
}

#[test]
fn init_quiet_has_no_stdout() {
    let env = TestEnv::new();

    env.cvx()
        .args(["init", "--quiet"])
        .assert()
        .success()
        .stdout("");
}

#[test]
fn version_reports_engine_formats() {
    let env = TestEnv::new();

    env.cvx()
        .args(["version"])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "cvx {}",
            env!("CARGO_PKG_VERSION")
        )))
        .stdout(predicate::str::contains("object formats: sha1, sha256"));
}

#[test]
fn created_repository_can_be_opened_by_discovery() {
    let env = TestEnv::new();
    env.cvx().args(["init"]).assert().success();

    let nested = env.repo.join("src/nested");
    fs::create_dir_all(&nested).expect("nested");

    let repository = cvx_repository::Repository::discover(&nested).expect("discover");
    assert_eq!(
        repository.work_tree(),
        env.repo.canonicalize().expect("canonical repo")
    );
}

#[test]
fn help_lists_only_implemented_commands() {
    let env = TestEnv::new();

    let output = env.cvx().args(["--help"]).output().expect("help");
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains("init"));
    assert!(text.contains("version"));
    assert!(text.contains("clone"));
    assert!(text.contains("push"));
}
