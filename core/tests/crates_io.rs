use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn crates_io() {
    Command::new("cargo")
        .current_dir("../crates_io")
        .args(&["build"])
        .assert()
        .success();

    Command::new("cargo")
        .current_dir("../crates_io")
        .args(&["run", "--", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Assume testing framework is <FRAMEWORK> [possible values: auto, hardhat-ts, rust]",
        ));
}
