use lazy_static::lazy_static;
use regex::Regex;
use std::{
    fs::{remove_file, write},
    io::{stderr, BufRead, BufReader, Write},
};
use subprocess::{Exec, Redirection};
use tempfile::tempdir;

struct Test {
    expensive: bool,
    url: &'static str,
    subdir: Option<&'static str>,
    framework_and_tomls: &'static [(Option<&'static str>, Option<&'static str>)],
}

const TESTS: &[Test] = &[
    // https://www.reddit.com/r/rust/comments/s6olun/comment/ht5l2kj
    Test {
        expensive: false,
        url: "https://github.com/diem/diem",
        subdir: None,
        framework_and_tomls: &[],
    },
    Test {
        expensive: false,
        url: "https://github.com/ProjectOpenSea/operator-filter-registry",
        subdir: None,
        framework_and_tomls: &[],
    },
    Test {
        expensive: false,
        url: "https://github.com/ProjectOpenSea/seaport",
        subdir: None,
        framework_and_tomls: &[(Some("hardhat-ts"), Some(SEAPORT_CONFIG))],
    },
    // https://users.rust-lang.org/t/largest-rust-codebases/17027/7
    Test {
        expensive: true,
        url: "https://github.com/rusoto/rusoto",
        subdir: None,
        framework_and_tomls: &[],
    },
    Test {
        expensive: false,
        url: "https://github.com/smartcontractkit/chainlink",
        subdir: Some("contracts"),
        framework_and_tomls: &[
            (Some("foundry"), None),
            (Some("hardhat-ts"), Some(CHAINLINK_CONFIG)),
        ],
    },
    Test {
        expensive: false,
        url: "https://github.com/Uniswap/v3-core",
        subdir: None,
        framework_and_tomls: &[(None, Some(UNISWAP_CONFIG))],
    },
];

const CHAINLINK_CONFIG: &str = "\
ignored_functions = [\"bigNumEquals\", \"evmRevert\"]
";

const SEAPORT_CONFIG: &str = "\
ignored_functions = [\"checkExpectedEvents\"]
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
    for &Test {
        expensive,
        url,
        subdir,
        framework_and_tomls,
    } in TESTS.iter()
    {
        if expensive {
            continue;
        }
        run_test(url, subdir, framework_and_tomls);
    }
}

#[cfg_attr(
    dylint_lib = "non_thread_safe_call_in_test",
    allow(non_thread_safe_call_in_test)
)]
#[test]
#[ignore]
fn all_tests() {
    for &Test {
        expensive: _,
        url,
        subdir,
        framework_and_tomls,
    } in TESTS.iter()
    {
        run_test(url, subdir, framework_and_tomls);
    }
}

fn run_test(url: &str, subdir: Option<&str>, framework_and_tomls: &[(Option<&str>, Option<&str>)]) {
    let tempdir = tempdir().unwrap();

    #[allow(clippy::explicit_write)]
    writeln!(stderr()).unwrap();

    assert!(Exec::cmd("git")
        .args(&["clone", "--depth=1", url, &tempdir.path().to_string_lossy()])
        .stdout(Redirection::Merge)
        .stderr(Redirection::Merge)
        .join()
        .unwrap()
        .success());

    let framework_and_tomls = if framework_and_tomls.is_empty() {
        &[(None, None)]
    } else {
        assert!(framework_and_tomls
            .iter()
            .all(|(framework, toml)| framework.is_some() || toml.is_some()));
        framework_and_tomls
    };

    for (framework, toml) in framework_and_tomls {
        let tomls = toml.map_or(vec![None], |toml| vec![None, Some(toml)]);

        let mut candidates_prev = None;

        for toml in tomls {
            #[allow(clippy::explicit_write)]
            writeln!(
                stderr(),
                "
{}{}{}",
                url,
                if let Some(framework) = framework {
                    format!(" (with framework `{})", framework)
                } else {
                    String::new()
                },
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

            let necessist_toml = root.join("necessist.toml");
            if let Some(toml) = toml {
                write(necessist_toml, toml).unwrap();
            } else {
                remove_file(necessist_toml).unwrap_or_default();
            }

            let line = {
                let mut exec = Exec::cmd("../target/debug/necessist");
                exec = exec.args(&[
                    "--no-sqlite",
                    "--root",
                    &root.to_string_lossy(),
                    "--allow=config-files-experimental",
                ]);
                if let Some(framework) = framework {
                    exec = exec.args(&["--framework", framework]);
                }
                exec = exec.stdout(Redirection::Pipe);
                exec = exec.stderr(Redirection::Merge);

                let mut popen = exec.popen().unwrap();
                let stdout = popen.stdout.as_ref().unwrap();
                let reader = BufReader::new(stdout);
                let line = reader.lines().next().unwrap().unwrap();

                #[allow(clippy::explicit_write)]
                writeln!(stderr(), "{}", line).unwrap();

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
}
