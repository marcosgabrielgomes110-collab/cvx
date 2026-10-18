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
            ExitCode::from(128)
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Init(args) => commands::init::run(args),
        Command::Version => {
            commands::version::run();
            Ok(())
        }
    }
}
