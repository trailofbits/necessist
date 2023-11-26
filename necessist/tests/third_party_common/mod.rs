use assert_cmd::output::OutputError;
use necessist_core::{util, Span};
use necessist_util::{tempdir, TempDir};
use regex::Regex;
use serde::Deserialize;
use similar_asserts::SimpleDiff;
use std::{
    collections::{BTreeMap, HashSet},
    env::{consts, join_paths, set_var, split_paths, var},
    ffi::OsStr,
    fmt::Write as _,
    fs::{read_dir, read_to_string, remove_file, write},
    io::{stderr, Read, Write},
    panic::{set_hook, take_hook},
    path::{Path, PathBuf},
    process::{exit, Command},
    rc::Rc,
    sync::mpsc::channel,
    thread::{available_parallelism, spawn},
    time::Instant,
};
use subprocess::{Exec, Redirection};

mod string_or_vec;
use string_or_vec::StringOrVec;

// smoelius: `ERROR_EXIT_CODE` is from:
// https://github.com/rust-lang/rust/blob/12397e9dd5a97460d76c884d449ca1c2d26da8ed/src/libtest/lib.rs#L94
const ERROR_EXIT_CODE: i32 = 101;

const PREFIX_HTTPS: &str = "https://github.com/";
const PREFIX_SSH: &str = "git@github.com:";

// smoelius: The Go packages were chosen because their ratios of "number of tests" to "time required
// to run the tests" are high.

// smoelius: Put `toml::Table`s toward the end.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Test {
    /// Repo url
    url: String,

    /// Repo revision; `None` (the default) means the head of the default branch
    #[serde(default)]
    rev: Option<String>,

    /// Command to run after the repository is checked out, but before any tests are run. The
    /// command is run with `bash -c '...'`.
    #[serde(default)]
    init: Option<String>,

    /// Path to canonicalize and prepend to the `PATH` environment variable after the `init`
    /// command is run, but before Necessist is run. The path is relative to the repository root.
    #[serde(default)]
    path_prefix: Option<String>,

    /// OS on which the test should run; `None` (the default) means all OSes
    #[serde(default)]
    target_os: Option<StringOrVec>,

    /// Subdirectory of the repo in which Necessist should run; `None` (the default) means the root
    /// of the repository
    #[serde(default)]
    subdir: Option<String>,

    /// Testing framework to use (i.e., `foundry`, `go`, etc.); `None` (the default) means `auto`
    #[serde(default)]
    framework: Option<String>,

    /// Test files to mutilate
    #[serde(default)]
    test_files: Vec<String>,

    /// If false (the default), Necessist dumps removal candidates and exits (i.e.,
    /// --dump-candidates is passed to Necessist); if true, Necessist is run with --verbose
    #[serde(default)]
    full: bool,

    /// Check that the spans and urls written to the database are consistent
    #[serde(default)]
    check_sqlite_urls: bool,

    /// If false (the default), Necessist performs two runs, one with and one without the config;
    /// if true, Necessist performs just one run with the config
    #[serde(default)]
    config_mandatory: bool,

    /// [Configuration file] contents
    ///
    /// configuration file: https://github.com/trailofbits/necessist#configuration-files
    #[serde(default)]
    config: toml::Table,
}

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
struct Key {
    url: String,
    rev: Option<String>,
    init: Option<String>,
}

impl Key {
    fn from_test(test: &Test) -> Self {
        Self {
            url: test.url.clone(),
            rev: test.rev.clone(),
            init: test.init.clone(),
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

pub fn all_tests_in(path: impl AsRef<Path>) {
    set_var("CARGO_TERM_COLOR", "never");

    let mut tests = BTreeMap::<_, Vec<_>>::new();
    let mut n_tests = 0;

    for entry in read_dir(path).unwrap() {
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

        if test.url.starts_with(PREFIX_SSH) && !ssh_agent_is_running() {
            #[allow(clippy::explicit_write)]
            writeln!(
                stderr(),
                "Skipping {path:?} as ssh-agent is not running or has no identities",
            )
            .unwrap();

            continue;
        }

        if test.target_os.as_ref().map_or(false, |target_os| {
            target_os.get().iter().all(|s| s != consts::OS)
        }) {
            continue;
        }

        let key = Key::from_test(&test);
        tests.entry(key).or_default().push((path, test));
        n_tests += 1;
    }

    if n_tests == 1 {
        let (key, mut tests) = tests.pop_first().unwrap();
        let (path, test) = tests.remove(0);
        let tempdir = tempdir().unwrap();
        let mut output = init_tempdir(tempdir.path(), &key);
        output += &run_test(tempdir.path(), &path, &test);
        #[allow(clippy::explicit_write)]
        write!(stderr(), "{output}").unwrap();
    } else {
        run_tests_concurrently(tests, n_tests);
    }
}

fn ssh_agent_is_running() -> bool {
    Command::new("ssh-add")
        .arg("-l")
        .status()
        .unwrap()
        .success()
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
    if key.rev.is_none() {
        command.arg("--depth=1");
    }
    let output = command.output().unwrap();
    assert!(output.status.success(), "{}", OutputError::new(output));

    let mut output_combined = std::str::from_utf8(&output.stderr).unwrap().to_owned();

    if let Some(rev) = &key.rev {
        let output = Command::new("git")
            .args(["checkout", rev])
            .current_dir(tempdir)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", OutputError::new(output));

        output_combined += std::str::from_utf8(&output.stderr).unwrap();
    }

    if let Some(init) = &key.init {
        let output = Command::new("bash")
            .args(["-c", init])
            .current_dir(tempdir)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", OutputError::new(output));

        output_combined += std::str::from_utf8(&output.stderr).unwrap();
    }

    writeln!(output_combined).unwrap();

    output_combined
}

#[allow(clippy::too_many_lines)]
fn run_test(tempdir: &Path, path: &Path, test: &Test) -> String {
    let mut output = String::new();

    let tempdir_canonicalized = tempdir.canonicalize().unwrap();

    let configs = if test.config.is_empty() {
        vec![None]
    } else if test.config_mandatory {
        vec![Some(&test.config)]
    } else {
        vec![None, Some(&test.config)]
    };

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

        let path_stdout = if test.config.is_empty() {
            path.with_extension("stdout")
        } else if config.is_none() {
            path.with_extension("without_config.stdout")
        } else {
            path.with_extension("with_config.stdout")
        };

        let stdout_expected = read_to_string(&path_stdout)
            .map_err(|error| format!("Failed to read {path_stdout:?}: {error:?}"))
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
        exec = exec.args(&["--no-sqlite", "--root", &root.to_string_lossy()]);
        if let Some(prefix) = &test.path_prefix {
            let prefix_canonicalized = tempdir.join(prefix).canonicalize().unwrap();
            let path = var("PATH").unwrap();
            let path_prepended =
                join_paths(std::iter::once(prefix_canonicalized).chain(split_paths(&path)))
                    .unwrap();
            exec = exec.env("PATH", path_prepended);
        }
        if let Some(framework) = &test.framework {
            exec = exec.args(&["--framework", framework]);
        }
        if test.full {
            exec = exec.arg("--verbose");
        } else {
            exec = exec.arg("--dump-candidates");
        }
        for test_file in &test.test_files {
            exec = exec.arg(
                tempdir
                    .join(test.subdir.as_deref().unwrap_or("."))
                    .join(test_file),
            );
        }
        exec = exec.stdout(Redirection::Pipe);
        exec = exec.stderr(Redirection::Merge);

        let start = Instant::now();

        let popen = exec.popen().unwrap();
        let mut stdout = popen.stdout.as_ref().unwrap();

        let mut buf = Vec::new();

        let _ = stdout.read_to_end(&mut buf).unwrap();

        #[allow(clippy::explicit_write)]
        writeln!(output, "elapsed: {:?}", start.elapsed()).unwrap();

        let stdout_actual = std::str::from_utf8(&buf).unwrap();

        // smoelius: macOS requires the paths to be canonicalized, because `/tmp` is symlinked to
        // `private/tmp`.
        let stdout_normalized = normalize_paths(stdout_actual, &tempdir_canonicalized);

        if enabled("BLESS") {
            write(path_stdout, stdout_normalized).unwrap();
        } else {
            // smoelius: Because test files could be traversed in different orders on different
            // machines, the warnings could appear out of order. So simply verify that
            // `stdout_expected` and `stdout_actual` contain the same lines.
            // smoelius: Also, ignore timeouts. Some of the `v3-core_factory` tests take close to
            // the time limit on macOS, and trying to ignore the tests individually is like playing
            // whack-a-mole.
            assert!(
                permutation_ignoring_timeouts(&stdout_expected, &stdout_normalized),
                "{}",
                SimpleDiff::from_str(&stdout_expected, &stdout_normalized, "left", "right")
            );
            // smoelius: If `stdout_expected` and `stdout_actual` differ, prefer that the
            // lexicographically smaller one be stored in the repository.
            assert!(
                stdout_expected <= stdout_normalized,
                "{}",
                SimpleDiff::from_str(&stdout_expected, &stdout_normalized, "left", "right")
            );
        }

        if test.check_sqlite_urls {
            assert!(test.rev.is_some());
            assert!(test.full);
            check_sqlite_urls(tempdir, &root, test);
        }
    }

    output
}

fn check_sqlite_urls(tempdir: &Path, root: &Path, test: &Test) {
    let root = Rc::new(root.to_path_buf());

    let necessist_db = root.join("necessist.db");

    let url_https = if let Some(suffix) = test.url.strip_prefix(PREFIX_SSH) {
        String::from(PREFIX_HTTPS) + suffix
    } else {
        test.url.clone()
    };

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
                url_https,
                test.rev.as_ref().unwrap(),
                util::strip_prefix(&span.source_file, tempdir)
                    .unwrap()
                    .to_string_lossy(),
                span.start.line,
                span.end.line
            ),
            url
        );
    }
}

fn normalize_paths(mut s: &str, path: &Path) -> String {
    let path_str = path.to_string_lossy();
    let mut buf = String::new();
    while let Some(i) = s.find(&*path_str) {
        buf.push_str(&s[..i]);
        buf.push_str("$DIR");
        s = &s[i + path_str.len()..];
        // smoelius: Replace `\` up until the next whitespace.
        let n = s.find(char::is_whitespace).unwrap_or(s.len());
        buf.push_str(&s[..n].replace('\\', "/"));
        s = &s[n..];
    }
    // smoelius: Push whatever is remaining.
    buf.push_str(s);
    buf
}

fn permutation_ignoring_timeouts(expected: &str, actual: &str) -> bool {
    let mut expected_lines = expected.lines().collect::<Vec<_>>();
    let mut actual_lines = actual.lines().collect::<Vec<_>>();
    expected_lines.sort_unstable();
    actual_lines.sort_unstable();
    expected_lines.len() == actual_lines.len()
        && expected_lines
            .into_iter()
            .zip(actual_lines)
            .all(|(expected_line, actual_line)| {
                expected_line == actual_line
                    || actual_line
                        .strip_suffix("timed-out")
                        .map_or(false, |prefix| expected_line.starts_with(prefix))
            })
}

pub fn stdout_subsequence_in(path: impl AsRef<Path>) {
    for entry in read_dir(path).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        let Some(base) = path
            .to_string_lossy()
            .strip_suffix(".without_config.stdout")
            .map(ToOwned::to_owned)
        else {
            continue;
        };

        let path_with_config = Path::new(&base).with_extension("with_config.stdout");
        if !path_with_config.try_exists().unwrap_or_default() {
            continue;
        }

        let contents_with_config = read_to_string(path_with_config).unwrap();
        let contents_without_config = read_to_string(path).unwrap();

        let lines_with_config = contents_with_config.lines();
        let lines_without_config = contents_without_config.lines();

        assert!(
            subsequence(lines_with_config, lines_without_config),
            "failed for {base:?}"
        );
    }
}

fn subsequence<'a, 'b>(
    xs: impl Iterator<Item = &'a str>,
    mut ys: impl Iterator<Item = &'b str>,
) -> bool {
    let re = Regex::new(r"^(\d+) candidates in (\d+) test file(s)?$").unwrap();

    let mut xs = xs.peekable();

    while let Some(&x) = xs.peek() {
        let Some(y) = ys.next() else {
            dbg!(x);
            return false;
        };
        if x == y || (re.is_match(x) && re.is_match(y)) {
            let _ = xs.next();
        }
    }

    true
}

fn enabled(key: &str) -> bool {
    var(key).map_or(false, |value| value != "0")
}
