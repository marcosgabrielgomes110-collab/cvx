use std::process::{Command, Stdio};

use assert_cmd::prelude::*;

fn cvx() -> Command {
    Command::cargo_bin("cvx").expect("cvx binary")
}

#[test]
fn stores_credentials_with_owner_only_permissions() {
    let temp = tempfile::tempdir().expect("tempdir");
    let credential_file = temp.path().join("credentials");
    let mut child = cvx()
        .env("CVX_CREDENTIAL_FILE", &credential_file)
        .args(["credential", "store", "github.com"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn");
    {
        use std::io::Write;
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(b"test-token\n")
            .expect("write token");
    }
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("wait");
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("test-token"));
    assert!(credential_file.is_file());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&credential_file)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    let list = cvx()
        .env("CVX_CREDENTIAL_FILE", &credential_file)
        .args(["credential", "list"])
        .output()
        .expect("list");
    assert!(String::from_utf8_lossy(&list.stdout).contains("github.com"));

    let remove = cvx()
        .env("CVX_CREDENTIAL_FILE", &credential_file)
        .args(["credential", "remove", "github.com"])
        .output()
        .expect("remove");
    assert!(remove.status.success());
    assert!(
        !credential_file.exists()
            || std::fs::read_to_string(&credential_file)
                .unwrap()
                .is_empty()
    );
}
