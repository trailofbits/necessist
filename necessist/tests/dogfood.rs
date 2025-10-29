#![cfg(feature = "dogfood")]

use assert_cmd::{Command, cargo::cargo_bin_cmd};
use std::io::{Write, stderr};

const TIMEOUT: &str = "5";

#[test]
fn dogfood() {
    if Command::new("git")
        .args(["diff", "--exit-code"])
        .assert()
        .try_success()
        .is_err()
    {
        #[allow(clippy::explicit_write)]
        writeln!(stderr(), "Skipping as repository is dirty").unwrap();
        return;
    }

    cargo_bin_cmd!("necessist")
        .args(["--timeout", TIMEOUT, "--verbose"])
        .assert()
        .success();
}
