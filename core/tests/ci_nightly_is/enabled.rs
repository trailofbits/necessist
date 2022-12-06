use assert_cmd::Command;

#[test]
fn udeps() {
    Command::new("cargo")
        .args(["+nightly", "udeps", "--all-targets"])
        .current_dir("..")
        .assert()
        .success();

    Command::new("cargo")
        .args([
            "+nightly",
            "udeps",
            "--all-targets",
            "--no-default-features",
        ])
        .current_dir("..")
        .assert()
        .success();
}
