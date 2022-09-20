use assert_cmd::prelude::*;
use necessist::{util, Span};
use std::process::Command;
use tempfile::tempdir;

const COMMIT: &str = "7866f6d67afd9790ec6d7ee085db510cefe1a181";

#[test]
fn proptest_urls_https() {
    test("https://github.com/nzeh/proptest.git");
}

#[test]
#[ignore]
fn proptest_urls_ssh() {
    test("git@github.com:nzeh/proptest.git");
}

fn test(repository: &str) {
    let tempdir = tempdir().unwrap();

    Command::new("git")
        .args(["clone", repository, &tempdir.path().to_string_lossy()])
        .assert()
        .success();

    Command::new("git")
        .current_dir(&tempdir)
        .args(["checkout", COMMIT])
        .assert()
        .success();

    let proptest = tempdir.path().join("proptest");

    Command::cargo_bin(env!("CARGO_PKG_NAME"))
        .unwrap()
        .current_dir(&proptest)
        .args(["--sqlite", "src/num.rs"])
        .assert()
        .success();

    let necessist_db = proptest.join("necessist.db");

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
        let span = s.parse::<Span>().unwrap();
        assert_eq!(
            &format!(
                "https://github.com/nzeh/proptest/blob/{}/{}#L{}-L{}",
                COMMIT,
                util::strip_prefix(&span.source_file, &tempdir.path().canonicalize().unwrap())
                    .unwrap()
                    .to_string_lossy(),
                span.start.line,
                span.end.line
            ),
            url
        );
    }
}
