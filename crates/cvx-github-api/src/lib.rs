//! GitHub REST API client.
//!
//! The client authenticates with a token resolved from the credential store or
//! `CVX_TOKEN`/`GITHUB_TOKEN`. It implements the repository, issue,
//! pull-request, release and workflow operations used by the CLI.

#![forbid(unsafe_code)]

use std::io::Read;
use std::time::Duration;

use cvx_core::{Error, Result};
use cvx_credential::resolve_for_host;
use serde::Deserialize;
use serde_json::{Value, json};

const API_BASE: &str = "https://api.github.com";
const API_VERSION: &str = "2022-11-28";
const MAX_BODY: u64 = 8 * 1024 * 1024;

/// Authenticated GitHub user.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct User {
    /// Login name.
    pub login: String,
    /// Numeric account id.
    pub id: u64,
    /// Display name, when set.
    #[serde(default)]
    pub name: Option<String>,
}

/// GitHub repository summary.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Repository {
    /// Owner login.
    pub owner: Owner,
    /// Repository name.
    pub name: String,
    /// `owner/name` form.
    pub full_name: String,
    /// Whether the repository is private.
    pub private: bool,
    /// HTTPS clone URL.
    pub clone_url: String,
    /// SSH clone URL.
    pub ssh_url: String,
    /// Browser URL.
    pub html_url: String,
    /// Default branch name.
    pub default_branch: String,
}

/// Repository owner.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Owner {
    /// Login name.
    pub login: String,
}

/// GitHub issue summary.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Issue {
    /// Issue number.
    pub number: u64,
    /// Title.
    pub title: String,
    /// `open` or `closed`.
    pub state: String,
    /// Browser URL.
    pub html_url: String,
    /// Author login.
    pub user: Owner,
}

/// GitHub pull request summary.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct PullRequest {
    /// Pull request number.
    pub number: u64,
    /// Title.
    pub title: String,
    /// `open`, `closed` or `merged` in list responses.
    pub state: String,
    /// Browser URL.
    pub html_url: String,
    /// Source branch.
    pub head: PullRef,
    /// Target branch.
    pub base: PullRef,
    /// Merge state, when requested.
    #[serde(default)]
    pub mergeable: Option<bool>,
}

/// One side of a pull request.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct PullRef {
    /// Branch label, such as `owner:branch`.
    pub label: String,
    /// Branch name.
    pub r#ref: String,
    /// Commit id.
    pub sha: String,
}

/// A pull-request review.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Review {
    /// Review id.
    pub id: u64,
    /// `APPROVED`, `CHANGES_REQUESTED`, `COMMENTED`, etc.
    pub state: String,
    /// Review body.
    #[serde(default)]
    pub body: Option<String>,
    /// Author.
    pub user: Owner,
    /// Browser URL.
    #[serde(default)]
    pub html_url: Option<String>,
}

/// A GitHub Actions check run.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CheckRun {
    /// Check run id.
    pub id: u64,
    /// Check name.
    pub name: String,
    /// Current status.
    pub status: String,
    /// Final conclusion, when complete.
    #[serde(default)]
    pub conclusion: Option<String>,
    /// Commit being checked.
    pub head_sha: String,
    /// Browser URL.
    #[serde(default)]
    pub html_url: Option<String>,
}

/// A GitHub label.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Label {
    /// Label name.
    pub name: String,
    /// Hex color without `#`.
    pub color: String,
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
}

/// GitHub release summary.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Release {
    /// Tag name.
    pub tag_name: String,
    /// Release name.
    #[serde(default)]
    pub name: Option<String>,
    /// Browser URL.
    pub html_url: String,
    /// Whether this is a draft.
    pub draft: bool,
    /// Whether this is a prerelease.
    pub prerelease: bool,
}

/// One workflow run.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct WorkflowRun {
    /// Run id.
    pub id: u64,
    /// Display name.
    #[serde(default)]
    pub name: Option<String>,
    /// Run status.
    pub status: String,
    /// Run conclusion, when complete.
    #[serde(default)]
    pub conclusion: Option<String>,
    /// Browser URL.
    pub html_url: String,
}

/// Options for creating a release.
#[derive(Debug, Clone, Copy)]
pub struct ReleaseOptions<'a> {
    /// Tag name.
    pub tag: &'a str,
    /// Release name.
    pub name: Option<&'a str>,
    /// Release notes.
    pub body: Option<&'a str>,
    /// Create as a draft.
    pub draft: bool,
    /// Mark as a prerelease.
    pub prerelease: bool,
}

/// Result of merging a pull request.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct MergeResult {
    /// Whether the merge succeeded.
    pub merged: bool,
    /// Result message.
    pub message: String,
    /// Merge commit id, when available.
    #[serde(default)]
    pub sha: Option<String>,
}

/// Authenticated GitHub REST client.
pub struct GitHubClient {
    token: String,
    agent: ureq::Agent,
}

impl GitHubClient {
    /// Creates a client from the configured credential store or environment.
    pub fn from_credentials() -> Result<Self> {
        let credential = resolve_for_host("api.github.com")?
            .or(resolve_for_host("github.com")?)
            .ok_or_else(|| {
                Error::Message(
                    "GitHub token is not configured; run `cvx github login` or set CVX_TOKEN"
                        .to_owned(),
                )
            })?;
        Ok(Self::new(&credential.password))
    }

    /// Creates a client with an explicit token.
    pub fn new(token: &str) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(60))
            .redirects(0)
            .user_agent(concat!("cvx/", env!("CARGO_PKG_VERSION")))
            .build();
        Self {
            token: token.to_owned(),
            agent,
        }
    }

    /// Returns the authenticated user.
    pub fn current_user(&self) -> Result<User> {
        self.parse(self.request("GET", "/user", None)?)
    }

    /// Creates a repository for the authenticated user.
    pub fn create_repository(
        &self,
        name: &str,
        private: bool,
        description: Option<&str>,
    ) -> Result<Repository> {
        validate_name(name, "repository")?;
        let body = json!({
            "name": name,
            "private": private,
            "description": description,
            "auto_init": false,
        });
        self.parse(self.request("POST", "/user/repos", Some(body))?)
    }

    /// Deletes a repository.
    pub fn delete_repository(&self, owner: &str, repo: &str) -> Result<()> {
        self.request(
            "DELETE",
            &format!(
                "/repos/{}/{}",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            None,
        )?;
        Ok(())
    }

    /// Returns a repository.
    pub fn repository(&self, owner: &str, repo: &str) -> Result<Repository> {
        self.parse(self.request(
            "GET",
            &format!(
                "/repos/{}/{}",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            None,
        )?)
    }

    /// Creates an issue.
    pub fn create_issue(
        &self,
        owner: &str,
        repo: &str,
        title: &str,
        body: Option<&str>,
    ) -> Result<Issue> {
        let body = json!({ "title": title, "body": body });
        self.parse(self.request(
            "POST",
            &format!(
                "/repos/{}/{}/issues",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            Some(body),
        )?)
    }

    /// Lists issues, excluding pull requests.
    pub fn list_issues(&self, owner: &str, repo: &str, state: &str) -> Result<Vec<Issue>> {
        validate_state(state)?;
        let value = self.request(
            "GET",
            &format!(
                "/repos/{}/{}/issues?state={state}&per_page=100",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            None,
        )?;
        let values = value
            .as_array()
            .ok_or_else(|| Error::corrupt("GitHub issues response is not an array"))?;
        let mut issues = Vec::new();
        for value in values {
            if value.get("pull_request").is_some() {
                continue;
            }
            issues.push(
                serde_json::from_value(value.clone())
                    .map_err(|error| Error::corrupt(format!("invalid GitHub issue: {error}")))?,
            );
        }
        Ok(issues)
    }

    /// Closes an issue.
    pub fn close_issue(&self, owner: &str, repo: &str, number: u64) -> Result<Issue> {
        self.parse(self.request(
            "PATCH",
            &format!(
                "/repos/{}/{}/issues/{number}",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            Some(json!({ "state": "closed" })),
        )?)
    }

    /// Creates a pull request.
    pub fn create_pull_request(
        &self,
        owner: &str,
        repo: &str,
        title: &str,
        head: &str,
        base: &str,
        body: Option<&str>,
    ) -> Result<PullRequest> {
        let body = json!({ "title": title, "head": head, "base": base, "body": body });
        self.parse(self.request(
            "POST",
            &format!(
                "/repos/{}/{}/pulls",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            Some(body),
        )?)
    }

    /// Lists pull requests.
    pub fn list_pull_requests(
        &self,
        owner: &str,
        repo: &str,
        state: &str,
    ) -> Result<Vec<PullRequest>> {
        validate_state(state)?;
        self.parse(self.request(
            "GET",
            &format!(
                "/repos/{}/{}/pulls?state={state}&per_page=100",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            None,
        )?)
    }

    /// Lists reviews for a pull request.
    pub fn list_pull_request_reviews(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<Vec<Review>> {
        self.parse(self.request(
            "GET",
            &format!(
                "/repos/{}/{}/pulls/{number}/reviews?per_page=100",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            None,
        )?)
    }

    /// Creates a pull-request review.
    ///
    /// `event` is `APPROVE`, `REQUEST_CHANGES` or `COMMENT`.
    pub fn create_pull_request_review(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        event: &str,
        body: Option<&str>,
    ) -> Result<Review> {
        let event = event.to_ascii_uppercase();
        if !matches!(event.as_str(), "APPROVE" | "REQUEST_CHANGES" | "COMMENT") {
            return Err(Error::Message(format!("invalid review event: {event}")));
        }
        let payload = json!({ "event": event, "body": body });
        self.parse(self.request(
            "POST",
            &format!(
                "/repos/{}/{}/pulls/{number}/reviews",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            Some(payload),
        )?)
    }

    /// Lists check runs for a commit or ref.
    pub fn list_check_runs(
        &self,
        owner: &str,
        repo: &str,
        reference: &str,
    ) -> Result<Vec<CheckRun>> {
        let reference = encode_path_segment(reference);
        let value = self.request(
            "GET",
            &format!(
                "/repos/{}/{}/commits/{reference}/check-runs?per_page=100",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            None,
        )?;
        let runs = value
            .get("check_runs")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::corrupt("GitHub check-runs response is missing check_runs"))?;
        runs.iter()
            .cloned()
            .map(|value| {
                serde_json::from_value(value)
                    .map_err(|error| Error::corrupt(format!("invalid check run: {error}")))
            })
            .collect()
    }

    /// Lists repository labels.
    pub fn list_labels(&self, owner: &str, repo: &str) -> Result<Vec<Label>> {
        self.parse(self.request(
            "GET",
            &format!(
                "/repos/{}/{}/labels?per_page=100",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            None,
        )?)
    }

    /// Creates a repository label.
    pub fn create_label(
        &self,
        owner: &str,
        repo: &str,
        name: &str,
        color: &str,
        description: Option<&str>,
    ) -> Result<Label> {
        validate_label_color(color)?;
        let payload = json!({ "name": name, "color": color, "description": description });
        self.parse(self.request(
            "POST",
            &format!(
                "/repos/{}/{}/labels",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            Some(payload),
        )?)
    }

    /// Deletes a repository label.
    pub fn delete_label(&self, owner: &str, repo: &str, name: &str) -> Result<()> {
        let name = encode_path_segment(name);
        self.request(
            "DELETE",
            &format!(
                "/repos/{}/{}/labels/{name}",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            None,
        )?;
        Ok(())
    }

    /// Adds labels to an issue or pull request.
    pub fn add_labels(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        labels: &[String],
    ) -> Result<Vec<Label>> {
        self.parse(self.request(
            "POST",
            &format!(
                "/repos/{}/{}/issues/{number}/labels",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            Some(json!({ "labels": labels })),
        )?)
    }

    /// Removes one label from an issue or pull request.
    pub fn remove_label(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        label: &str,
    ) -> Result<Vec<Label>> {
        let label = encode_path_segment(label);
        self.parse(self.request(
            "DELETE",
            &format!(
                "/repos/{}/{}/issues/{number}/labels/{label}",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            None,
        )?)
    }

    /// Merges a pull request.
    pub fn merge_pull_request(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        method: &str,
    ) -> Result<MergeResult> {
        if !matches!(method, "merge" | "squash" | "rebase") {
            return Err(Error::Message(format!("invalid merge method '{method}'")));
        }
        self.parse(self.request(
            "PUT",
            &format!(
                "/repos/{}/{}/pulls/{number}/merge",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            Some(json!({ "merge_method": method })),
        )?)
    }

    /// Creates a release.
    pub fn create_release(
        &self,
        owner: &str,
        repo: &str,
        options: &ReleaseOptions<'_>,
    ) -> Result<Release> {
        let body = json!({
            "tag_name": options.tag,
            "name": options.name,
            "body": options.body,
            "draft": options.draft,
            "prerelease": options.prerelease,
        });
        self.parse(self.request(
            "POST",
            &format!(
                "/repos/{}/{}/releases",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            Some(body),
        )?)
    }

    /// Lists releases.
    pub fn list_releases(&self, owner: &str, repo: &str) -> Result<Vec<Release>> {
        self.parse(self.request(
            "GET",
            &format!(
                "/repos/{}/{}/releases?per_page=30",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            None,
        )?)
    }

    /// Lists recent workflow runs.
    pub fn list_workflow_runs(&self, owner: &str, repo: &str) -> Result<Vec<WorkflowRun>> {
        let value = self.request(
            "GET",
            &format!(
                "/repos/{}/{}/actions/runs?per_page=30",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            None,
        )?;
        let runs = value
            .get("workflow_runs")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::corrupt("GitHub workflow response is missing workflow_runs"))?;
        runs.iter()
            .cloned()
            .map(|value| {
                serde_json::from_value(value)
                    .map_err(|error| Error::corrupt(format!("invalid workflow run: {error}")))
            })
            .collect()
    }

    fn request(&self, method: &str, path: &str, body: Option<Value>) -> Result<Value> {
        let url = format!("{API_BASE}{path}");
        let request = self
            .agent
            .request(method, &url)
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Accept", "application/vnd.github+json")
            .set("X-GitHub-Api-Version", API_VERSION);

        let response = match body {
            Some(body) => {
                let bytes = serde_json::to_vec(&body).map_err(|error| {
                    Error::Message(format!("failed to encode request: {error}"))
                })?;
                request
                    .set("Content-Type", "application/json")
                    .send_bytes(&bytes)
            }
            None => request.call(),
        };

        let response = response.map_err(|error| github_error(&url, error))?;
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(MAX_BODY)
            .read_to_end(&mut bytes)
            .map_err(|error| Error::io("failed to read GitHub response", error))?;
        if bytes.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&bytes)
            .map_err(|error| Error::corrupt(format!("invalid GitHub response: {error}")))
    }

    fn parse<T: for<'de> Deserialize<'de>>(&self, value: Value) -> Result<T> {
        serde_json::from_value(value)
            .map_err(|error| Error::corrupt(format!("invalid GitHub response: {error}")))
    }
}

fn github_error(url: &str, error: ureq::Error) -> Error {
    match error {
        ureq::Error::Status(code, response) => {
            let mut text = String::new();
            let _ = response.into_reader().take(4096).read_to_string(&mut text);
            let message = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|value| {
                    value
                        .get("message")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| text.trim().to_owned());
            if code == 401 {
                Error::Message(
                    "GitHub authentication failed; check `cvx github login` or CVX_TOKEN"
                        .to_owned(),
                )
            } else if code == 403 {
                Error::Message(format!(
                    "GitHub denied the request (HTTP 403): {message}. Check token scopes and rate limits"
                ))
            } else if message.is_empty() {
                Error::Message(format!("GitHub API returned HTTP {code} for {url}"))
            } else {
                Error::Message(format!("GitHub API returned HTTP {code}: {message}"))
            }
        }
        ureq::Error::Transport(transport) => {
            Error::Message(format!("GitHub transport error for {url}: {transport}"))
        }
    }
}

fn validate_name<'a>(name: &'a str, kind: &str) -> Result<&'a str> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(Error::Message(format!(
            "invalid GitHub {kind} name: {name}"
        )));
    }
    Ok(name)
}

fn validate_label_color(color: &str) -> Result<()> {
    let color = color.strip_prefix('#').unwrap_or(color);
    if color.len() != 6 || !color.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Error::Message(format!(
            "invalid label color '{color}'; expected six hexadecimal digits"
        )));
    }
    Ok(())
}

fn encode_path_segment(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn validate_state(state: &str) -> Result<()> {
    if matches!(state, "open" | "closed" | "all") {
        Ok(())
    } else {
        Err(Error::Message(format!("invalid issue/PR state: {state}")))
    }
}

/// Parses `owner/repo` from a GitHub URL or bare slug.
pub fn parse_repository_slug(url: &str) -> Result<(String, String)> {
    let path = if let Some(rest) = url.strip_prefix("https://github.com/") {
        rest
    } else if let Some(rest) = url.strip_prefix("http://github.com/") {
        rest
    } else if let Some(rest) = url.strip_prefix("ssh://git@github.com/") {
        rest
    } else if let Some((_, path)) = url.split_once("github.com:") {
        path
    } else {
        url
    };
    let path = path.trim_end_matches('/').trim_end_matches(".git");
    let (owner, repo) = path
        .split_once('/')
        .ok_or_else(|| Error::Message(format!("cannot parse GitHub repository from '{url}'")))?;
    let owner = validate_name(owner, "owner")?.to_owned();
    let repo = validate_name(repo, "repository")?.to_owned();
    Ok((owner, repo))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_repository_slugs() {
        for url in [
            "https://github.com/owner/repo.git",
            "git@github.com:owner/repo.git",
            "ssh://git@github.com/owner/repo.git",
            "owner/repo",
        ] {
            assert_eq!(
                parse_repository_slug(url).expect("slug"),
                ("owner".to_owned(), "repo".to_owned())
            );
        }
        assert!(parse_repository_slug("https://example.com/owner/repo").is_err());
    }

    #[test]
    fn validates_names_and_states() {
        assert!(validate_name("repo-1.0", "repository").is_ok());
        assert!(validate_name("bad/name", "repository").is_err());
        assert!(validate_state("open").is_ok());
        assert!(validate_state("merged").is_err());
        assert!(validate_label_color("ff00aa").is_ok());
        assert!(validate_label_color("#ff00aa").is_ok());
        assert!(validate_label_color("xyz").is_err());
        assert_eq!(
            encode_path_segment("good first issue"),
            "good%20first%20issue"
        );
        assert_eq!(
            encode_path_segment("refs/heads/main"),
            "refs%2Fheads%2Fmain"
        );
    }
}
