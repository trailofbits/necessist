use assert_cmd::prelude::*;
use regex::Regex;
use std::{
    io::{stderr, BufRead, BufReader, Write},
    process::Command,
};
use subprocess::{Exec, NullFile, Redirection};
use tempfile::tempdir;

const URLS: &[&str] = &[
    // https://www.reddit.com/r/rust/comments/s6olun/comment/ht5l2kj
    "https://github.com/diem/diem",
    // https://users.rust-lang.org/t/largest-rust-codebases/17027/7
    "https://github.com/rusoto/rusoto",
    "https://github.com/Uniswap/v3-core",
];

#[test]
fn parsing() {
    let re = Regex::new(r"^\d+ candidates in \d+ test files$").unwrap();

    for url in URLS {
        #[allow(clippy::explicit_write)]
        writeln!(stderr(), "{}", url).unwrap();

        let tempdir = tempdir().unwrap();

        Command::new("git")
            .args(["clone", url, &tempdir.path().to_string_lossy()])
            .assert()
            .success();

        let line = {
            let mut exec = Exec::cmd("target/debug/necessist");
            exec = exec.args(&["--root", &tempdir.path().to_string_lossy()]);
            exec = exec.stdout(Redirection::Pipe);
            exec = exec.stderr(NullFile);

            let mut popen = exec.popen().unwrap();
            let stdout = popen.stdout.as_ref().unwrap();
            let reader = BufReader::new(stdout);
            let line = reader.lines().next().unwrap().unwrap();
            popen.kill().unwrap_or_default();
            line
        };

        #[allow(clippy::explicit_write)]
        writeln!(stderr(), "{}", line).unwrap();

        assert!(re.is_match(&line));
    }
}
