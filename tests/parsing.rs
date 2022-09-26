use assert_cmd::prelude::*;
use regex::Regex;
use std::{
    fs::write,
    io::{stderr, BufRead, BufReader, Write},
    process::Command,
};
use subprocess::{Exec, NullFile, Redirection};
use tempfile::tempdir;

const TESTS: &[(&str, Option<&str>)] = &[
    // https://www.reddit.com/r/rust/comments/s6olun/comment/ht5l2kj
    ("https://github.com/diem/diem", None),
    // https://users.rust-lang.org/t/largest-rust-codebases/17027/7
    ("https://github.com/rusoto/rusoto", None),
    ("https://github.com/Uniswap/v3-core", Some(UNISWAP_CONFIG)),
];

const UNISWAP_CONFIG: &str = "\
ignored_functions = [\"checkObservationEquals\", \"snapshotGasCost\"]
";

#[test]
fn parsing() {
    let re = Regex::new(r"^(\d+) candidates in (\d+) test files$").unwrap();

    for (url, toml) in TESTS {
        let tempdir = tempdir().unwrap();

        Command::new("git")
            .args(["clone", url, &tempdir.path().to_string_lossy()])
            .assert()
            .success();

        let tomls = toml.map_or(vec![None], |toml| vec![None, Some(toml)]);

        let mut candidates_prev = None;

        for toml in tomls {
            #[allow(clippy::explicit_write)]
            writeln!(
                stderr(),
                "{}{}",
                url,
                if toml.is_some() {
                    " (with necessist.toml)"
                } else {
                    ""
                }
            )
            .unwrap();

            if let Some(toml) = toml {
                write(tempdir.path().join("necessist.toml"), toml).unwrap();
            }

            let line = {
                let mut exec = Exec::cmd("target/debug/necessist");
                exec = exec.args(&["--no-sqlite", "--root", &tempdir.path().to_string_lossy()]);
                exec = exec.stdout(Redirection::Pipe);
                exec = exec.stderr(NullFile);

                let mut popen = exec.popen().unwrap();
                let stdout = popen.stdout.as_ref().unwrap();
                let reader = BufReader::new(stdout);
                let line = reader
                    .lines()
                    .find_map(|line| {
                        let line = line.unwrap();
                        if line == "Warning: Configuration files are experimental" {
                            None
                        } else {
                            Some(line)
                        }
                    })
                    .unwrap();
                popen.kill().unwrap_or_default();
                line
            };

            #[allow(clippy::explicit_write)]
            writeln!(stderr(), "{}", line).unwrap();

            let captures = re.captures(&line).unwrap();
            assert!(captures.len() == 3);
            let candidates_curr = captures[1].parse::<usize>().unwrap();
            if let Some(candidates_prev) = candidates_prev {
                assert!(candidates_prev > candidates_curr);
            }
            candidates_prev = Some(candidates_curr);
        }
    }
}
