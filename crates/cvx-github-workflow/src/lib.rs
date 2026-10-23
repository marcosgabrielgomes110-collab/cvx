//! High-level GitHub workflows built on the REST client and local repository.

#![forbid(unsafe_code)]

use std::path::Path;

use cvx_core::{Error, Result};
use cvx_github_api::{
    AuthScopes, CheckRun, Collaborator, CollaboratorPermission, GitHubClient, Issue, IssueOptions,
    Label, MergeResult, Milestone, PullRequest, Release, ReleaseAsset, ReleaseAssetOptions,
    ReleaseOptions, Repository, RequiredStatusChecks, Review, ReviewThread, ReviewThreadState,
    User, WorkflowRun, parse_repository_slug,
};
use cvx_repository::{PushOptions, Repository as LocalRepository};
use serde_json::Value;

/// GitHub operations bound to a local repository.
pub struct GitHubWorkflows<'a> {
    repository: &'a LocalRepository,
    client: GitHubClient,
    remote: String,
}

impl<'a> GitHubWorkflows<'a> {
    /// Creates workflows using credentials from the environment or store.
    pub fn new(repository: &'a LocalRepository) -> Result<Self> {
        Ok(Self {
            repository,
            client: GitHubClient::from_credentials()?,
            remote: "origin".to_owned(),
        })
    }

    /// Uses an explicit remote name.
    pub fn with_remote(mut self, remote: &str) -> Self {
        self.remote = remote.to_owned();
        self
    }

    /// Returns the authenticated user.
    pub fn user(&self) -> Result<User> {
        self.client.current_user()
    }

    /// Resolves `owner/repo` from an explicit value or the local remote.
    pub fn slug(&self, explicit: Option<&str>) -> Result<(String, String)> {
        if let Some(explicit) = explicit {
            return parse_repository_slug(explicit);
        }
        let url = self.repository.remote_url(&self.remote)?.ok_or_else(|| {
            Error::Message(format!(
                "remote '{}' is not configured; pass --repo owner/name",
                self.remote
            ))
        })?;
        parse_repository_slug(&url)
    }

    /// Creates a repository without pushing.
    pub fn create_repo(
        &self,
        name: &str,
        private: bool,
        description: Option<&str>,
    ) -> Result<Repository> {
        self.client.create_repository(name, private, description)
    }

    /// Creates a repository and pushes the current branch to it.
    pub fn create_repo_and_push(
        &self,
        name: &str,
        private: bool,
        description: Option<&str>,
        branch: Option<&str>,
    ) -> Result<Repository> {
        let created = self.client.create_repository(name, private, description)?;
        self.repository
            .set_remote_url(&self.remote, &created.clone_url)?;
        self.repository.push(&PushOptions {
            remote: Some(self.remote.clone()),
            branch: branch.map(str::to_owned),
            force: false,
            force_with_lease: false,
        })?;
        Ok(created)
    }

    /// Returns repository information.
    pub fn repo(&self, explicit: Option<&str>) -> Result<Repository> {
        let (owner, repo) = self.slug(explicit)?;
        self.client.repository(&owner, &repo)
    }

    /// Deletes a repository.
    pub fn delete_repo(&self, explicit: Option<&str>) -> Result<(String, String)> {
        let (owner, repo) = self.slug(explicit)?;
        self.client.delete_repository(&owner, &repo)?;
        Ok((owner, repo))
    }

    /// Creates an issue.
    pub fn create_issue(
        &self,
        explicit: Option<&str>,
        title: &str,
        body: Option<&str>,
        assignees: &[String],
        milestone: Option<u64>,
    ) -> Result<Issue> {
        let (owner, repo) = self.slug(explicit)?;
        self.client.create_issue(
            &owner,
            &repo,
            &IssueOptions {
                title,
                body,
                assignees,
                milestone,
            },
        )
    }

    /// Lists issues.
    pub fn list_issues(&self, explicit: Option<&str>, state: &str) -> Result<Vec<Issue>> {
        let (owner, repo) = self.slug(explicit)?;
        self.client.list_issues(&owner, &repo, state)
    }

    /// Closes an issue.
    pub fn close_issue(&self, explicit: Option<&str>, number: u64) -> Result<Issue> {
        let (owner, repo) = self.slug(explicit)?;
        self.client.close_issue(&owner, &repo, number)
    }

    /// Adds assignees to an issue or pull request.
    pub fn assign_issue(
        &self,
        explicit: Option<&str>,
        number: u64,
        assignees: &[String],
    ) -> Result<Issue> {
        let (owner, repo) = self.slug(explicit)?;
        self.client.add_assignees(&owner, &repo, number, assignees)
    }

    /// Removes assignees from an issue or pull request.
    pub fn unassign_issue(
        &self,
        explicit: Option<&str>,
        number: u64,
        assignees: &[String],
    ) -> Result<Issue> {
        let (owner, repo) = self.slug(explicit)?;
        self.client
            .remove_assignees(&owner, &repo, number, assignees)
    }

    /// Lists milestones.
    pub fn milestones(&self, explicit: Option<&str>, state: &str) -> Result<Vec<Milestone>> {
        let (owner, repo) = self.slug(explicit)?;
        self.client.list_milestones(&owner, &repo, state)
    }

    /// Creates a milestone.
    pub fn create_milestone(
        &self,
        explicit: Option<&str>,
        title: &str,
        description: Option<&str>,
        due_on: Option<&str>,
    ) -> Result<Milestone> {
        let (owner, repo) = self.slug(explicit)?;
        self.client
            .create_milestone(&owner, &repo, title, description, due_on)
    }

    /// Closes a milestone.
    pub fn close_milestone(&self, explicit: Option<&str>, number: u64) -> Result<Milestone> {
        let (owner, repo) = self.slug(explicit)?;
        self.client.close_milestone(&owner, &repo, number)
    }

    /// Lists repository collaborators.
    pub fn collaborators(&self, explicit: Option<&str>) -> Result<Vec<Collaborator>> {
        let (owner, repo) = self.slug(explicit)?;
        self.client.list_collaborators(&owner, &repo)
    }

    /// Returns a collaborator's permission.
    pub fn collaborator_permission(
        &self,
        explicit: Option<&str>,
        user: &str,
    ) -> Result<CollaboratorPermission> {
        let (owner, repo) = self.slug(explicit)?;
        self.client.collaborator_permission(&owner, &repo, user)
    }

    /// Adds or updates a collaborator.
    pub fn add_collaborator(
        &self,
        explicit: Option<&str>,
        user: &str,
        permission: &str,
    ) -> Result<()> {
        let (owner, repo) = self.slug(explicit)?;
        self.client
            .add_collaborator(&owner, &repo, user, permission)
    }

    /// Removes a collaborator.
    pub fn remove_collaborator(&self, explicit: Option<&str>, user: &str) -> Result<()> {
        let (owner, repo) = self.slug(explicit)?;
        self.client.remove_collaborator(&owner, &repo, user)
    }

    /// Returns OAuth scopes for the configured token.
    pub fn auth_scopes(&self) -> Result<AuthScopes> {
        self.client.auth_scopes()
    }

    /// Executes a GraphQL query.
    pub fn graphql(&self, query: &str, variables: Option<Value>) -> Result<Value> {
        self.client.graphql(query, variables)
    }

    /// Lists review threads for a pull request.
    pub fn review_threads(&self, explicit: Option<&str>, number: u64) -> Result<Vec<ReviewThread>> {
        let (owner, repo) = self.slug(explicit)?;
        self.client.review_threads(&owner, &repo, number)
    }

    /// Resolves or unresolves a review thread.
    pub fn set_review_thread_resolved(
        &self,
        thread_id: &str,
        resolved: bool,
    ) -> Result<ReviewThreadState> {
        self.client.set_review_thread_resolved(thread_id, resolved)
    }

    /// Returns required status checks for a branch.
    pub fn required_status_checks(
        &self,
        explicit: Option<&str>,
        branch: &str,
    ) -> Result<RequiredStatusChecks> {
        let (owner, repo) = self.slug(explicit)?;
        self.client.required_status_checks(&owner, &repo, branch)
    }

    /// Replaces required status checks for a branch.
    pub fn set_required_status_checks(
        &self,
        explicit: Option<&str>,
        branch: &str,
        contexts: &[String],
        strict: bool,
    ) -> Result<RequiredStatusChecks> {
        let (owner, repo) = self.slug(explicit)?;
        self.client
            .set_required_status_checks(&owner, &repo, branch, contexts, strict)
    }

    /// Creates a pull request against the repository's default branch.
    pub fn create_pull_request(
        &self,
        explicit: Option<&str>,
        title: &str,
        head: &str,
        base: Option<&str>,
        body: Option<&str>,
    ) -> Result<PullRequest> {
        let (owner, repo) = self.slug(explicit)?;
        let base = match base {
            Some(base) => base.to_owned(),
            None => self.client.repository(&owner, &repo)?.default_branch,
        };
        self.client
            .create_pull_request(&owner, &repo, title, head, &base, body)
    }

    /// Lists pull requests.
    pub fn list_pull_requests(
        &self,
        explicit: Option<&str>,
        state: &str,
    ) -> Result<Vec<PullRequest>> {
        let (owner, repo) = self.slug(explicit)?;
        self.client.list_pull_requests(&owner, &repo, state)
    }

    /// Lists reviews for a pull request.
    pub fn list_reviews(&self, explicit: Option<&str>, number: u64) -> Result<Vec<Review>> {
        let (owner, repo) = self.slug(explicit)?;
        self.client.list_pull_request_reviews(&owner, &repo, number)
    }

    /// Creates a pull-request review.
    pub fn create_review(
        &self,
        explicit: Option<&str>,
        number: u64,
        event: &str,
        body: Option<&str>,
    ) -> Result<Review> {
        let (owner, repo) = self.slug(explicit)?;
        self.client
            .create_pull_request_review(&owner, &repo, number, event, body)
    }

    /// Lists check runs for a commit, ref or the local `HEAD`.
    pub fn check_runs(
        &self,
        explicit: Option<&str>,
        reference: Option<&str>,
    ) -> Result<Vec<CheckRun>> {
        let (owner, repo) = self.slug(explicit)?;
        let reference = match reference {
            Some(reference) => reference.to_owned(),
            None => self
                .repository
                .head()?
                .oid()
                .ok_or_else(|| Error::Message("HEAD has no commits".to_owned()))?
                .to_hex(),
        };
        self.client.list_check_runs(&owner, &repo, &reference)
    }

    /// Lists labels.
    pub fn labels(&self, explicit: Option<&str>) -> Result<Vec<Label>> {
        let (owner, repo) = self.slug(explicit)?;
        self.client.list_labels(&owner, &repo)
    }

    /// Creates a label.
    pub fn create_label(
        &self,
        explicit: Option<&str>,
        name: &str,
        color: &str,
        description: Option<&str>,
    ) -> Result<Label> {
        let (owner, repo) = self.slug(explicit)?;
        self.client
            .create_label(&owner, &repo, name, color, description)
    }

    /// Deletes a label.
    pub fn delete_label(&self, explicit: Option<&str>, name: &str) -> Result<()> {
        let (owner, repo) = self.slug(explicit)?;
        self.client.delete_label(&owner, &repo, name)
    }

    /// Adds labels to an issue or pull request.
    pub fn add_labels(
        &self,
        explicit: Option<&str>,
        number: u64,
        labels: &[String],
    ) -> Result<Vec<Label>> {
        let (owner, repo) = self.slug(explicit)?;
        self.client.add_labels(&owner, &repo, number, labels)
    }

    /// Removes a label from an issue or pull request.
    pub fn remove_label(
        &self,
        explicit: Option<&str>,
        number: u64,
        label: &str,
    ) -> Result<Vec<Label>> {
        let (owner, repo) = self.slug(explicit)?;
        self.client.remove_label(&owner, &repo, number, label)
    }

    /// Merges a pull request.
    pub fn merge_pull_request(
        &self,
        explicit: Option<&str>,
        number: u64,
        method: &str,
    ) -> Result<MergeResult> {
        let (owner, repo) = self.slug(explicit)?;
        self.client
            .merge_pull_request(&owner, &repo, number, method)
    }

    /// Creates a release.
    pub fn create_release(
        &self,
        explicit: Option<&str>,
        tag: &str,
        name: Option<&str>,
        body: Option<&str>,
        draft: bool,
        prerelease: bool,
    ) -> Result<Release> {
        let (owner, repo) = self.slug(explicit)?;
        self.client.create_release(
            &owner,
            &repo,
            &ReleaseOptions {
                tag,
                name,
                body,
                draft,
                prerelease,
            },
        )
    }

    /// Lists releases.
    pub fn list_releases(&self, explicit: Option<&str>) -> Result<Vec<Release>> {
        let (owner, repo) = self.slug(explicit)?;
        self.client.list_releases(&owner, &repo)
    }

    /// Lists assets attached to a release tag.
    pub fn release_assets(&self, explicit: Option<&str>, tag: &str) -> Result<Vec<ReleaseAsset>> {
        let (owner, repo) = self.slug(explicit)?;
        let release = self.client.release_by_tag(&owner, &repo, tag)?;
        self.client.list_release_assets(&owner, &repo, release.id)
    }

    /// Uploads a local file as a release asset.
    pub fn upload_release_asset(
        &self,
        explicit: Option<&str>,
        tag: &str,
        path: &Path,
        name: Option<&str>,
        label: Option<&str>,
        clobber: bool,
    ) -> Result<ReleaseAsset> {
        let (owner, repo) = self.slug(explicit)?;
        let release = self.client.release_by_tag(&owner, &repo, tag)?;
        let name = name
            .map(str::to_owned)
            .or_else(|| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .ok_or_else(|| Error::Message("release asset has no file name".to_owned()))?;
        let bytes = std::fs::read(path).map_err(|error| {
            Error::io(format!("failed to read asset {}", path.display()), error)
        })?;
        let content_type = content_type_for_path(path);

        if clobber {
            for asset in self.client.list_release_assets(&owner, &repo, release.id)? {
                if asset.name == name {
                    self.client.delete_release_asset(&owner, &repo, asset.id)?;
                }
            }
        }

        self.client.upload_release_asset(
            &owner,
            &repo,
            release.id,
            &ReleaseAssetOptions {
                name: &name,
                label,
                content_type,
                bytes: &bytes,
            },
        )
    }

    /// Deletes a release asset by id.
    pub fn delete_release_asset(&self, explicit: Option<&str>, asset_id: u64) -> Result<()> {
        let (owner, repo) = self.slug(explicit)?;
        self.client.delete_release_asset(&owner, &repo, asset_id)
    }

    /// Lists recent workflow runs.
    pub fn workflow_runs(&self, explicit: Option<&str>) -> Result<Vec<WorkflowRun>> {
        let (owner, repo) = self.slug(explicit)?;
        self.client.list_workflow_runs(&owner, &repo)
    }
}

fn content_type_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("txt") | Some("md") => "text/plain; charset=utf-8",
        Some("json") => "application/json",
        Some("toml") => "application/toml",
        Some("zip") => "application/zip",
        Some("gz") | Some("tgz") => "application/gzip",
        Some("tar") => "application/x-tar",
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("svg") => "image/svg+xml",
        Some("exe") => "application/vnd.microsoft.portable-executable",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_asset_content_types() {
        assert_eq!(
            content_type_for_path(Path::new("notes.md")),
            "text/plain; charset=utf-8"
        );
        assert_eq!(
            content_type_for_path(Path::new("cvx.tar.gz")),
            "application/gzip"
        );
        assert_eq!(
            content_type_for_path(Path::new("unknown.bin")),
            "application/octet-stream"
        );
    }
}
