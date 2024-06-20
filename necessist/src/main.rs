#![warn(clippy::expect_used)]
#![warn(clippy::unwrap_used)]
#![warn(clippy::panic)]

use anyhow::Result;
use clap::Parser;
use necessist_backends::Identifier;
use necessist_core::{cli, framework::Auto, necessist, Necessist};
use std::env::args;

mod backends;

fn main() -> Result<()> {
    env_logger::init();

    let (opts, framework): (Necessist, Auto<Identifier>) = cli::Opts::parse_from(args()).into();

    necessist(&opts, framework)
}
