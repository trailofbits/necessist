#![warn(clippy::expect_used)]
#![warn(clippy::unwrap_used)]
#![warn(clippy::panic)]

use anyhow::Result;
use clap::Parser;
use necessist_backends::Identifier;
use necessist_core::{Necessist, cli, framework::Auto, necessist};
use std::env::args_os;

mod backends;

fn main() -> Result<()> {
    env_logger::init();

    // `args_os` rather than `args` because the latter panics on an argument that is not valid
    // Unicode. Note that `--root <ROOT>` and `<TEST_FILES_OR_DIRS>` are both paths, which need not
    // be valid Unicode.
    let (opts, framework): (Necessist, Auto<Identifier>) = cli::Opts::parse_from(args_os()).into();

    necessist(&opts, framework)
}
