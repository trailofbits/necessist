use anyhow::Result;
use cargo_necessist::cargo_necessist;
use std::env;
use std::ffi::OsString;

pub fn main() -> Result<()> {
    env_logger::init();

    let args: Vec<_> = env::args().map(OsString::from).collect();

    cargo_necessist(&args)
}
