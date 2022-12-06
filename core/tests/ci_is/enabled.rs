use assert_cmd::Command;
use regex::Regex;
use std::{
    fs::read_to_string,
    io::{stderr, Write},
    path::Path,
    str::from_utf8,
};
use tempfile::tempdir;

#[test]
fn clippy() {
    Command::new("cargo")
        .args([
            "clippy",
            "--all-features",
            "--all-targets",
            "--",
            "--deny=warnings",
            "--warn=clippy::pedantic",
            "--allow=clippy::missing-errors-doc",
            "--allow=clippy::missing-panics-doc",
        ])
        .current_dir("..")
        .assert()
        .success();
}

#[test]
fn dylint() {
    Command::new("cargo")
        .args(["dylint", "--all", "--", "--all-features", "--all-targets"])
        .env("DYLINT_RUSTFLAGS", "--deny warnings")
        .current_dir("..")
        .assert()
        .success();
}

#[test]
fn format() {
    preserves_cleanliness(|| {
        Command::new("cargo")
            .arg("fmt")
            .current_dir("..")
            .assert()
            .success();
    });
}

#[test]
fn license() {
    let re = Regex::new(r"^[^:]*\b(Apache-2.0|0BSD|BSD-\d-Clause|CC0-1.0|MIT)\b").unwrap();

    for line in std::str::from_utf8(
        &Command::new("cargo")
            .arg("license")
            .current_dir("..")
            .assert()
            .get_output()
            .stdout,
    )
    .unwrap()
    .lines()
    {
        assert!(re.is_match(line), "{:?} does not match", line);
    }
}

#[test]
fn markdown_link_check() {
    let tempdir = tempdir().unwrap();

    Command::new("npm")
        .args(["install", "markdown-link-check"])
        .current_dir(&tempdir)
        .assert()
        .success();

    // smoelius: https://github.com/rust-lang/crates.io/issues/788
    let config = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("markdown_link_check.json");

    let readme_md = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("README.md");

    Command::new("npx")
        .args([
            "markdown-link-check",
            "--config",
            &config.to_string_lossy(),
            &readme_md.to_string_lossy(),
        ])
        .current_dir(&tempdir)
        .assert()
        .success();
}

#[test]
fn prettier() {
    let tempdir = tempdir().unwrap();

    Command::new("npm")
        .args(["install", "prettier"])
        .current_dir(&tempdir)
        .assert()
        .success();

    Command::new("npx")
        .args([
            "prettier",
            "--check",
            &format!("{}/../**/*.json", env!("CARGO_MANIFEST_DIR")),
            &format!("{}/../**/*.md", env!("CARGO_MANIFEST_DIR")),
            &format!("{}/../**/*.yml", env!("CARGO_MANIFEST_DIR")),
            &format!("!{}/../examples/**", env!("CARGO_MANIFEST_DIR")),
            &format!("!{}/../target/**", env!("CARGO_MANIFEST_DIR")),
        ])
        .current_dir(&tempdir)
        .assert()
        .success();
}

#[test]
fn readme_contains_usage() {
    let readme = read_to_string("../README.md").unwrap();

    // smoelius: Ensure `necessist` binary is up to date.
    Command::new("cargo")
        .args(["build", "--bin", "necessist"])
        .current_dir("..")
        .assert()
        .success();

    let stdout = Command::cargo_bin("necessist")
        .unwrap()
        .arg("--help")
        .assert()
        .get_output()
        .stdout
        .clone();

    let usage = from_utf8(&stdout).unwrap();

    assert!(readme.contains(usage));
}

#[test]
fn sort() {
    Command::new("cargo")
        .args(["sort", "--check", "--grouped"])
        .current_dir("..")
        .assert()
        .success();
}

#[test]
fn update() {
    preserves_cleanliness(|| {
        Command::new("cargo")
            .args(["update", "--workspace"])
            .current_dir("..")
            .assert()
            .success();
    });
}

fn preserves_cleanliness(f: impl FnOnce()) {
    if dirty() {
        #[allow(clippy::explicit_write)]
        writeln!(stderr(), "Skipping as repository is dirty").unwrap();
        return;
    }

    f();

    assert!(!dirty());
}

fn dirty() -> bool {
    Command::new("git")
        .args(["diff", "--exit-code"])
        .current_dir("..")
        .assert()
        .try_success()
        .is_err()
}
