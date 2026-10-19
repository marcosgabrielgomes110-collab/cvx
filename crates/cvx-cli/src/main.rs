//! CVX command-line entry point.

#![forbid(unsafe_code)]

mod cli;
mod commands;

use std::process::ExitCode;

use clap::Parser;
use cli::{Cli, Command};
use cvx_core::Result;

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let exit_code = if error.use_stderr() { 129 } else { 0 };
            let _ = error.print();
            return ExitCode::from(exit_code);
        }
    };

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("cvx: fatal: {error}");
            ExitCode::from(error.exit_code())
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Init(args) => commands::init::run(args),
        Command::Add(args) => commands::add::run(args),
        Command::Commit(args) => commands::commit::run(args),
        Command::Status(args) => commands::status::run(args),
        Command::Log(args) => commands::log::run(args),
        Command::Diff(args) => commands::diff::run(args),
        Command::Restore(args) => commands::restore::run(args),
        Command::Checkout(args) => commands::checkout::run(args),
        Command::Branch(args) => commands::branch::run(args),
        Command::Switch(args) => commands::switch::run(args),
        Command::Merge(args) => commands::merge::run(args),
        Command::Tag(args) => commands::tag::run(args),
        Command::Version => {
            commands::version::run();
            Ok(())
        }
    }
}
