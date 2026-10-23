#![cfg(unix)]

mod common;

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::Arc;

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

fn serve_git_http(project_root: std::path::PathBuf) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let root = Arc::new(project_root);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                continue;
            };
            let _ = handle_request(&mut stream, &root);
        }
    });
    format!("http://127.0.0.1:{port}")
}

fn handle_request(
    stream: &mut std::net::TcpStream,
    project_root: &std::path::Path,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_owned();
    let target = parts.next().unwrap_or("/").to_owned();

    let mut headers = BTreeMap::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body)?;

    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    let mut command = Command::new("git");
    command
        .arg("http-backend")
        .env("GIT_PROJECT_ROOT", project_root)
        .env("GIT_HTTP_EXPORT_ALL", "1")
        .env("PATH_INFO", path)
        .env("QUERY_STRING", query)
        .env("REQUEST_METHOD", method)
        .env("SERVER_PROTOCOL", "HTTP/1.1")
        .env("CONTENT_LENGTH", content_length.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(content_type) = headers.get("content-type") {
        command.env("CONTENT_TYPE", content_type);
    }
    if let Some(protocol) = headers.get("git-protocol") {
        command.env("GIT_PROTOCOL", protocol);
    }
    let mut child = command.spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&body)?;
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(std::io::Error::other(format!(
            "git http-backend failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let split = output
        .stdout
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| (position, 4))
        .or_else(|| {
            output
                .stdout
                .windows(2)
                .position(|window| window == b"\n\n")
                .map(|position| (position, 2))
        });
    let (headers, body) = match split {
        Some((position, length)) => (
            &output.stdout[..position],
            &output.stdout[position + length..],
        ),
        None => (&[][..], &output.stdout[..]),
    };
    let headers = String::from_utf8_lossy(headers);
    let status = headers
        .lines()
        .find_map(|line| line.strip_prefix("Status:"))
        .and_then(|line| line.split_whitespace().next())
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or(200);
    let content_type = headers
        .lines()
        .find_map(|line| line.strip_prefix("Content-Type:"))
        .map(str::trim)
        .unwrap_or("application/octet-stream");

    write!(
        stream,
        "HTTP/1.1 {status} OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()
}

#[test]
fn sha256_clone_and_push_over_http() {
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    let env = TestEnv::new();
    let temp = env.path("..");
    let remote = temp.join("remote.git");
    let work = temp.join("source");
    run_git(
        &temp,
        &["init", "--bare", "--object-format=sha256", "remote.git"],
    );
    run_git(&temp, &["init", "--object-format=sha256", "source"]);
    run_git(&work, &["config", "user.name", "Test User"]);
    run_git(&work, &["config", "user.email", "test@example.com"]);
    std::fs::write(work.join("file.txt"), "sha256 over http\n").unwrap();
    run_git(&work, &["add", "."]);
    run_git(&work, &["commit", "-m", "sha256 base"]);
    run_git(&work, &["branch", "-M", "main"]);
    run_git(&work, &["push", remote.to_str().unwrap(), "main"]);
    run_git(
        &temp,
        &["--git-dir=remote.git", "config", "http.receivepack", "true"],
    );

    let url = serve_git_http(temp.clone());
    let clone = temp.join("clone");
    env.cvx_in(&temp)
        .args([
            "clone",
            &format!("{url}/remote.git"),
            clone.to_str().unwrap(),
        ])
        .assert()
        .success();
    let log = env
        .cvx_in(&clone)
        .args(["log", "--oneline"])
        .output()
        .unwrap();
    assert!(stdout(&log).contains("sha256 base"));
    let fsck = Command::new("git")
        .args(["--git-dir=.cvx", "--work-tree=.", "fsck", "--strict"])
        .current_dir(&clone)
        .output()
        .unwrap();
    assert!(
        fsck.status.success(),
        "{}",
        String::from_utf8_lossy(&fsck.stderr)
    );

    let repo = temp.join("pusher");
    env.cvx_in(&temp)
        .args(["init", "--object-format", "sha256", "pusher"])
        .assert()
        .success();
    std::fs::write(repo.join("pushed.txt"), "pushed\n").unwrap();
    env.cvx_in(&repo).args(["add", "."]).assert().success();
    env.cvx_in(&repo)
        .args(["commit", "-m", "sha256 push"])
        .assert()
        .success();
    env.cvx_in(&repo)
        .args(["remote", "add", "origin", &format!("{url}/remote.git")])
        .assert()
        .success();
    env.cvx_in(&repo)
        .args(["push", "origin", "main"])
        .assert()
        .success();

    let log = Command::new("git")
        .args(["--git-dir=remote.git", "log", "main", "--oneline"])
        .current_dir(&temp)
        .output()
        .unwrap();
    assert!(
        stdout(&log).contains("sha256 push"),
        "remote log: {}",
        stdout(&log)
    );
}
