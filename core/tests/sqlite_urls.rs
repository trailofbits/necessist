use assert_cmd::prelude::*;
use necessist_core::{util, Span};
use std::{
    io::{stderr, Write},
    process::Command,
    rc::Rc,
};
use tempfile::tempdir;

const URL_HTTPS: &str = "https://github.com/nzeh/proptest";
const URL_SSH: &str = "git@github.com:nzeh/proptest";
const COMMIT: &str = "7866f6d67afd9790ec6d7ee085db510cefe1a181";
const SUBDIR: &str = "proptest";
const TEST_FILE: &str = "src/sample.rs";

#[test]
fn https() {
    run_test(URL_HTTPS);
}

#[test]
fn ssh() {
    if !Command::new("ssh-add")
        .arg("-l")
        .status()
        .unwrap()
        .success()
    {
        #[allow(clippy::explicit_write)]
        writeln!(
            stderr(),
            "Skipping as ssh-agent is not running or has no identities"
        )
        .unwrap();
        return;
    }

    run_test(URL_SSH);
}

fn run_test(url: &str) {
    let tempdir = tempdir().unwrap();

    Command::new("git")
        .args(["clone", url, &tempdir.path().to_string_lossy()])
        .assert()
        .success();

    Command::new("git")
        .current_dir(&tempdir)
        .args(["checkout", COMMIT])
        .assert()
        .success();

    let root = Rc::new(tempdir.path().join(SUBDIR));

    Command::cargo_bin("necessist")
        .unwrap()
        .current_dir(&*root)
        .args([TEST_FILE])
        .assert()
        .success();

    let necessist_db = root.join("necessist.db");

    let output = Command::new("sqlite3")
        .args([
            &necessist_db.to_string_lossy(),
            "select span, url from removal",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    for line in stdout.lines() {
        let (s, url) = line.split_once('|').unwrap();
        let span = Span::parse(&root, s).unwrap();
        assert_eq!(
            &format!(
                "{}/blob/{}/{}#L{}-L{}",
                URL_HTTPS,
                COMMIT,
                util::strip_prefix(&span.source_file, tempdir.path())
                    .unwrap()
                    .to_string_lossy(),
                span.start.line,
                span.end.line
            ),
            url
        );
    }
}
