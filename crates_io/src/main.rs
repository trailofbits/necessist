use anyhow::Result;
use clap::Parser;
use necessist_core::{cli, framework::Auto, necessist, Necessist};
use necessist_frameworks::Identifier;
use std::env::args;

fn main() -> Result<()> {
    env_logger::init();

    let (opts, framework): (Necessist, Auto<Identifier>) = cli::Opts::parse_from(args()).into();

    necessist(&opts, framework)
}
