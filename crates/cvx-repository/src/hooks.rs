use std::path::PathBuf;
use std::process::{Command, Stdio};

use cvx_core::{Error, Result};

use crate::repository::Repository;

impl Repository {
    /// Returns the directory containing repository hooks.
    pub fn hooks_dir(&self) -> Result<PathBuf> {
        if let Some(path) = self.config_value("core.hookspath")? {
            let path = PathBuf::from(path);
            return Ok(if path.is_absolute() {
                path
            } else {
                self.work_tree().join(path)
            });
        }
        Ok(self.control_dir().join("hooks"))
    }

    /// Runs a repository hook when it exists and is executable.
    ///
    /// Missing hooks are a no-op. A non-zero exit status aborts the caller.
    pub(crate) fn run_hook(&self, name: &str, args: &[String], stdin: Option<&[u8]>) -> Result<()> {
        let hook = self.hooks_dir()?.join(name);
        if !hook.is_file() {
            return Ok(());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&hook)
                .map_err(|error| Error::io(hook.display().to_string(), error))?
                .permissions()
                .mode();
            if mode & 0o111 == 0 {
                return Ok(());
            }
        }

        let mut command = hook_command(&hook);
        command
            .args(args)
            .env("GIT_DIR", self.control_dir())
            .env("GIT_WORK_TREE", self.work_tree())
            .env("CVX_DIR", self.control_dir())
            .current_dir(self.work_tree())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            });

        let mut child = command
            .spawn()
            .map_err(|error| Error::Message(format!("failed to run {name} hook: {error}")))?;
        if let Some(input) = stdin {
            use std::io::Write;
            if let Some(mut handle) = child.stdin.take() {
                handle.write_all(input).map_err(|error| {
                    Error::Message(format!("failed to write to {name} hook: {error}"))
                })?;
            }
        }
        let status = child
            .wait()
            .map_err(|error| Error::Message(format!("failed to wait for {name} hook: {error}")))?;
        if status.success() {
            Ok(())
        } else {
            Err(Error::Message(format!(
                "{name} hook declined the operation ({status})"
            )))
        }
    }
}

#[cfg(windows)]
fn hook_command(hook: &std::path::Path) -> Command {
    let extension = hook
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    if extension.eq_ignore_ascii_case("bat") || extension.eq_ignore_ascii_case("cmd") {
        let mut command = Command::new("cmd");
        command.arg("/C").arg(hook);
        command
    } else {
        Command::new(hook)
    }
}

#[cfg(not(windows))]
fn hook_command(hook: &std::path::Path) -> Command {
    Command::new(hook)
}
