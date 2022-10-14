use assert_cmd::Command;
use std::io::{stderr, Write};

const TIMEOUT: &str = "5";

#[test]
#[ignore]
fn dogfood() {
    if Command::new("git")
        .args(&["diff", "--exit-code"])
        .assert()
        .try_success()
        .is_err()
    {
        #[allow(clippy::explicit_write)]
        writeln!(stderr(), "Skipping as repository is dirty").unwrap();
        return;
    }

    Command::cargo_bin("necessist")
        .unwrap()
        .args(&["--timeout", TIMEOUT, "--verbose"])
        .assert()
        .success();
}
