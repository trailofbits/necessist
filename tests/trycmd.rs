#![cfg(unix)]

use assert_cmd::prelude::*;
use necessist::util;
use std::{path::PathBuf, process::Command};
use trycmd::TestCases;

const ROOT: &str = "examples/basic";
const TIMEOUT: &str = "5";

#[test]
fn trycmd() {
    TestCases::new().case("tests/no_necessist_db/*.toml");

    Command::cargo_bin(env!("CARGO_PKG_NAME"))
        .unwrap()
        .args(&["--root", ROOT, "--timeout", TIMEOUT])
        .assert()
        .success();

    let _remove_file = util::RemoveFile(PathBuf::from(ROOT).join("necessist.db"));

    TestCases::new().case("tests/necessist_db/*.toml");
}
