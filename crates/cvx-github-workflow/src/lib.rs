//! High-level GitHub workflows built on the REST client and local repository.

#![forbid(unsafe_code)]

use cvx_core::{Error, Result};
use cvx_github_api::{
    GitHubClient, Issue, MergeResult, PullRequest, Release, ReleaseOptions, Repository, User,
    WorkflowRun, parse_repository_slug,
};
use cvx_repository::{PushOptions, Repository as LocalRepository};

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
    ) -> Result<Issue> {
        let (owner, repo) = self.slug(explicit)?;
        self.client.create_issue(&owner, &repo, title, body)
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

    /// Lists recent workflow runs.
    pub fn workflow_runs(&self, explicit: Option<&str>) -> Result<Vec<WorkflowRun>> {
        let (owner, repo) = self.slug(explicit)?;
        self.client.list_workflow_runs(&owner, &repo)
    }
}
