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

mod oauth;

pub use oauth::{DeviceCode, DevicePoll, OAuthToken, poll_device_token, request_device_code};

const API_BASE: &str = "https://api.github.com";
const API_VERSION: &str = "2022-11-28";
const MAX_BODY: u64 = 8 * 1024 * 1024;

const REVIEW_THREADS_QUERY: &str = r#"
query($owner: String!, $repo: String!, $number: Int!) {
  repository(owner: $owner, name: $repo) {
    pullRequest(number: $number) {
      reviewThreads(first: 100) {
        nodes {
          id
          isResolved
          isOutdated
          path
          line
          comments(first: 100) {
            nodes {
              id
              body
              author { login }
              url
              createdAt
            }
          }
        }
      }
    }
  }
}
"#;

const RESOLVE_REVIEW_THREAD_MUTATION: &str = r#"
mutation($threadId: ID!) {
  resolveReviewThread(input: {threadId: $threadId}) {
    thread { id isResolved }
  }
}
"#;

const UNRESOLVE_REVIEW_THREAD_MUTATION: &str = r#"
mutation($threadId: ID!) {
  unresolveReviewThread(input: {threadId: $threadId}) {
    thread { id isResolved }
  }
}
"#;

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
    /// Assigned users.
    #[serde(default)]
    pub assignees: Vec<Owner>,
    /// Milestone, when assigned.
    #[serde(default)]
    pub milestone: Option<Milestone>,
}

/// Repository collaborator.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Collaborator {
    /// Login name.
    pub login: String,
    /// Numeric account id.
    pub id: u64,
    /// Effective permissions, when returned by the API.
    #[serde(default)]
    pub permissions: Option<CollaboratorPermissions>,
}

/// Boolean permission set returned for collaborators.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CollaboratorPermissions {
    /// Administrative access.
    #[serde(default)]
    pub admin: bool,
    /// Maintain access.
    #[serde(default)]
    pub maintain: bool,
    /// Push access.
    #[serde(default)]
    pub push: bool,
    /// Triage access.
    #[serde(default)]
    pub triage: bool,
    /// Pull access.
    #[serde(default)]
    pub pull: bool,
}

/// Effective permission for one collaborator.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CollaboratorPermission {
    /// Permission name: pull, triage, push, maintain or admin.
    pub permission: String,
    /// User.
    pub user: Owner,
}

/// Repository milestone.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Milestone {
    /// Milestone number.
    pub number: u64,
    /// Title.
    pub title: String,
    /// `open` or `closed`.
    pub state: String,
    /// Description.
    #[serde(default)]
    pub description: Option<String>,
    /// Due date in ISO 8601 form.
    #[serde(default)]
    pub due_on: Option<String>,
    /// Open issue count.
    #[serde(default)]
    pub open_issues: u64,
    /// Closed issue count.
    #[serde(default)]
    pub closed_issues: u64,
    /// Browser URL.
    #[serde(default)]
    pub html_url: Option<String>,
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

/// GitHub release asset.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ReleaseAsset {
    /// Asset id.
    pub id: u64,
    /// File name.
    pub name: String,
    /// Size in bytes.
    pub size: u64,
    /// MIME type reported by GitHub.
    pub content_type: String,
    /// Upload state.
    pub state: String,
    /// Download URL.
    pub browser_download_url: String,
    /// Optional label.
    #[serde(default)]
    pub label: Option<String>,
}

/// GitHub release summary.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Release {
    /// Release id.
    #[serde(default)]
    pub id: u64,
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
    /// Uploaded assets.
    #[serde(default)]
    pub assets: Vec<ReleaseAsset>,
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

/// Options for creating an issue.
#[derive(Debug, Clone, Copy)]
pub struct IssueOptions<'a> {
    /// Issue title.
    pub title: &'a str,
    /// Issue body.
    pub body: Option<&'a str>,
    /// Logins to assign.
    pub assignees: &'a [String],
    /// Milestone number.
    pub milestone: Option<u64>,
}

/// Options for uploading a release asset.
#[derive(Debug, Clone, Copy)]
pub struct ReleaseAssetOptions<'a> {
    /// Asset file name.
    pub name: &'a str,
    /// Optional label.
    pub label: Option<&'a str>,
    /// MIME type.
    pub content_type: &'a str,
    /// File bytes.
    pub bytes: &'a [u8],
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

/// Required status checks configured for a protected branch.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RequiredStatusChecks {
    /// Require branches to be up to date before merging.
    #[serde(default)]
    pub strict: bool,
    /// Required check contexts.
    #[serde(default)]
    pub contexts: Vec<String>,
    /// Required checks with explicit app ids.
    #[serde(default)]
    pub checks: Vec<RequiredCheck>,
}

/// One required check entry.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RequiredCheck {
    /// Check context.
    pub context: String,
    /// GitHub App id, when configured.
    #[serde(default)]
    pub app_id: Option<u64>,
}

/// A review comment in a GraphQL review thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewThreadComment {
    /// Node id.
    pub id: String,
    /// Author login, when available.
    pub author: Option<String>,
    /// Comment body.
    pub body: String,
    /// Browser URL.
    pub url: Option<String>,
    /// Creation timestamp.
    pub created_at: Option<String>,
}

/// A GraphQL review thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewThread {
    /// Thread node id.
    pub id: String,
    /// Whether the thread is resolved.
    pub is_resolved: bool,
    /// Whether the thread is outdated.
    pub is_outdated: bool,
    /// File path.
    pub path: Option<String>,
    /// Line number.
    pub line: Option<u64>,
    /// Comments in the thread.
    pub comments: Vec<ReviewThreadComment>,
}

/// Resolution state returned by a review-thread mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewThreadState {
    /// Thread node id.
    pub id: String,
    /// Whether the thread is resolved.
    pub is_resolved: bool,
}

/// OAuth scopes reported by GitHub for the current token.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuthScopes {
    /// Scopes granted to the token.
    pub token: Vec<String>,
    /// Scopes accepted by the endpoint.
    pub accepted: Vec<String>,
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

    /// Returns OAuth scopes reported for the current token.
    pub fn auth_scopes(&self) -> Result<AuthScopes> {
        let (_, headers) = self.request_internal("GET", "/user", None)?;
        Ok(AuthScopes {
            token: headers.token_scopes,
            accepted: headers.accepted_scopes,
        })
    }

    /// Executes a GraphQL query and returns the raw response data.
    pub fn graphql(&self, query: &str, variables: Option<Value>) -> Result<Value> {
        let payload = json!({ "query": query, "variables": variables });
        let value = self.request("POST", "/graphql", Some(payload))?;
        if let Some(errors) = value.get("errors").and_then(Value::as_array) {
            let messages: Vec<String> = errors
                .iter()
                .filter_map(|error| error.get("message").and_then(Value::as_str))
                .map(str::to_owned)
                .collect();
            return Err(Error::Message(format!(
                "GitHub GraphQL error: {}",
                if messages.is_empty() {
                    "unknown error".to_owned()
                } else {
                    messages.join("; ")
                }
            )));
        }
        Ok(value)
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

    /// Lists repository collaborators.
    pub fn list_collaborators(&self, owner: &str, repo: &str) -> Result<Vec<Collaborator>> {
        self.parse(self.request(
            "GET",
            &format!(
                "/repos/{}/{}/collaborators?affiliation=all&per_page=100",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            None,
        )?)
    }

    /// Returns a collaborator's effective permission.
    pub fn collaborator_permission(
        &self,
        owner: &str,
        repo: &str,
        user: &str,
    ) -> Result<CollaboratorPermission> {
        let user = encode_path_segment(validate_name(user, "user")?);
        self.parse(self.request(
            "GET",
            &format!(
                "/repos/{}/{}/collaborators/{user}/permission",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            None,
        )?)
    }

    /// Adds or updates a repository collaborator.
    pub fn add_collaborator(
        &self,
        owner: &str,
        repo: &str,
        user: &str,
        permission: &str,
    ) -> Result<()> {
        validate_collaborator_permission(permission)?;
        let user = encode_path_segment(validate_name(user, "user")?);
        self.request(
            "PUT",
            &format!(
                "/repos/{}/{}/collaborators/{user}",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            Some(json!({ "permission": permission })),
        )?;
        Ok(())
    }

    /// Removes a repository collaborator.
    pub fn remove_collaborator(&self, owner: &str, repo: &str, user: &str) -> Result<()> {
        let user = encode_path_segment(validate_name(user, "user")?);
        self.request(
            "DELETE",
            &format!(
                "/repos/{}/{}/collaborators/{user}",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            None,
        )?;
        Ok(())
    }

    /// Creates an issue.
    pub fn create_issue(
        &self,
        owner: &str,
        repo: &str,
        options: &IssueOptions<'_>,
    ) -> Result<Issue> {
        let body = json!({
            "title": options.title,
            "body": options.body,
            "assignees": options.assignees,
            "milestone": options.milestone,
        });
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

    /// Adds assignees to an issue or pull request.
    pub fn add_assignees(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        assignees: &[String],
    ) -> Result<Issue> {
        self.parse(self.request(
            "POST",
            &format!(
                "/repos/{}/{}/issues/{number}/assignees",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            Some(json!({ "assignees": assignees })),
        )?)
    }

    /// Removes assignees from an issue or pull request.
    pub fn remove_assignees(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        assignees: &[String],
    ) -> Result<Issue> {
        self.parse(self.request(
            "DELETE",
            &format!(
                "/repos/{}/{}/issues/{number}/assignees",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            Some(json!({ "assignees": assignees })),
        )?)
    }

    /// Lists milestones.
    pub fn list_milestones(&self, owner: &str, repo: &str, state: &str) -> Result<Vec<Milestone>> {
        validate_state(state)?;
        self.parse(self.request(
            "GET",
            &format!(
                "/repos/{}/{}/milestones?state={state}&per_page=100",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            None,
        )?)
    }

    /// Creates a milestone.
    pub fn create_milestone(
        &self,
        owner: &str,
        repo: &str,
        title: &str,
        description: Option<&str>,
        due_on: Option<&str>,
    ) -> Result<Milestone> {
        self.parse(self.request(
            "POST",
            &format!(
                "/repos/{}/{}/milestones",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            Some(json!({
                "title": title,
                "description": description,
                "due_on": due_on,
            })),
        )?)
    }

    /// Closes a milestone.
    pub fn close_milestone(&self, owner: &str, repo: &str, number: u64) -> Result<Milestone> {
        self.parse(self.request(
            "PATCH",
            &format!(
                "/repos/{}/{}/milestones/{number}",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            Some(json!({ "state": "closed" })),
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

    /// Returns required status checks for a protected branch.
    pub fn required_status_checks(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
    ) -> Result<RequiredStatusChecks> {
        let branch = encode_path_segment(branch);
        self.parse(self.request(
            "GET",
            &format!(
                "/repos/{}/{}/branches/{branch}/protection/required_status_checks",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            None,
        )?)
    }

    /// Replaces required status checks for a protected branch.
    pub fn set_required_status_checks(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        contexts: &[String],
        strict: bool,
    ) -> Result<RequiredStatusChecks> {
        let branch = encode_path_segment(branch);
        self.parse(self.request(
            "PATCH",
            &format!(
                "/repos/{}/{}/branches/{branch}/protection/required_status_checks",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            Some(json!({ "strict": strict, "contexts": contexts })),
        )?)
    }

    /// Lists pull-request review threads through GraphQL.
    pub fn review_threads(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<Vec<ReviewThread>> {
        let variables = json!({ "owner": owner, "repo": repo, "number": number });
        let value = self.graphql(REVIEW_THREADS_QUERY, Some(variables))?;
        parse_review_threads(&value)
    }

    /// Resolves or unresolves a review thread.
    pub fn set_review_thread_resolved(
        &self,
        thread_id: &str,
        resolved: bool,
    ) -> Result<ReviewThreadState> {
        if thread_id.trim().is_empty() {
            return Err(Error::Message("review thread id is empty".to_owned()));
        }
        let mutation = if resolved {
            RESOLVE_REVIEW_THREAD_MUTATION
        } else {
            UNRESOLVE_REVIEW_THREAD_MUTATION
        };
        let value = self.graphql(mutation, Some(json!({ "threadId": thread_id })))?;
        parse_review_thread_state(&value)
    }

    /// Returns a release by tag.
    pub fn release_by_tag(&self, owner: &str, repo: &str, tag: &str) -> Result<Release> {
        let tag = encode_path_segment(tag);
        self.parse(self.request(
            "GET",
            &format!(
                "/repos/{}/{}/releases/tags/{tag}",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            None,
        )?)
    }

    /// Lists assets attached to a release.
    pub fn list_release_assets(
        &self,
        owner: &str,
        repo: &str,
        release_id: u64,
    ) -> Result<Vec<ReleaseAsset>> {
        self.parse(self.request(
            "GET",
            &format!(
                "/repos/{}/{}/releases/{release_id}/assets?per_page=100",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            None,
        )?)
    }

    /// Uploads an asset to a release.
    pub fn upload_release_asset(
        &self,
        owner: &str,
        repo: &str,
        release_id: u64,
        options: &ReleaseAssetOptions<'_>,
    ) -> Result<ReleaseAsset> {
        let mut url = format!(
            "https://uploads.github.com/repos/{}/{}/releases/{release_id}/assets?name={}",
            validate_name(owner, "owner")?,
            validate_name(repo, "repository")?,
            encode_path_segment(options.name)
        );
        if let Some(label) = options.label {
            url.push_str("&label=");
            url.push_str(&encode_path_segment(label));
        }
        let response = self
            .agent
            .request("POST", &url)
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Accept", "application/vnd.github+json")
            .set("X-GitHub-Api-Version", API_VERSION)
            .set("Content-Type", options.content_type)
            .send_bytes(options.bytes)
            .map_err(|error| github_error(&url, error))?;
        let value = read_json(response)?;
        self.parse(value)
    }

    /// Deletes a release asset.
    pub fn delete_release_asset(&self, owner: &str, repo: &str, asset_id: u64) -> Result<()> {
        self.request(
            "DELETE",
            &format!(
                "/repos/{}/{}/releases/assets/{asset_id}",
                validate_name(owner, "owner")?,
                validate_name(repo, "repository")?
            ),
            None,
        )?;
        Ok(())
    }

    fn request(&self, method: &str, path: &str, body: Option<Value>) -> Result<Value> {
        Ok(self.request_internal(method, path, body)?.0)
    }

    fn request_internal(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> Result<(Value, ResponseHeaders)> {
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
        let headers = ResponseHeaders {
            token_scopes: parse_scope_header(response.header("x-oauth-scopes")),
            accepted_scopes: parse_scope_header(response.header("x-accepted-oauth-scopes")),
        };
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(MAX_BODY)
            .read_to_end(&mut bytes)
            .map_err(|error| Error::io("failed to read GitHub response", error))?;
        if bytes.is_empty() {
            return Ok((Value::Null, headers));
        }
        let value = serde_json::from_slice(&bytes)
            .map_err(|error| Error::corrupt(format!("invalid GitHub response: {error}")))?;
        Ok((value, headers))
    }

    fn parse<T: for<'de> Deserialize<'de>>(&self, value: Value) -> Result<T> {
        serde_json::from_value(value)
            .map_err(|error| Error::corrupt(format!("invalid GitHub response: {error}")))
    }
}

#[derive(Debug, Default)]
struct ResponseHeaders {
    token_scopes: Vec<String>,
    accepted_scopes: Vec<String>,
}

fn parse_scope_header(value: Option<&str>) -> Vec<String> {
    value
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|scope| !scope.is_empty())
        .map(str::to_owned)
        .collect()
}

fn read_json(response: ureq::Response) -> Result<Value> {
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

fn parse_review_threads(value: &Value) -> Result<Vec<ReviewThread>> {
    let nodes = value
        .pointer("/data/repository/pullRequest/reviewThreads/nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::corrupt("GitHub GraphQL response is missing review threads"))?;

    let mut threads = Vec::with_capacity(nodes.len());
    for node in nodes {
        let id = node
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::corrupt("review thread is missing an id"))?
            .to_owned();
        let comments = node
            .pointer("/comments/nodes")
            .and_then(Value::as_array)
            .map(|nodes| {
                nodes
                    .iter()
                    .filter_map(|comment| {
                        Some(ReviewThreadComment {
                            id: comment.get("id")?.as_str()?.to_owned(),
                            author: comment
                                .pointer("/author/login")
                                .and_then(Value::as_str)
                                .map(str::to_owned),
                            body: comment
                                .get("body")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_owned(),
                            url: comment
                                .get("url")
                                .and_then(Value::as_str)
                                .map(str::to_owned),
                            created_at: comment
                                .get("createdAt")
                                .and_then(Value::as_str)
                                .map(str::to_owned),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        threads.push(ReviewThread {
            id,
            is_resolved: node
                .get("isResolved")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            is_outdated: node
                .get("isOutdated")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            path: node.get("path").and_then(Value::as_str).map(str::to_owned),
            line: node.get("line").and_then(Value::as_u64),
            comments,
        });
    }
    Ok(threads)
}

fn parse_review_thread_state(value: &Value) -> Result<ReviewThreadState> {
    let thread = value
        .pointer("/data/resolveReviewThread/thread")
        .or_else(|| value.pointer("/data/unresolveReviewThread/thread"))
        .ok_or_else(|| Error::corrupt("GitHub GraphQL response is missing the review thread"))?;
    Ok(ReviewThreadState {
        id: thread
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::corrupt("review thread is missing an id"))?
            .to_owned(),
        is_resolved: thread
            .get("isResolved")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn validate_collaborator_permission(permission: &str) -> Result<()> {
    if matches!(
        permission,
        "pull" | "triage" | "push" | "maintain" | "admin"
    ) {
        Ok(())
    } else {
        Err(Error::Message(format!(
            "invalid collaborator permission '{permission}'; expected pull, triage, push, maintain or admin"
        )))
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
        assert!(validate_collaborator_permission("push").is_ok());
        assert!(validate_collaborator_permission("owner").is_err());
    }

    #[test]
    fn parses_scope_headers() {
        assert_eq!(
            parse_scope_header(Some("repo, workflow, read:org")),
            vec![
                "repo".to_owned(),
                "workflow".to_owned(),
                "read:org".to_owned()
            ]
        );
        assert!(parse_scope_header(None).is_empty());
    }

    #[test]
    fn parses_review_threads() {
        let value = json!({
            "data": {
                "repository": {
                    "pullRequest": {
                        "reviewThreads": {
                            "nodes": [{
                                "id": "thread-1",
                                "isResolved": false,
                                "isOutdated": true,
                                "path": "src/lib.rs",
                                "line": 42,
                                "comments": {
                                    "nodes": [{
                                        "id": "comment-1",
                                        "body": "please fix",
                                        "author": { "login": "reviewer" },
                                        "url": "https://example.test/comment",
                                        "createdAt": "2026-10-19T00:00:00Z"
                                    }]
                                }
                            }]
                        }
                    }
                }
            }
        });
        let threads = parse_review_threads(&value).expect("threads");
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "thread-1");
        assert_eq!(threads[0].line, Some(42));
        assert_eq!(threads[0].comments[0].author.as_deref(), Some("reviewer"));
    }

    #[test]
    fn parses_review_thread_mutation() {
        let resolved = json!({
            "data": { "resolveReviewThread": { "thread": { "id": "t", "isResolved": true } } }
        });
        assert_eq!(
            parse_review_thread_state(&resolved).unwrap(),
            ReviewThreadState {
                id: "t".to_owned(),
                is_resolved: true
            }
        );
        let unresolved = json!({
            "data": { "unresolveReviewThread": { "thread": { "id": "t", "isResolved": false } } }
        });
        assert!(!parse_review_thread_state(&unresolved).unwrap().is_resolved);
    }
}
