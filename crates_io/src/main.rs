use anyhow::Result;
use clap::Parser;
use necessist_core::{cli, necessist, AutoUnion, Empty, Identifier, Necessist};
use std::env::args;

fn main() -> Result<()> {
    env_logger::init();

    let (opts, framework): (Necessist, AutoUnion<Identifier, Empty>) =
        cli::Opts::parse_from(args()).into();

    necessist(&opts, framework)
}
