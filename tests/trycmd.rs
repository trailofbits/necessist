#![cfg(unix)]

use assert_cmd::prelude::*;
use std::{path::PathBuf, process::Command};
use trycmd::TestCases;

const ROOT: &str = "examples/basic";
const TIMEOUT: &str = "10";

struct RemoveFile(PathBuf);

impl Drop for RemoveFile {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0)
            .map_err(|err| eprintln!("{}", err))
            .unwrap_or_default();
    }
}

#[test]
fn trycmd() {
    TestCases::new().case("tests/no_necessist_db/*.toml");

    Command::cargo_bin(env!("CARGO_PKG_NAME"))
        .unwrap()
        .args(&["--root", ROOT, "--sqlite", "--timeout", TIMEOUT])
        .assert()
        .success();

    let _remove_file = RemoveFile(PathBuf::from(ROOT).join("necessist.db"));

    TestCases::new().case("tests/necessist_db/*.toml");
}
