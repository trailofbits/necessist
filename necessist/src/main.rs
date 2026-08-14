#![warn(clippy::expect_used)]
#![warn(clippy::unwrap_used)]
#![warn(clippy::panic)]

use clap::Parser;
use necessist_backends::Identifier;
use necessist_core::{Necessist, cli, framework::Auto, necessist};
use std::{env::args_os, process::ExitCode};

mod backends;

// Exit codes follow the convention used by `grep` and `diff`: 0 means the question was answered
// affirmatively, 1 means it was answered negatively, and 2 means an error occurred. Errors are
// mapped to 2 here rather than relying on `Termination`, which would map them to 1. Note that Clap
// also exits with 2 on a usage error.
fn main() -> ExitCode {
    env_logger::init();

    // `args_os` rather than `args` because the latter panics on an argument that is not valid
    // Unicode. Note that `--root <ROOT>` and `<TEST_FILES_OR_DIRS>` are both paths, which need not
    // be valid Unicode.
    let (opts, framework): (Necessist, Auto<Identifier>) = cli::Opts::parse_from(args_os()).into();

    match necessist(&opts, framework) {
        Ok(()) => ExitCode::SUCCESS,
        // `{:?}` is what `Termination` uses, and is what produces `anyhow`'s `Caused by` sections.
        Err(error) => {
            eprintln!("Error: {error:?}");
            ExitCode::from(2)
        }
    }
}
