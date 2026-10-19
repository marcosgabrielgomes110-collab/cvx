use std::io::Read;

use cvx_core::{Error, Result};
use cvx_credential::{Credential, CredentialBackend, CredentialStore};
use cvx_github_workflow::GitHubWorkflows;
use cvx_repository::Repository;

use crate::cli::{
    GithubArgs, GithubCommand, GithubIssueCommand, GithubLabelCommand, GithubPrCommand,
    GithubReleaseCommand, GithubRepoCommand,
};

/// Executes `cvx github`.
pub fn run(args: GithubArgs) -> Result<()> {
    match args.command {
        GithubCommand::Login => login(),
        GithubCommand::Status => status(),
        GithubCommand::Logout => logout(),
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
            GithubIssueCommand::Create { title, body, repo } => {
                let issue = workflows.create_issue(repo.as_deref(), &title, body.as_deref())?;
                println!(
                    "#{} [{}] {} {}",
                    issue.number, issue.state, issue.title, issue.html_url
                );
                Ok(())
            }
            GithubIssueCommand::List { state, repo } => {
                for issue in workflows.list_issues(repo.as_deref(), &state)? {
                    println!(
                        "#{:<6} [{}] {} {}",
                        issue.number, issue.state, issue.title, issue.html_url
                    );
                }
                Ok(())
            }
            GithubIssueCommand::Close { number, repo } => {
                let issue = workflows.close_issue(repo.as_deref(), number)?;
                println!("#{} closed: {}", issue.number, issue.html_url);
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
        GithubCommand::Checks { reference, repo } => with_workflows(|workflows| {
            for check in workflows.check_runs(repo.as_deref(), reference.as_deref())? {
                println!(
                    "{:<12} {:<14} {:<30} {}",
                    check.status,
                    check.conclusion.unwrap_or_else(|| "-".to_owned()),
                    check.name,
                    check.html_url.unwrap_or_default()
                );
            }
            Ok(())
        }),
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

fn login() -> Result<()> {
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

    let store = CredentialStore::open_default()?;
    let backend = store.store(
        "github.com",
        &Credential {
            username: "x-access-token".to_owned(),
            password: token,
        },
    )?;
    let location = match backend {
        CredentialBackend::Keychain => "the operating-system keychain".to_owned(),
        CredentialBackend::File => store.path().display().to_string(),
    };
    println!("Stored GitHub credential in {location}");
    Ok(())
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
