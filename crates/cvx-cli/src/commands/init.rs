use cvx_core::Result;
use cvx_repository::{InitOptions, Repository, RepositoryKind};

use crate::cli::InitArgs;

/// Executes `cvx init`.
pub fn run(args: InitArgs) -> Result<()> {
    let outcome = Repository::init(InitOptions {
        directory: args.directory,
        initial_branch: args.initial_branch,
        storage: args.storage.into(),
        quiet: args.quiet,
    })?;

    if args.quiet {
        return Ok(());
    }

    let repository = outcome.repository();
    let control_dir = display_control_dir(repository);

    if outcome.was_existing_git() {
        println!(
            "Detected existing Git repository in {control_dir}; CVX will use it in compatibility mode"
        );
    } else if outcome.was_created() {
        match repository.kind() {
            RepositoryKind::Cvx => {
                println!("Initialized empty CVX repository in {control_dir}");
            }
            RepositoryKind::Git => {
                println!("Initialized empty Git repository in {control_dir}");
            }
        }
    } else {
        match repository.kind() {
            RepositoryKind::Cvx => {
                println!("Reinitialized existing CVX repository in {control_dir}");
            }
            RepositoryKind::Git => {
                println!("Reinitialized existing Git repository in {control_dir}");
            }
        }
    }

    Ok(())
}

fn display_control_dir(repository: &Repository) -> String {
    let mut path = repository.control_dir().display().to_string();
    if !path.ends_with(std::path::MAIN_SEPARATOR) {
        path.push(std::path::MAIN_SEPARATOR);
    }
    path
}
