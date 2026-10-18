#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use assert_cmd::prelude::*;
use tempfile::TempDir;

pub struct TestEnv {
    _temp: TempDir,
    pub repo: PathBuf,
    global_config: PathBuf,
    home: PathBuf,
    xdg: PathBuf,
}

impl TestEnv {
    pub fn new() -> Self {
        Self::with_global_config(
            "[user]\n\tname = Test User\n\temail = test@example.com\n[init]\n\tdefaultBranch = main\n",
        )
    }

    pub fn with_global_config(contents: &str) -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let repo = temp.path().join("repo");
        let home = temp.path().join("home");
        let xdg = temp.path().join("xdg");
        fs::create_dir(&repo).expect("create repo");
        fs::create_dir(&home).expect("create home");
        fs::create_dir(&xdg).expect("create xdg");
        let global_config = temp.path().join("global.gitconfig");
        fs::write(&global_config, contents).expect("global config");

        Self {
            _temp: temp,
            repo,
            global_config,
            home,
            xdg,
        }
    }

    pub fn cvx(&self) -> Command {
        let mut command = Command::cargo_bin("cvx").expect("cvx binary");
        command
            .current_dir(&self.repo)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", &self.global_config)
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", &self.xdg);
        clear_identity_environment(&mut command);
        command
    }

    pub fn git(&self, args: &[&str]) -> Output {
        let mut command = Command::new("git");
        command
            .args(args)
            .current_dir(&self.repo)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", &self.global_config)
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", &self.xdg);
        clear_identity_environment(&mut command);
        command.output().expect("run git")
    }

    /// Runs Git against the CVX control directory.
    pub fn git_cvx(&self, args: &[&str]) -> Output {
        let mut full = vec!["--git-dir=.cvx", "--work-tree=."];
        full.extend_from_slice(args);
        self.git(&full)
    }

    pub fn assert_git_ok(&self, args: &[&str]) -> Output {
        self.assert_git_with(args, false)
    }

    pub fn assert_git_cvx_ok(&self, args: &[&str]) -> Output {
        self.assert_git_with(args, true)
    }

    fn assert_git_with(&self, args: &[&str], cvx: bool) -> Output {
        let output = if cvx {
            self.git_cvx(args)
        } else {
            self.git(args)
        };
        assert!(
            output.status.success(),
            "git {:?} failed\nstdout:\n{}\nstderr:\n{}",
            args,
            stdout(&output),
            stderr(&output)
        );
        output
    }

    pub fn write(&self, path: &str, contents: &str) {
        let path = self.repo.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, contents).expect("write file");
    }

    pub fn read(&self, path: &str) -> String {
        fs::read_to_string(self.repo.join(path)).expect("read file")
    }

    pub fn remove(&self, path: &str) {
        fs::remove_file(self.repo.join(path)).expect("remove file");
    }

    pub fn path(&self, path: &str) -> PathBuf {
        self.repo.join(path)
    }
}

pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

pub fn normalize(output: &Output) -> String {
    stdout(output)
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

pub fn assert_path_exists(path: &Path) {
    assert!(path.exists(), "{} does not exist", path.display());
}

const IDENTITY_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_AUTHOR_NAME",
    "GIT_AUTHOR_EMAIL",
    "GIT_AUTHOR_DATE",
    "GIT_COMMITTER_NAME",
    "GIT_COMMITTER_EMAIL",
    "GIT_COMMITTER_DATE",
    "CVX_DIR",
    "CVX_AUTHOR_NAME",
    "CVX_AUTHOR_EMAIL",
    "CVX_AUTHOR_DATE",
    "CVX_COMMITTER_NAME",
    "CVX_COMMITTER_EMAIL",
    "CVX_COMMITTER_DATE",
];

fn clear_identity_environment(command: &mut Command) {
    for name in IDENTITY_ENV {
        command.env_remove(name);
    }
}
