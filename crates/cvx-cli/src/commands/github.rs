use std::io::Read;
use std::time::{Duration, Instant};

use cvx_core::{Error, Result};
use cvx_credential::{Credential, CredentialBackend, CredentialStore};
use cvx_github_api::{DevicePoll, GitHubClient, poll_device_token, request_device_code};
use cvx_github_workflow::GitHubWorkflows;
use cvx_repository::Repository;
use serde_json::Value;

use crate::cli::{
    ChecksArgs, ChecksCommand, GithubArgs, GithubCollaboratorCommand, GithubCommand,
    GithubIssueCommand, GithubLabelCommand, GithubMilestoneCommand, GithubPrCommand,
    GithubReleaseCommand, GithubRepoCommand, GraphqlArgs, LoginArgs, ScopesArgs,
};

/// Executes `cvx github`.
pub fn run(args: GithubArgs) -> Result<()> {
    match args.command {
        GithubCommand::Login(args) => login(args),
        GithubCommand::Status => status(),
        GithubCommand::Logout => logout(),
        GithubCommand::Scopes(args) => scopes(args),
        GithubCommand::Graphql(args) => graphql(args),
        GithubCommand::Repo { command } => with_workflows(|workflows| match command {
            GithubRepoCommand::Create {
                name,
                private,
                description,
                push,
                branch,
            } => {
                if branch.is_some() && !push {
                    return Err(Error::Message("--branch requires --push".to_owned()));
                }
                let repository = if push {
                    workflows.create_repo_and_push(
                        &name,
                        private,
                        description.as_deref(),
                        branch.as_deref(),
                    )?
                } else {
                    workflows.create_repo(&name, private, description.as_deref())?
                };
                println!(
                    "Created {} ({}): {}",
                    repository.full_name,
                    if repository.private {
                        "private"
                    } else {
                        "public"
                    },
                    repository.html_url
                );
                if push {
                    println!("Pushed current branch to {}", repository.clone_url);
                }
                Ok(())
            }
            GithubRepoCommand::Delete { repo, yes } => {
                if !yes {
                    return Err(Error::Message(
                        "refusing to delete a repository without --yes".to_owned(),
                    ));
                }
                let (owner, name) = workflows.delete_repo(repo.as_deref())?;
                println!("Deleted {owner}/{name}");
                Ok(())
            }
            GithubRepoCommand::Show { repo } => {
                let repository = workflows.repo(repo.as_deref())?;
                println!("{}", repository.full_name);
                println!(
                    "visibility: {}",
                    if repository.private {
                        "private"
                    } else {
                        "public"
                    }
                );
                println!("default branch: {}", repository.default_branch);
                println!("clone: {}", repository.clone_url);
                println!("ssh: {}", repository.ssh_url);
                println!("url: {}", repository.html_url);
                Ok(())
            }
        }),
        GithubCommand::Issue { command } => with_workflows(|workflows| match command {
            GithubIssueCommand::Create {
                title,
                body,
                assignees,
                milestone,
                repo,
            } => {
                let issue = workflows.create_issue(
                    repo.as_deref(),
                    &title,
                    body.as_deref(),
                    &assignees,
                    milestone,
                )?;
                println!(
                    "#{} [{}] {} {}",
                    issue.number, issue.state, issue.title, issue.html_url
                );
                Ok(())
            }
            GithubIssueCommand::List { state, repo } => {
                for issue in workflows.list_issues(repo.as_deref(), &state)? {
                    let assignees: Vec<&str> = issue
                        .assignees
                        .iter()
                        .map(|owner| owner.login.as_str())
                        .collect();
                    let milestone = issue
                        .milestone
                        .as_ref()
                        .map(|milestone| format!(" milestone={}", milestone.title))
                        .unwrap_or_default();
                    let assignees = if assignees.is_empty() {
                        String::new()
                    } else {
                        format!(" @{}", assignees.join(", @"))
                    };
                    println!(
                        "#{:<6} [{}] {}{}{} {}",
                        issue.number,
                        issue.state,
                        issue.title,
                        assignees,
                        milestone,
                        issue.html_url
                    );
                }
                Ok(())
            }
            GithubIssueCommand::Close { number, repo } => {
                let issue = workflows.close_issue(repo.as_deref(), number)?;
                println!("#{} closed: {}", issue.number, issue.html_url);
                Ok(())
            }
            GithubIssueCommand::Assign {
                number,
                assignees,
                repo,
            } => {
                let issue = workflows.assign_issue(repo.as_deref(), number, &assignees)?;
                let names: Vec<&str> = issue
                    .assignees
                    .iter()
                    .map(|owner| owner.login.as_str())
                    .collect();
                println!("#{} assignees: {}", number, names.join(", "));
                Ok(())
            }
            GithubIssueCommand::Unassign {
                number,
                assignees,
                repo,
            } => {
                let issue = workflows.unassign_issue(repo.as_deref(), number, &assignees)?;
                let names: Vec<&str> = issue
                    .assignees
                    .iter()
                    .map(|owner| owner.login.as_str())
                    .collect();
                println!("#{} assignees: {}", number, names.join(", "));
                Ok(())
            }
        }),
        GithubCommand::Pr { command } => with_workflows(|workflows| match command {
            GithubPrCommand::Create {
                title,
                head,
                base,
                body,
                repo,
            } => {
                let pull = workflows.create_pull_request(
                    repo.as_deref(),
                    &title,
                    &head,
                    base.as_deref(),
                    body.as_deref(),
                )?;
                println!(
                    "#{} [{}] {} -> {} {}",
                    pull.number, pull.state, pull.head.r#ref, pull.base.r#ref, pull.html_url
                );
                Ok(())
            }
            GithubPrCommand::List { state, repo } => {
                for pull in workflows.list_pull_requests(repo.as_deref(), &state)? {
                    println!(
                        "#{:<6} [{}] {} -> {} {}",
                        pull.number, pull.state, pull.head.r#ref, pull.base.r#ref, pull.html_url
                    );
                }
                Ok(())
            }
            GithubPrCommand::Merge {
                number,
                method,
                repo,
            } => {
                let result = workflows.merge_pull_request(repo.as_deref(), number, &method)?;
                println!(
                    "merged: {} {}",
                    result.merged,
                    result.sha.unwrap_or(result.message)
                );
                Ok(())
            }
            GithubPrCommand::Reviews { number, repo } => {
                for review in workflows.list_reviews(repo.as_deref(), number)? {
                    println!(
                        "{:<18} {:<20} {} {}",
                        review.state,
                        review.user.login,
                        review.body.unwrap_or_default(),
                        review.html_url.unwrap_or_default()
                    );
                }
                Ok(())
            }
            GithubPrCommand::Review {
                number,
                approve,
                request_changes,
                comment,
                body,
                repo,
            } => {
                let event = if approve {
                    "APPROVE"
                } else if request_changes {
                    "REQUEST_CHANGES"
                } else if comment {
                    "COMMENT"
                } else {
                    return Err(Error::Message(
                        "choose --approve, --request-changes or --comment".to_owned(),
                    ));
                };
                let review =
                    workflows.create_review(repo.as_deref(), number, event, body.as_deref())?;
                println!(
                    "{} review submitted for PR #{}: {}",
                    review.state,
                    number,
                    review.html_url.unwrap_or_default()
                );
                Ok(())
            }
            GithubPrCommand::Threads { number, repo } => {
                for thread in workflows.review_threads(repo.as_deref(), number)? {
                    let location = match (&thread.path, thread.line) {
                        (Some(path), Some(line)) => format!("{path}:{line}"),
                        (Some(path), None) => path.clone(),
                        _ => "-".to_owned(),
                    };
                    println!(
                        "{} resolved={} outdated={} {}",
                        thread.id, thread.is_resolved, thread.is_outdated, location
                    );
                    for comment in &thread.comments {
                        println!(
                            "  @{}: {}",
                            comment.author.as_deref().unwrap_or("unknown"),
                            comment.body.replace('\n', " ")
                        );
                    }
                }
                Ok(())
            }
            GithubPrCommand::Resolve { thread_id } => {
                let state = workflows.set_review_thread_resolved(&thread_id, true)?;
                println!("{} resolved={}", state.id, state.is_resolved);
                Ok(())
            }
            GithubPrCommand::Unresolve { thread_id } => {
                let state = workflows.set_review_thread_resolved(&thread_id, false)?;
                println!("{} resolved={}", state.id, state.is_resolved);
                Ok(())
            }
        }),
        GithubCommand::Label { command } => with_workflows(|workflows| match command {
            GithubLabelCommand::List { repo } => {
                for label in workflows.labels(repo.as_deref())? {
                    println!(
                        "{:<24} #{:<6} {}",
                        label.name,
                        label.color,
                        label.description.unwrap_or_default()
                    );
                }
                Ok(())
            }
            GithubLabelCommand::Create {
                name,
                color,
                description,
                repo,
            } => {
                let label = workflows.create_label(
                    repo.as_deref(),
                    &name,
                    &color,
                    description.as_deref(),
                )?;
                println!("Created label {} (#{})", label.name, label.color);
                Ok(())
            }
            GithubLabelCommand::Delete { name, repo } => {
                workflows.delete_label(repo.as_deref(), &name)?;
                println!("Deleted label {name}");
                Ok(())
            }
            GithubLabelCommand::Add {
                number,
                labels,
                repo,
            } => {
                let labels = workflows.add_labels(repo.as_deref(), number, &labels)?;
                let names: Vec<&str> = labels.iter().map(|label| label.name.as_str()).collect();
                println!("#{} labels: {}", number, names.join(", "));
                Ok(())
            }
            GithubLabelCommand::Remove {
                number,
                label,
                repo,
            } => {
                let labels = workflows.remove_label(repo.as_deref(), number, &label)?;
                let names: Vec<&str> = labels.iter().map(|label| label.name.as_str()).collect();
                println!("#{} labels: {}", number, names.join(", "));
                Ok(())
            }
        }),
        GithubCommand::Collaborator { command } => with_workflows(|workflows| match command {
            GithubCollaboratorCommand::List { repo } => {
                for collaborator in workflows.collaborators(repo.as_deref())? {
                    let permissions = collaborator.permissions.as_ref();
                    let permission = permissions
                        .map(|permissions| {
                            if permissions.admin {
                                "admin"
                            } else if permissions.maintain {
                                "maintain"
                            } else if permissions.push {
                                "push"
                            } else if permissions.triage {
                                "triage"
                            } else {
                                "pull"
                            }
                        })
                        .unwrap_or("-");
                    println!("{:<32} {}", collaborator.login, permission);
                }
                Ok(())
            }
            GithubCollaboratorCommand::Add {
                user,
                permission,
                repo,
            } => {
                workflows.add_collaborator(repo.as_deref(), &user, &permission)?;
                println!("{user} added with {permission} permission");
                Ok(())
            }
            GithubCollaboratorCommand::Remove { user, repo } => {
                workflows.remove_collaborator(repo.as_deref(), &user)?;
                println!("Removed collaborator {user}");
                Ok(())
            }
            GithubCollaboratorCommand::Permission { user, repo } => {
                let permission = workflows.collaborator_permission(repo.as_deref(), &user)?;
                println!("{}: {}", permission.user.login, permission.permission);
                Ok(())
            }
        }),
        GithubCommand::Milestone { command } => with_workflows(|workflows| match command {
            GithubMilestoneCommand::List { state, repo } => {
                for milestone in workflows.milestones(repo.as_deref(), &state)? {
                    println!(
                        "#{:<5} [{}] {} open={} closed={}",
                        milestone.number,
                        milestone.state,
                        milestone.title,
                        milestone.open_issues,
                        milestone.closed_issues
                    );
                }
                Ok(())
            }
            GithubMilestoneCommand::Create {
                title,
                description,
                due_on,
                repo,
            } => {
                let milestone = workflows.create_milestone(
                    repo.as_deref(),
                    &title,
                    description.as_deref(),
                    due_on.as_deref(),
                )?;
                println!(
                    "#{} [{}] {}",
                    milestone.number, milestone.state, milestone.title
                );
                Ok(())
            }
            GithubMilestoneCommand::Close { number, repo } => {
                let milestone = workflows.close_milestone(repo.as_deref(), number)?;
                println!("#{} closed: {}", milestone.number, milestone.title);
                Ok(())
            }
        }),
        GithubCommand::Checks(args) => checks(args),
        GithubCommand::Release { command } => with_workflows(|workflows| match command {
            GithubReleaseCommand::List { repo } => {
                for release in workflows.list_releases(repo.as_deref())? {
                    println!(
                        "{:<20} draft={:<5} prerelease={:<5} {}",
                        release.tag_name, release.draft, release.prerelease, release.html_url
                    );
                }
                Ok(())
            }
            GithubReleaseCommand::Create {
                tag,
                name,
                body,
                draft,
                prerelease,
                assets,
                repo,
            } => {
                let release = workflows.create_release(
                    repo.as_deref(),
                    &tag,
                    name.as_deref(),
                    body.as_deref(),
                    draft,
                    prerelease,
                )?;
                println!(
                    "Created release {} (draft={}, prerelease={}): {}",
                    release.tag_name, release.draft, release.prerelease, release.html_url
                );
                for asset in assets {
                    let uploaded = workflows.upload_release_asset(
                        repo.as_deref(),
                        &tag,
                        &asset,
                        None,
                        None,
                        false,
                    )?;
                    println!("Uploaded {} ({} bytes)", uploaded.name, uploaded.size);
                }
                Ok(())
            }
            GithubReleaseCommand::Assets { tag, repo } => {
                for asset in workflows.release_assets(repo.as_deref(), &tag)? {
                    println!(
                        "{:<10} {:<40} {:>12} {}",
                        asset.id, asset.name, asset.size, asset.browser_download_url
                    );
                }
                Ok(())
            }
            GithubReleaseCommand::Upload {
                tag,
                path,
                name,
                label,
                clobber,
                repo,
            } => {
                let asset = workflows.upload_release_asset(
                    repo.as_deref(),
                    &tag,
                    &path,
                    name.as_deref(),
                    label.as_deref(),
                    clobber,
                )?;
                println!(
                    "Uploaded {} ({} bytes): {}",
                    asset.name, asset.size, asset.browser_download_url
                );
                Ok(())
            }
            GithubReleaseCommand::DeleteAsset { asset_id, repo } => {
                workflows.delete_release_asset(repo.as_deref(), asset_id)?;
                println!("Deleted release asset {asset_id}");
                Ok(())
            }
        }),
        GithubCommand::Workflows { repo } => with_workflows(|workflows| {
            for run in workflows.workflow_runs(repo.as_deref())? {
                println!(
                    "{:<10} {:<12} {} {}",
                    run.status,
                    run.conclusion.unwrap_or_else(|| "-".to_owned()),
                    run.name.unwrap_or_else(|| "(unnamed)".to_owned()),
                    run.html_url
                );
            }
            Ok(())
        }),
    }
}

fn with_workflows<T>(operation: impl FnOnce(&GitHubWorkflows<'_>) -> Result<T>) -> Result<T> {
    let cwd = std::env::current_dir().map_err(|error| Error::io("current directory", error))?;
    let repository = Repository::discover(&cwd)?;
    let workflows = GitHubWorkflows::new(&repository)?;
    operation(&workflows)
}

fn login(args: LoginArgs) -> Result<()> {
    if args.oauth {
        login_oauth(args)
    } else {
        login_token()
    }
}

fn login_token() -> Result<()> {
    let mut token = String::new();
    std::io::stdin()
        .read_to_string(&mut token)
        .map_err(|error| Error::io("failed to read GitHub token", error))?;
    let token = token.trim().to_owned();
    if token.is_empty() {
        return Err(Error::Message(
            "no GitHub token was provided on standard input".to_owned(),
        ));
    }

    store_token(&token)?;
    let user = GitHubClient::new(&token).current_user()?;
    println!("Authenticated as {}", user.login);
    Ok(())
}

fn login_oauth(args: LoginArgs) -> Result<()> {
    let client_id = args
        .client_id
        .or_else(|| std::env::var("CVX_GITHUB_CLIENT_ID").ok())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            Error::Message(
                "OAuth client id is required; set CVX_GITHUB_CLIENT_ID or pass --client-id"
                    .to_owned(),
            )
        })?;
    let scopes = if args.scopes.is_empty() {
        vec![
            "repo".to_owned(),
            "workflow".to_owned(),
            "read:org".to_owned(),
        ]
    } else {
        args.scopes
    };

    let code = request_device_code(&client_id, &scopes)?;
    println!(
        "Open {} and enter code {}",
        code.verification_uri, code.user_code
    );
    println!("Waiting for authorization...");

    let mut interval = code.interval.max(1);
    let deadline = Instant::now() + Duration::from_secs(code.expires_in);
    loop {
        if Instant::now() >= deadline {
            return Err(Error::Message("GitHub device code expired".to_owned()));
        }
        std::thread::sleep(Duration::from_secs(interval));
        match poll_device_token(&client_id, &code.device_code)? {
            DevicePoll::Pending => {}
            DevicePoll::SlowDown => interval = interval.saturating_add(5),
            DevicePoll::Authorized(token) => {
                store_token(&token.access_token)?;
                let user = GitHubClient::new(&token.access_token).current_user()?;
                println!(
                    "Authenticated as {} with scopes: {}",
                    user.login,
                    token.scopes.join(", ")
                );
                return Ok(());
            }
            DevicePoll::AccessDenied => {
                return Err(Error::Message(
                    "GitHub OAuth authorization was denied".to_owned(),
                ));
            }
            DevicePoll::Expired => {
                return Err(Error::Message("GitHub device code expired".to_owned()));
            }
        }
    }
}

fn store_token(token: &str) -> Result<()> {
    let store = CredentialStore::open_default()?;
    let backend = store.store(
        "github.com",
        &Credential {
            username: "x-access-token".to_owned(),
            password: token.to_owned(),
        },
    )?;
    let location = match backend {
        CredentialBackend::Keychain => "the operating-system keychain".to_owned(),
        CredentialBackend::File => store.path().display().to_string(),
    };
    println!("Stored GitHub credential in {location}");
    Ok(())
}

fn scopes(args: ScopesArgs) -> Result<()> {
    let client = GitHubClient::from_credentials()?;
    let scopes = client.auth_scopes()?;
    println!("token scopes: {}", scopes.token.join(", "));
    println!("accepted scopes: {}", scopes.accepted.join(", "));
    for required in args.require {
        if !scopes.token.iter().any(|scope| scope == &required) {
            return Err(Error::Message(format!(
                "GitHub token is missing required scope '{required}'"
            )));
        }
    }
    Ok(())
}

fn graphql(args: GraphqlArgs) -> Result<()> {
    let variables = match (args.variables, args.variables_file) {
        (Some(value), None) => {
            Some(serde_json::from_str::<Value>(&value).map_err(|error| {
                Error::Message(format!("invalid GraphQL variables JSON: {error}"))
            })?)
        }
        (None, Some(path)) => {
            let text = std::fs::read_to_string(&path)
                .map_err(|error| Error::io(path.display().to_string(), error))?;
            Some(serde_json::from_str::<Value>(&text).map_err(|error| {
                Error::Message(format!(
                    "invalid GraphQL variables JSON in {}: {error}",
                    path.display()
                ))
            })?)
        }
        (None, None) => None,
        (Some(_), Some(_)) => unreachable!("clap conflicts_with prevents both"),
    };
    if variables.as_ref().is_some_and(|value| !value.is_object()) {
        return Err(Error::Message(
            "GraphQL variables must be a JSON object".to_owned(),
        ));
    }

    let value = with_workflows(|workflows| workflows.graphql(&args.query, variables))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&value).map_err(|error| Error::Message(format!(
            "failed to render GraphQL response: {error}"
        )))?
    );
    Ok(())
}

fn checks(args: ChecksArgs) -> Result<()> {
    with_workflows(|workflows| match args.command {
        Some(ChecksCommand::Required { branch, repo }) => {
            let checks = workflows.required_status_checks(repo.as_deref(), &branch)?;
            println!("branch: {branch}");
            println!("strict: {}", checks.strict);
            if checks.contexts.is_empty() {
                println!("required checks: none");
            } else {
                println!("required checks:");
                for context in &checks.contexts {
                    println!("  {context}");
                }
            }
            Ok(())
        }
        Some(ChecksCommand::Require {
            contexts,
            branch,
            strict,
            repo,
        }) => {
            let checks = workflows.set_required_status_checks(
                repo.as_deref(),
                &branch,
                &contexts,
                strict,
            )?;
            println!(
                "branch {} now requires: {}",
                branch,
                checks.contexts.join(", ")
            );
            Ok(())
        }
        None => {
            for check in workflows.check_runs(args.repo.as_deref(), args.reference.as_deref())? {
                println!(
                    "{:<12} {:<14} {:<30} {}",
                    check.status,
                    check.conclusion.unwrap_or_else(|| "-".to_owned()),
                    check.name,
                    check.html_url.unwrap_or_default()
                );
            }
            Ok(())
        }
    })
}

fn status() -> Result<()> {
    let client = cvx_github_api::GitHubClient::from_credentials()?;
    let user = client.current_user()?;
    println!("Authenticated as {}", user.login);
    if let Some(name) = user.name {
        println!("Name: {name}");
    }
    println!("User id: {}", user.id);
    Ok(())
}

fn logout() -> Result<()> {
    let store = CredentialStore::open_default()?;
    let mut removed = false;
    for host in ["github.com", "api.github.com"] {
        removed |= store.remove(host)?;
    }
    if removed {
        println!("Removed stored GitHub credential");
    } else {
        println!("No stored GitHub credential was found");
    }
    Ok(())
}
