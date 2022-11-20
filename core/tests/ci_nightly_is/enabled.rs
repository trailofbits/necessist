use assert_cmd::Command;

#[test]
fn udeps() {
    Command::new("cargo")
        .args(["+nightly", "udeps", "--all-targets"])
        .assert()
        .success();

    Command::new("cargo")
        .args([
            "+nightly",
            "udeps",
            "--all-targets",
            "--no-default-features",
        ])
        .assert()
        .success();
}
