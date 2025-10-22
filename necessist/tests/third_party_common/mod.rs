use assert_cmd::output::OutputError;
use necessist_core::{Span, util};
use regex::Regex;
use serde::Deserialize;
use similar_asserts::SimpleDiff;
use std::{
    collections::{BTreeMap, HashSet},
    env::{consts, join_paths, set_var, split_paths, var},
    ffi::OsStr,
    fmt::Write as _,
    fs::{read_dir, read_to_string, remove_file, write},
    io::{Read, Write, stderr},
    ops::Not,
    panic::{set_hook, take_hook},
    path::{Path, PathBuf},
    process::{Command, exit},
    rc::Rc,
    sync::{LazyLock, mpsc::channel},
    thread::{available_parallelism, spawn},
    time::{Duration, Instant},
};
use subprocess::{Exec, Redirection};

mod string_or_vec;
use string_or_vec::StringOrVec;

#[path = "../tempfile_util.rs"]
mod tempfile_util;
use tempfile_util::{TempDir, tempdir};

const N_PARTITIONS: usize = 2;

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

    /// Path to prepend to the `PATH` environment variable after the `init` command is run, but
    /// before Necessist is run. The path is relative to the repository root.
    #[serde(default)]
    path_prefix: Option<String>,

    // smoelius: Allow `clippy::doc_markdown` until the following appears in nightly:
    // https://github.com/rust-lang/rust-clippy/pull/12419
    #[allow(clippy::doc_markdown)]
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
    source_files: Vec<String>,

    /// If true, Necessist dumps removal candidates and exits (i.e., --dump-candidates is passed to
    /// Necessist); if false (the default), Necessist is run with --verbose
    #[serde(default)]
    parsing_only: bool,

    /// Additional arguments to pass to Necessist; appended at the end of the command
    #[serde(default)]
    args: Vec<String>,

    /// Check that the spans and urls written to the database are consistent
    #[serde(default)]
    check_sqlite_urls: bool,

    /// If false (the default), Necessist performs two runs, one with and one without the config;
    /// if true, Necessist performs just one run with the config
    #[serde(default)]
    config_mandatory: bool,

    /// [Configuration file] contents
    ///
    /// [Configuration file]: https://github.com/trailofbits/necessist#configuration-files
    #[serde(default)]
    config: toml::Table,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
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
    workdir: TempDir,
    inited: bool,
    busy: bool,
    n_outstanding_tests: usize,
}

struct Task {
    /// Repo url and revision
    key: Key,

    /// Path to temporary directory to hold the repo
    workdir: PathBuf,

    /// Whether the repo has been cloned already
    inited: bool,

    /// Path to toml file describing the test
    toml_path: PathBuf,

    /// The [`Test`] itself
    test: Test,
}

pub fn all_tests_in(dir: impl AsRef<Path>) {
    unsafe {
        set_var("CARGO_TERM_COLOR", "never");
    }

    let mut tests = read_tests_in(dir, true);
    let n_tests = tests.values().map(Vec::len).sum();

    if n_tests == 1 {
        let (key, mut tests) = tests.pop_first().unwrap();
        let (toml_path, test) = tests.remove(0);
        let workdir = tempdir().unwrap();
        let mut output_combined = init_workdir(workdir.path(), &key);
        let (output, elapsed) = run_test(workdir.path(), &toml_path, &test);
        output_combined += &output;
        #[allow(clippy::explicit_write)]
        write!(stderr(), "{output}").unwrap();
        #[allow(clippy::explicit_write)]
        writeln!(stderr(), "elapsed: {elapsed:?}").unwrap();
    } else {
        run_tests_concurrently(tests, n_tests);
    }
}

fn read_tests_in(dir: impl AsRef<Path>, filter: bool) -> BTreeMap<Key, Vec<(PathBuf, Test)>> {
    let mut tests = BTreeMap::<_, Vec<_>>::new();

    for entry in read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.extension() != Some(OsStr::new("toml")) {
            continue;
        }

        let toml_path = path;

        // smoelius: `TESTNAME` is what Clippy uses:
        // https://github.com/rust-lang/rust-clippy/blame/f8f9d01c2ad0dff565bdd60feeb4cbd09dada8cd/book/src/development/adding_lints.md#L99
        if filter
            && var("TESTNAME")
                .is_ok_and(|testname| toml_path.file_stem() != Some(OsStr::new(&testname)))
        {
            continue;
        }

        let contents = read_to_string(&toml_path).unwrap();
        let test: Test = toml::from_str(&contents).unwrap();

        if test.url.starts_with(PREFIX_SSH) && !ssh_agent_is_running() {
            #[allow(clippy::explicit_write)]
            writeln!(
                stderr(),
                "Skipping `{}` as ssh-agent is not running or has no identities",
                toml_path.display()
            )
            .unwrap();

            continue;
        }

        if filter && !target_os_includes(test.target_os.as_ref(), consts::OS) {
            continue;
        }

        let key = Key::from_test(&test);
        tests.entry(key).or_default().push((toml_path, test));
    }

    tests
}

fn ssh_agent_is_running() -> bool {
    Command::new("ssh-add")
        .arg("-l")
        .status()
        .unwrap()
        .success()
}

#[allow(clippy::too_many_lines)]
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

    let mut summary = BTreeMap::<Key, (Vec<(PathBuf, Duration)>, Duration)>::new();

    let mut repos = BTreeMap::new();

    for (key, tests) in &tests {
        let workdir = tempdir().unwrap();

        let n_outstanding_tests = tests.len();

        repos.insert(
            key.clone(),
            Repo {
                workdir,
                inited: false,
                busy: false,
                n_outstanding_tests,
            },
        );
    }

    let (tx_output, rx_output) = channel::<(Key, usize, String, PathBuf, Duration)>();

    let n_children = available_parallelism().unwrap().get();
    let mut children = Vec::new();

    for i in 0..n_children {
        let tx_output = tx_output.clone();
        let (tx_task, rx_task) = channel::<Task>();
        children.push((
            tx_task,
            spawn(move || {
                while let Ok(task) = rx_task.recv() {
                    let mut output_combined = if task.inited {
                        String::new()
                    } else {
                        init_workdir(&task.workdir, &task.key)
                    };
                    let (output, elapsed) = run_test(&task.workdir, &task.toml_path, &task.test);
                    output_combined += &output;
                    tx_output
                        .send((task.key, i, output_combined, task.toml_path, elapsed))
                        .unwrap();
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

                let (toml_path, test) = tests.pop().unwrap();

                let task = Task {
                    key,
                    workdir: repo.workdir.path().to_path_buf(),
                    inited: repo.inited,
                    toml_path,
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
            let (key, i, output, toml_path, elapsed) = rx_output.recv().unwrap();

            // smoelius: The next `writeln!` used to prepend a blank line to the child's output.
            // However, now that `git` is invoked with `--quiet`, this is no longer necessary.
            #[allow(clippy::explicit_write)]
            write!(stderr(), "{output}").unwrap();

            children_idle.insert(i);

            let repo = repos.get_mut(&key).unwrap();
            repo.inited = true;
            repo.busy = false;
            repo.n_outstanding_tests -= 1;
            if repo.n_outstanding_tests == 0 {
                let workdir = repo.workdir.path().to_path_buf();
                #[allow(clippy::explicit_write)]
                writeln!(stderr(), "--> Removing workdir for {key:?}").unwrap();
                repos.remove(&key);
                assert!(!workdir.try_exists().unwrap());
            }

            let value = summary.entry(key).or_default();
            value.0.push((toml_path, elapsed));
            value.1 += elapsed;
        }
    }

    for (tx_task, child) in children {
        drop(tx_task);
        child.join().unwrap();
    }

    display_summary(summary);
}

fn display_summary(mut summary: BTreeMap<Key, (Vec<(PathBuf, Duration)>, Duration)>) {
    for (pairs, _) in summary.values_mut() {
        pairs.sort_by_key(|&(_, elapsed)| elapsed);
    }

    let mut summary = summary.into_iter().collect::<Vec<_>>();
    summary.sort_by_key(|&(_, (_, total))| total);

    let width_toml_path = summary
        .iter()
        .flat_map(|(_, (pairs, _))| pairs.iter().map(|(toml_path, _)| toml_path))
        .fold(0, |width, toml_path| {
            std::cmp::max(width, toml_path.to_string_lossy().len())
        });

    let width_elapsed = summary
        .iter()
        .flat_map(|(_, (pairs, total))| {
            pairs
                .iter()
                .map(|(_, elapsed)| elapsed)
                .chain(std::iter::once(total))
        })
        .fold(0, |width, elapsed| {
            std::cmp::max(width, elapsed.as_secs().to_string().len())
        });

    // smoelius: Prepend the summary with a blank line.
    println!();

    for (key, (pairs, total)) in summary {
        println!("{key:?}");
        for (toml_path, elapsed) in pairs {
            println!(
                "    {:width_toml_path$}  {:>width_elapsed$}s",
                toml_path.to_string_lossy(),
                elapsed.as_secs(),
            );
        }
        println!(
            "    {:width_toml_path$}  {:>width_elapsed$}s",
            "",
            total.as_secs()
        );
    }
}

#[cfg_attr(dylint_lib = "supplementary", allow(commented_out_code))]
#[must_use]
fn init_workdir(workdir: &Path, key: &Key) -> String {
    let mut command = Command::new("git");
    command.args([
        "clone",
        "--recursive",
        "--quiet",
        &key.url,
        &workdir.to_string_lossy(),
    ]);
    if key.rev.is_none() {
        command.arg("--depth=1");
    }
    let output = command.output().unwrap();
    assert!(output.status.success(), "{}", OutputError::new(output));

    let mut output_combined = std::str::from_utf8(&output.stderr).unwrap().to_owned();

    if let Some(rev) = &key.rev {
        let output = Command::new("git")
            .args(["checkout", "--quiet", rev])
            .current_dir(workdir)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", OutputError::new(output));

        output_combined += std::str::from_utf8(&output.stderr).unwrap();
    }

    if let Some(init) = &key.init {
        let output = Command::new("bash")
            .args(["-c", init])
            .current_dir(workdir)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", OutputError::new(output));

        output_combined += std::str::from_utf8(&output.stderr).unwrap();
    }

    // smoelius: The next `writeln!` used to append a blank line to `git`'s output. However, now
    // that `git` is invoked with `--quiet`, this is no longer necessary. In fact, I would expect
    // `git`'s output to be empty in most cases.
    // writeln!(output_combined).unwrap();

    output_combined
}

#[cfg_attr(dylint_lib = "supplementary", allow(commented_out_code))]
#[allow(clippy::too_many_lines)]
fn run_test(workdir: &Path, toml_path: &Path, test: &Test) -> (String, Duration) {
    let mut output = String::new();
    let mut elapsed = Duration::default();

    let configs = if test.config.is_empty() {
        vec![None]
    } else if test.config_mandatory {
        vec![Some(&test.config)]
    } else {
        vec![None, Some(&test.config)]
    };

    for config in configs {
        // smoelius: The next `writeln!` used to interpose blank lines between task descriptions.
        // However, now that `git` is invoked with `--quiet`, this is no longer necessary.
        /* if i > 0 {
            #[allow(clippy::explicit_write)]
            writeln!(output).unwrap();
        } */

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
            toml_path.with_extension("stdout")
        } else if config.is_none() {
            toml_path.with_extension("without_config.stdout")
        } else {
            toml_path.with_extension("with_config.stdout")
        };

        let stdout_expected = read_to_string(&path_stdout)
            .map_err(|error| format!("Failed to read `{}`: {error:?}", path_stdout.display()))
            .unwrap();

        let root = test
            .subdir
            .as_ref()
            .map_or_else(|| workdir.to_path_buf(), |subdir| workdir.join(subdir));

        let necessist_toml = root.join("necessist.toml");
        if let Some(config) = config {
            write(necessist_toml, config.to_string()).unwrap();
        } else {
            remove_file(necessist_toml).unwrap_or_default();
        }

        let mut exec = Exec::cmd("../target/debug/necessist");
        exec = exec.args(&["--no-sqlite", "--root", &root.to_string_lossy()]);
        if let Some(prefix) = &test.path_prefix {
            let prefix_in_workdir = workdir.join(prefix);
            let path = var("PATH").unwrap();
            let path_prepended =
                join_paths(std::iter::once(prefix_in_workdir).chain(split_paths(&path))).unwrap();
            exec = exec.env("PATH", path_prepended);
        }
        if let Some(framework) = &test.framework {
            exec = exec.args(&["--framework", framework]);
        }
        if test.parsing_only {
            exec = exec.arg("--dump-candidates");
        } else {
            exec = exec.arg("--verbose");
        }
        for source_file in &test.source_files {
            exec = exec.arg(
                workdir
                    .join(test.subdir.as_deref().unwrap_or("."))
                    .join(source_file),
            );
        }
        exec = exec.args(&test.args);

        exec = exec.stdout(Redirection::Pipe);
        exec = exec.stderr(Redirection::Merge);

        let start = Instant::now();

        let popen = exec.popen().unwrap();
        let mut stdout = popen.stdout.as_ref().unwrap();

        let mut buf = Vec::new();

        let _: usize = stdout.read_to_end(&mut buf).unwrap();

        elapsed += start.elapsed();

        let stdout_actual = std::str::from_utf8(&buf).unwrap();

        // smoelius: Removing the line-column information makes comparing diffs easier.
        let stdout_normalized = remove_timings(&remove_line_columns(&normalize_paths(
            stdout_actual,
            workdir,
        )));

        if enabled("BLESS") {
            write(path_stdout, stdout_normalized).unwrap();
        } else {
            // smoelius: Because source files could be traversed in different orders on different
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
            // smoelius: This choice has subtle implications. For example, suppose the following
            // warnings were emitted, grouped by `X`/`Y`:
            //   Warning: A in X
            //   Warning: C in X
            //   Warning: B in Y
            // This strategy would prefer that the third line appear before the second, even though
            // that would violate the grouping!
            assert!(
                stdout_expected <= stdout_normalized,
                "{}",
                SimpleDiff::from_str(&stdout_expected, &stdout_normalized, "left", "right")
            );
        }

        if test.check_sqlite_urls {
            assert!(test.rev.is_some());
            assert!(!test.parsing_only);
            check_sqlite_urls(workdir, &root, test);
        }
    }

    (output, elapsed)
}

fn check_sqlite_urls(workdir: &Path, root: &Path, test: &Test) {
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
                util::strip_prefix(&span.source_file, workdir)
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

static LINE_COLUMN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^\$DIR/([^:]*):[0-9]+:[0-9]+-[0-9]+:[0-9]+:").unwrap());

fn remove_line_columns(s: &str) -> String {
    LINE_COLUMN_RE.replace_all(s, r"$$DIR/$1:").to_string()
}

// smoelius: Don't put a `\b` at the start of this pattern. `assert_cmd::output::OutputError`
// escapes control characters (e.g., `\t`) and its output appears in the stdout files. So adding a
// `\b` could introduce false negatives.
static TIMING_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[0-9]+\.[0-9]+(m?)s\b").unwrap());

fn remove_timings(s: &str) -> String {
    TIMING_RE.replace_all(s, "[..]${1}s").to_string()
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
                        .is_some_and(|prefix| expected_line.starts_with(prefix))
            })
}

pub fn stdout_subsequence_in(dir: impl AsRef<Path>) {
    for entry in read_dir(dir).unwrap() {
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
    xs: impl IntoIterator<Item = &'a str>,
    ys: impl IntoIterator<Item = &'b str>,
) -> bool {
    let re = Regex::new(r"^(\d+) candidates in (\d+) source file(s)?$").unwrap();

    let mut xs = xs.into_iter().peekable();
    let mut ys = ys.into_iter();

    while let Some(&x) = xs.peek() {
        let Some(y) = ys.next() else {
            dbg!(x);
            return false;
        };
        if x == y || (re.is_match(x) && re.is_match(y)) {
            let _: Option<&str> = xs.next();
        }
    }

    true
}

static BIN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"/[^/]*-[0-9a-f]{16}\b").unwrap());

pub fn stdout_files_are_sanitary_in(dir: impl AsRef<Path>) {
    for entry in read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.extension() != Some(OsStr::new("stdout")) {
            continue;
        }

        let contents = read_to_string(&path).unwrap();

        assert!(
            !TIMING_RE.is_match(&contents),
            "`{}` matches `TIMING_RE`",
            path.display()
        );
        assert!(
            !BIN_RE.is_match(&contents),
            "`{}` matches `BIN_RE`",
            path.display()
        );
    }
}

static SPACE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(" +").unwrap());
static DASH_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new("-+").unwrap());

#[cfg_attr(dylint_lib = "general", allow(non_thread_safe_call_in_test))]
#[test]
fn readme_is_current() {
    let path_readme = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/README.md");

    let readme_actual = read_to_string(&path_readme).unwrap();
    let readme_space_normalized = &SPACE_RE.replace_all(&readme_actual, " ");
    let readme_normalized = DASH_RE.replace_all(readme_space_normalized, "-");

    let mut tests = Vec::new();
    for i in 0..N_PARTITIONS {
        tests.extend(
            read_tests_in(format!("tests/third_party_tests/{i}"), false)
                .into_values()
                .flatten()
                .map(|(toml_path, test)| (toml_path, test, i)),
        );
    }

    let mut test_lines = Vec::new();
    for (toml_path, test, partition) in tests {
        let name = toml_path.file_stem().unwrap();
        test_lines.push(format!(
            "| {} | {}| {}| {}| {}| {}| {}| {partition} |",
            name.to_string_lossy(),
            test.rev.map(|s| s + " ").unwrap_or_default(),
            test.framework.map(|s| s + " ").unwrap_or_default(),
            test.parsing_only.not().to_x_space(),
            target_os_includes(test.target_os.as_ref(), "linux").to_x_space(),
            target_os_includes(test.target_os.as_ref(), "macos").to_x_space(),
            target_os_includes(test.target_os.as_ref(), "windows").to_x_space(),
        ));
    }
    test_lines.sort();

    let mut readme_lines_expected = [
        "# Third-party tests",
        "",
        "| Name | Version | Framework | Full | Linux | macOS | Windows | Partition |",
        "| - | - | - | - | - | - | - | - |",
    ]
    .into_iter()
    .map(ToOwned::to_owned)
    .collect::<Vec<_>>();

    for test_line in test_lines {
        readme_lines_expected.push(test_line);
    }

    let readme_expected = readme_lines_expected
        .into_iter()
        .map(|s| format!("{s}\n"))
        .collect::<String>();

    if enabled("BLESS") {
        write(&path_readme, readme_expected).unwrap();
        let tempdir = tempdir().unwrap();
        assert!(
            Command::new("npm")
                .args(["install", "prettier"])
                .current_dir(&tempdir)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("npx")
                .args(["prettier", "--write", &path_readme.to_string_lossy()])
                .current_dir(&tempdir)
                .status()
                .unwrap()
                .success()
        );
    } else {
        assert_eq!(readme_expected, readme_normalized);
    }
}

trait ToXSpace {
    fn to_x_space(&self) -> &'static str;
}

impl ToXSpace for bool {
    fn to_x_space(&self) -> &'static str {
        if *self { "X " } else { "" }
    }
}

fn target_os_includes(target_os: Option<&StringOrVec>, os: &str) -> bool {
    let Some(target_os) = target_os else {
        return true;
    };
    target_os.get().iter().any(|s| s == os)
}

fn enabled(key: &str) -> bool {
    var(key).is_ok_and(|value| value != "0")
}
