use lazy_static::lazy_static;
use regex::Regex;
use std::{
    fs::write,
    io::{stderr, BufRead, BufReader, Write},
};
use subprocess::{Exec, Redirection};
use tempfile::tempdir;

const TESTS: &[(bool, &str, Option<&str>, Option<&str>)] = &[
    // https://www.reddit.com/r/rust/comments/s6olun/comment/ht5l2kj
    (false, "https://github.com/diem/diem", None, None),
    // https://users.rust-lang.org/t/largest-rust-codebases/17027/7
    (true, "https://github.com/rusoto/rusoto", None, None),
    (
        false,
        "https://github.com/smartcontractkit/chainlink",
        Some("contracts"),
        Some(CHAINLINK_CONFIG),
    ),
    (
        false,
        "https://github.com/Uniswap/v3-core",
        None,
        Some(UNISWAP_CONFIG),
    ),
];

const CHAINLINK_CONFIG: &str = "\
ignored_functions = [\"bigNumEquals\", \"evmRevert\"]
";

const UNISWAP_CONFIG: &str = "\
ignored_functions = [\"checkObservationEquals\", \"snapshotGasCost\"]
";

lazy_static! {
    static ref LINE_RE: Regex = Regex::new(r"^(\d+) candidates in (\d+) test files$").unwrap();
}

#[cfg_attr(
    dylint_lib = "non_thread_safe_call_in_test",
    allow(non_thread_safe_call_in_test)
)]
#[test]
fn cheap_tests() {
    for &(expensive, url, subdir, toml) in TESTS.iter() {
        if expensive {
            continue;
        }
        run_test(url, subdir, toml);
    }
}

#[cfg_attr(
    dylint_lib = "non_thread_safe_call_in_test",
    allow(non_thread_safe_call_in_test)
)]
#[test]
#[ignore]
fn all_tests() {
    for &(_, url, subdir, toml) in TESTS.iter() {
        run_test(url, subdir, toml);
    }
}

fn run_test(url: &str, subdir: Option<&str>, toml: Option<&str>) {
    let tempdir = tempdir().unwrap();

    assert!(Exec::cmd("git")
        .args(&["clone", url, &tempdir.path().to_string_lossy()])
        .stdout(Redirection::Merge)
        .stderr(Redirection::Merge)
        .join()
        .unwrap()
        .success());

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

        let root = subdir.map_or_else(
            || tempdir.path().to_path_buf(),
            |subdir| tempdir.path().join(subdir),
        );

        if let Some(toml) = toml {
            write(root.join("necessist.toml"), toml).unwrap();
        }

        let line = {
            let mut exec = Exec::cmd("../target/debug/necessist");
            exec = exec.args(&["--no-sqlite", "--root", &root.to_string_lossy()]);
            exec = exec.stdout(Redirection::Pipe);
            exec = exec.stderr(Redirection::Merge);

            let mut popen = exec.popen().unwrap();
            let stdout = popen.stdout.as_ref().unwrap();
            let reader = BufReader::new(stdout);
            let line = reader
                .lines()
                .find_map(|line| {
                    let line = line.unwrap();

                    #[allow(clippy::explicit_write)]
                    writeln!(stderr(), "{}", line).unwrap();

                    match line.as_ref() {
                        "Warning: Configuration files are experimental"
                        | "Silence this warning with: --allow config-files-experimental" => None,
                        _ => Some(line),
                    }
                })
                .unwrap();
            popen.kill().unwrap_or_default();
            line
        };

        let captures = LINE_RE.captures(&line).unwrap();
        assert!(captures.len() == 3);
        let candidates_curr = captures[1].parse::<usize>().unwrap();
        if let Some(candidates_prev) = candidates_prev {
            assert!(candidates_prev > candidates_curr);
        }
        candidates_prev = Some(candidates_curr);
    }
}
