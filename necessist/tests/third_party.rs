use lazy_static::lazy_static;
use pretty_assertions::assert_eq;
use regex::Regex;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashSet},
    env::{consts, var},
    ffi::OsStr,
    fmt::Write as _,
    fs::{read_dir, read_to_string, remove_file, write},
    io::{stderr, BufRead, BufReader, Read, Write},
    panic::{set_hook, take_hook},
    path::{Path, PathBuf},
    process::{exit, Command},
    sync::mpsc::channel,
    thread::{available_parallelism, spawn},
    time::Instant,
};
use subprocess::{Exec, Redirection};
use tempfile::{tempdir, TempDir};

// smoelius: `ERROR_EXIT_CODE` is from:
// https://github.com/rust-lang/rust/blob/12397e9dd5a97460d76c884d449ca1c2d26da8ed/src/libtest/lib.rs#L94
const ERROR_EXIT_CODE: i32 = 101;

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

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
struct Key {
    url: String,
    rev: Option<String>,
}

impl Key {
    fn from_test(test: &Test) -> Self {
        Self {
            url: test.url.clone(),
            rev: test.rev.clone(),
        }
    }
}

struct Repo {
    tempdir: TempDir,
    inited: bool,
    busy: bool,
}

struct Task {
    /// Repo url and revision
    key: Key,

    /// Path to temporary directory to hold the repo
    tempdir: PathBuf,

    /// Whether the repo has been cloned already
    inited: bool,

    /// Path to toml file describing the test
    path: PathBuf,

    /// The [`Test`] itself
    test: Test,
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
    let mut n_tests = 0;

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

        let key = Key::from_test(&test);
        tests.entry(key).or_default().push((path, test));
        n_tests += 1;
    }

    run_tests_concurrently(tests, n_tests);
}

fn run_tests_concurrently(mut tests: BTreeMap<Key, Vec<(PathBuf, Test)>>, mut n_tests: usize) {
    let hook = take_hook();
    set_hook(Box::new(move |panic_info| {
        hook(panic_info);
        #[allow(clippy::explicit_write)]
        writeln!(
            stderr(),
            "
If you do not see a panic message above, check that you passed --nocapture to the test binary.
",
        )
        .unwrap();
        exit(ERROR_EXIT_CODE);
    }));

    let mut repos = BTreeMap::new();

    for key in tests.keys().cloned() {
        let tempdir = tempdir().unwrap();

        repos.insert(
            key,
            Repo {
                tempdir,
                inited: false,
                busy: false,
            },
        );
    }

    let (tx_output, rx_output) = channel::<(Key, usize, String)>();

    let n_children = available_parallelism().unwrap().get();
    let mut children = Vec::new();

    for i in 0..n_children {
        let tx_output = tx_output.clone();
        let (tx_task, rx_task) = channel::<Task>();
        children.push((
            tx_task,
            spawn(move || {
                while let Ok(task) = rx_task.recv() {
                    let mut output = if task.inited {
                        String::new()
                    } else {
                        init_tempdir(&task.tempdir, &task.key)
                    };
                    output += &run_test(&task.tempdir, &task.path, &task.test);
                    tx_output.send((task.key, i, output)).unwrap();
                }
            }),
        ));
    }

    let mut children_idle = (0..n_children).collect::<HashSet<usize>>();

    while n_tests > 0 || children_idle.len() < n_children {
        let mut found = true;

        while n_tests > 0 && !children_idle.is_empty() && found {
            found = false;

            let mut tests_len = 0;

            for tests in tests.values_mut() {
                tests_len += tests.len();

                let Some((_, test)) = tests.last() else {
                    continue;
                };

                let key = Key::from_test(test);

                let repo = repos.get_mut(&key).unwrap();
                if repo.busy {
                    continue;
                }
                repo.busy = true;

                let (path, test) = tests.pop().unwrap();

                let task = Task {
                    key,
                    tempdir: repo.tempdir.path().to_path_buf(),
                    inited: repo.inited,
                    path,
                    test,
                };

                let i = *children_idle.iter().next().unwrap();
                children_idle.remove(&i);
                children[i].0.send(task).unwrap();

                n_tests -= 1;
                found = true;
                break;
            }

            assert!(found || n_tests == tests_len);
        }

        if children_idle.len() < n_children {
            let (key, i, output) = rx_output.recv().unwrap();

            #[allow(clippy::explicit_write)]
            write!(
                stderr(),
                "
{output}"
            )
            .unwrap();

            children_idle.insert(i);

            let repo = repos.get_mut(&key).unwrap();
            repo.inited = true;
            repo.busy = false;
        }
    }

    for (tx_task, child) in children {
        drop(tx_task);
        child.join().unwrap();
    }
}

#[must_use]
fn init_tempdir(tempdir: &Path, key: &Key) -> String {
    let mut command = Command::new("git");
    command.args(["clone", "--recursive", &key.url, &tempdir.to_string_lossy()]);
    if let Some(rev) = &key.rev {
        command.args(["--branch", rev]);
    } else {
        command.arg("--depth=1");
    }
    let output = command.output().unwrap();
    assert!(output.status.success());

    let mut output = std::str::from_utf8(&output.stderr).unwrap().to_owned();
    writeln!(output).unwrap();

    output
}

fn run_test(tempdir: &Path, path: &Path, test: &Test) -> String {
    let mut output = String::new();

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

    for (i, config) in configs.iter().enumerate() {
        if i > 0 {
            #[allow(clippy::explicit_write)]
            writeln!(output).unwrap();
        }

        #[allow(clippy::explicit_write)]
        writeln!(
            output,
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
            writeln!(output, "elapsed: {:?}", start.elapsed()).unwrap();
        } else {
            let reader = BufReader::new(stdout);
            let line = reader.lines().next().unwrap().unwrap();
            writeln!(buf, "{line}").unwrap();

            #[allow(clippy::explicit_write)]
            writeln!(output, "{line}").unwrap();

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

    output
}
