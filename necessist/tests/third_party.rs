use lazy_static::lazy_static;
use pretty_assertions::assert_eq;
use regex::Regex;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    env::{consts, var},
    ffi::OsStr,
    fs::read_dir,
    fs::{read_to_string, remove_file, write},
    io::{stderr, BufRead, BufReader, Read, Write},
    path::Path,
    time::Instant,
};
use subprocess::{Exec, Redirection};
use tempfile::tempdir;

// smoelius: The Go packages were chosen because their ratios of "number of tests" to "time required
// to run the tests" are high.

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Test {
    url: String,

    #[serde(default)]
    rev: Option<String>,

    #[serde(default)]
    target_os: Option<String>,

    #[serde(default)]
    subdir: Option<String>,

    #[serde(default)]
    framework: Option<String>,

    #[serde(default)]
    test_files: Vec<String>,

    #[serde(default)]
    config: toml::Table,

    #[serde(default)]
    full: bool,
}

lazy_static! {
    static ref LINE_RE: Regex = Regex::new(r"^(\d+) candidates in (\d+) test file(s)?$").unwrap();
}

#[cfg_attr(
    dylint_lib = "non_thread_safe_call_in_test",
    allow(non_thread_safe_call_in_test)
)]
#[test]
fn all_tests() {
    let mut tests = BTreeMap::<_, Vec<_>>::new();

    for entry in read_dir("tests/third_party_tests").unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.extension() != Some(OsStr::new("toml")) {
            continue;
        }

        // smoelius: `TESTNAME` is what Clippy uses:
        // https://github.com/rust-lang/rust-clippy/blame/f8f9d01c2ad0dff565bdd60feeb4cbd09dada8cd/book/src/development/adding_lints.md#L99
        if var("TESTNAME").ok().map_or(false, |testname| {
            path.file_stem() != Some(OsStr::new(&testname))
        }) {
            continue;
        }

        let contents = read_to_string(&path).unwrap();
        let test: Test = toml::from_str(&contents).unwrap();

        if test
            .target_os
            .as_ref()
            .map_or(false, |target_os| target_os != consts::OS)
        {
            continue;
        }

        tests
            .entry((test.url.clone(), test.rev.clone()))
            .or_default()
            .push((path, test));
    }

    for ((url, rev), tests) in tests {
        let tempdir = tempdir().unwrap();

        init_tempdir(tempdir.path(), &url, &rev);

        assert!(!tests.is_empty());

        for (path, test) in tests {
            run_test(tempdir.path(), &path, &test);
        }
    }
}

fn init_tempdir(tempdir: &Path, url: &str, rev: &Option<String>) {
    let mut exec =
        Exec::cmd("git").args(&["clone", "--recursive", &url, &tempdir.to_string_lossy()]);
    if let Some(rev) = rev {
        exec = exec.args(&["--branch", &rev]);
    } else {
        exec = exec.arg("--depth=1");
    }
    assert!(exec
        .stdout(Redirection::Merge)
        .stderr(Redirection::Merge)
        .join()
        .unwrap()
        .success());

    #[allow(clippy::explicit_write)]
    writeln!(stderr()).unwrap();
}

fn run_test(tempdir: &Path, path: &Path, test: &Test) {
    let tempdir_canonicalized = tempdir.canonicalize().unwrap();

    let path_stdout = path.with_extension("stdout");

    let stdout_expected = read_to_string(&path_stdout)
        .map_err(|error| format!("Failed to read {path_stdout:?}: {error}"))
        .unwrap();

    let configs = if test.config.is_empty() {
        vec![None]
    } else {
        vec![None, Some(&test.config)]
    };

    let mut candidates_prev = None;

    for config in configs {
        #[allow(clippy::explicit_write)]
        writeln!(
            stderr(),
            "{}{}{}{}",
            test.url,
            if let Some(subdir) = &test.subdir {
                format!(" (in `{subdir}`)")
            } else {
                String::new()
            },
            if let Some(framework) = &test.framework {
                format!(" (with framework `{framework}`)")
            } else {
                String::new()
            },
            if config.is_some() {
                " (with necessist.toml)"
            } else {
                ""
            }
        )
        .unwrap();

        let root = test
            .subdir
            .as_ref()
            .map_or_else(|| tempdir.to_path_buf(), |subdir| tempdir.join(subdir));

        let necessist_toml = root.join("necessist.toml");
        if let Some(config) = config {
            write(necessist_toml, config.to_string()).unwrap();
        } else {
            remove_file(necessist_toml).unwrap_or_default();
        }

        let mut exec = Exec::cmd("../target/debug/necessist");
        exec = exec.args(&[
            "--no-sqlite",
            "--root",
            &root.to_string_lossy(),
            "--allow=config-files-experimental",
            "--verbose",
        ]);
        if let Some(framework) = &test.framework {
            exec = exec.args(&["--framework", framework]);
        }
        for test_file in &test.test_files {
            exec = exec.arg(tempdir.join(test_file));
        }
        exec = exec.stdout(Redirection::Pipe);
        exec = exec.stderr(Redirection::Merge);

        let start = Instant::now();

        let mut popen = exec.popen().unwrap();
        let mut stdout = popen.stdout.as_ref().unwrap();

        let mut buf = Vec::new();

        if test.full {
            let _ = stdout.read_to_end(&mut buf).unwrap();

            #[allow(clippy::explicit_write)]
            writeln!(stderr(), "elapsed: {:?}\n", start.elapsed()).unwrap();
        } else {
            let reader = BufReader::new(stdout);
            let line = reader.lines().next().unwrap().unwrap();
            writeln!(&mut buf, "{line}").unwrap();

            #[allow(clippy::explicit_write)]
            writeln!(stderr(), "{line}\n").unwrap();

            popen.kill().unwrap();

            let captures = LINE_RE
                .captures(&line)
                .unwrap_or_else(|| panic!("Failed to find matching line for test {path:?}"));
            assert_eq!(4, captures.len());
            let candidates_curr = captures[1].parse::<usize>().unwrap();
            if let Some(candidates_prev) = candidates_prev {
                assert!(candidates_prev > candidates_curr);
            } else if !test.config.is_empty() {
                candidates_prev = Some(candidates_curr);
                continue;
            }
        }

        let stdout_actual = std::str::from_utf8(&buf).unwrap();

        // smoelius: macOS requires the paths to be canonicalized, because `/tmp` is symlinked to
        // `private/tmp`.
        let stdout_normalized =
            stdout_actual.replace(&tempdir_canonicalized.to_string_lossy().to_string(), "$DIR");

        assert_eq!(stdout_expected, stdout_normalized);
    }
}
