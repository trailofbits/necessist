//! Cases for `--dump --filtered`.
//!
//! Each case works in its own temporary directory. The Rust projects are written from scratch (see
//! [`Project`]); only the PHP one is copied, from `fixtures/php_basic`, for its `composer.json` and
//! `vendor/`, and even then its source file is overwritten. Nothing here is written back to
//! `fixtures/`, so these cases neither dirty the working tree -- `dogfood` skips itself when
//! `git diff --exit-code` is non-empty -- nor race the cases that mutilate the fixtures in place.
//!
//! `BASIC_LIB`, `DRF_LIB`, `DRF_B` and `PHP_TEST` reproduce fixture files byte for byte. `DRF_A`
//! adds one line, `let _ = ["x"].join("");`, so that `tests/a.rs` contributes a skipped method
//! call and not only a skipped statement. The remaining literals have no fixture counterpart.

use assert_cmd::{assert::OutputAssertExt, cargo::cargo_bin_cmd};
use elaborate::std::{
    fs::{create_dir_all_wc, read_to_string_wc, remove_dir_all_wc, remove_file_wc, write_wc},
    path::PathContext,
    process::ExitStatusContext,
};
use predicates::prelude::*;
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};
use testing::tempfile_util::{TempDir, tempdir};

use super::TIMEOUT;

/// One candidate that `ignored_functions` removes, and one that nothing removes.
const IGNORED_FUNCTION_LIB: &str = r"fn ignored_function<T>(_: T) {}

fn foo() -> u32 {
    1
}

#[test]
fn test() {
    let mut n = 0;
    n += 1;
    ignored_function(foo());
    assert!(n > 0);
}
";

/// The PHP fixture is the one project these cases cannot synthesize: it needs `composer.json` and
/// an installed `vendor/`. Its one source file is overwritten from [`PHP_TEST`] after copying, so
/// nothing under test is ever read from `fixtures/`.
const PHP_ROOT: &str = "fixtures/php_basic";

/// Four removal candidates: three statements and one method call.
const BASIC_LIB: &str = r#"#[test]
fn passed() {
    let mut n = 0;
    n += 1;
    noop();
}

fn noop() {}

#[test]
fn timed_out() {
    let mut n = 0;
    while n < 1 {
        n += 1;
    }
}

#[test]
fn failed() {
    let mut n = 0;
    n += 1;
    assert!(n >= 1);
}

#[test]
fn nonbuildable() {
    let _ = |xs: &[&str]| -> String {
        return xs.join("");
    };
}
"#;

/// Three removal candidates in three source files. `tests/a.rs` fails on its own, so its dry run
/// fails and both of its candidates -- one statement and one method call -- are recorded as
/// `Skipped`. The method call is there so that the `Skipped` cases are not all statements.
const DRF_LIB: &str = "\n";
const DRF_A: &str = r#"#[test]
fn dry_run_failed() {
    let mut n = 0;
    n += 1;
    let _ = ["x"].join("");
    assert!(n >= 2);
}
"#;
const DRF_B: &str = r"#[test]
fn passed() {
    let mut n = 0;
    n += 1;
    noop();
}

fn noop() {}
";

/// Four multi-line candidates. The first two (`v.extend(..)`) have an interior line indented with
/// a tab; the second two (`v.push(..)`) do not, and share no distinguishing token with the first.
/// Rewriting the file -- with CRLF endings, or with the tab expanded -- changes text without
/// moving spans, and the tab case leaves the second pair untouched.
const WHITESPACE_LIB: &str = "#[test]\nfn multiline() {\n    let mut v = Vec::new();\n    \
                              v.extend(\n\t[1, 2],\n    );\n    v.push(\n        3,\n    );\n    \
                              assert_eq!(3, v.len());\n}\n";

/// Four candidates. The first two can be changed in ASCII case alone, without moving their spans
/// and without breaking the build; the second two are left as they are, so that a check rejecting
/// everything that looks like them would be caught.
const CASE_LIB: &str = r#"#[test]
fn cased() {
    let mut s = String::new();
    s.push_str("ab");
    s.push_str("cd");
    assert!(!s.is_empty());
}
"#;

/// The test writes a marker every time it runs. The path is substituted in by
/// [`Project::create`] and points outside the project, so that running the project from a copy,
/// or deleting the project afterwards, cannot hide the evidence.
const MARKER_LIB_TEMPLATE: &str = r#"#[test]
fn marks() {
    std::fs::write("{MARKER}", "").unwrap();
    let mut n = 0;
    n += 1;
    assert!(n > 0);
}
"#;

const PHP_TEST: &str = r"<?php

use PHPUnit\Framework\TestCase;

class BasicTest extends TestCase
{
    public function testPassed()
    {
        $n = 0;
        $n += 1;
        noop();
    }

    public function testFailed()
    {
        $n = 0;
        $n += 1;
        $this->assertTrue($n >= 1);
    }
}

function noop() {}
";

/// Which project [`dump_filtered_setup`] should create
#[derive(Clone, Copy)]
enum Project {
    /// Synthesized; see [`BASIC_LIB`]
    Basic,
    /// Synthesized; see [`DRF_A`] and [`DRF_B`]
    DryRunFailure,
    /// Copied from `fixtures/php_basic` for its `composer.json` and `vendor/`, with its one source
    /// file overwritten
    Php,
    /// Synthesized; see [`WHITESPACE_LIB`]
    Whitespace,
    /// Synthesized; see [`CASE_LIB`]
    Case,
    /// Synthesized; see [`IGNORED_FUNCTION_LIB`]
    IgnoredFunction,
    /// Synthesized; see [`MARKER_LIB_TEMPLATE`]
    Marker,
}

impl Project {
    fn dir_name(self) -> &'static str {
        match self {
            Self::Basic => "basic",
            Self::DryRunFailure => "dry_run_failure",
            Self::Php => "php_basic",
            Self::Whitespace => "whitespace",
            Self::Case => "case",
            Self::IgnoredFunction => "ignored_function",
            Self::Marker => "marker",
        }
    }

    fn timeout(self) -> &'static str {
        match self {
            // PHPUnit through composer is slower to start than the Rust fixtures.
            Self::Php => "60",
            _ => TIMEOUT,
        }
    }

    /// Creates the project under `root`
    ///
    /// The Rust projects are written from scratch rather than copied. Other cases in this target
    /// mutilate `fixtures/basic` and `fixtures/dry_run_failure` in place while they run, so
    /// copying from them races: the copy can pick up an instrumented `src/lib.rs`, whose tests
    /// then fail with `NECESSIST_REMOVAL` unset, and every entry is recorded as `Skipped`.
    /// Writing the sources here removes the shared mutable input instead of trying to time
    /// around it.
    fn create(self, root: &Path) {
        match self {
            Self::Basic => {
                write_manifest(root, "basic");
                write_source(root, "src/lib.rs", BASIC_LIB);
            }
            Self::Whitespace => {
                write_manifest(root, "whitespace");
                write_source(root, "src/lib.rs", WHITESPACE_LIB);
            }
            Self::Case => {
                write_manifest(root, "case");
                write_source(root, "src/lib.rs", CASE_LIB);
            }
            Self::IgnoredFunction => {
                write_manifest(root, "ignored_function");
                write_source(root, "src/lib.rs", IGNORED_FUNCTION_LIB);
            }
            Self::Marker => {
                write_manifest(root, "marker");
                write_source(
                    root,
                    "src/lib.rs",
                    &MARKER_LIB_TEMPLATE.replace("{MARKER}", &marker_path(root)),
                );
            }
            Self::DryRunFailure => {
                write_manifest(root, "dry_run_failure");
                write_source(root, "src/lib.rs", DRF_LIB);
                write_source(root, "tests/a.rs", DRF_A);
                write_source(root, "tests/b.rs", DRF_B);
            }
            Self::Php => {
                Command::new("cp")
                    .args(["-r", PHP_ROOT, &root.parent_wc().unwrap().to_string_lossy()])
                    .assert()
                    .success();
                for name in ["NECESSIST_LOCK", "necessist.db"] {
                    let path_buf = root.join(name);
                    if path_buf.try_exists_wc().unwrap() {
                        remove_file_wc(&path_buf).unwrap();
                    }
                }
                let target = root.join("target");
                if target.is_dir() {
                    remove_dir_all_wc(&target).unwrap();
                }
                // Overwrite the one file under test, so an instrumented copy cannot leak in.
                write_source(root, "tests/BasicTest.php", PHP_TEST);
            }
        }
    }
}

/// Where [`Project::Marker`]'s test writes its marker: a sibling of the project directory, so it
/// survives the project being copied, moved or deleted. Backslashes are escaped because the path
/// is substituted into a Rust string literal.
fn marker_path(root: &Path) -> String {
    #[allow(clippy::unwrap_used)]
    root.parent_wc()
        .unwrap()
        .join("necessist-ran-marker")
        .to_string_lossy()
        .replace('\\', "\\\\")
}

fn write_manifest(root: &Path, name: &str) {
    write_source(
        root,
        "Cargo.toml",
        &format!(
            "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = \
             false\n\n[workspace]\n"
        ),
    );
}

fn write_source(root: &Path, relative_path: &str, contents: &str) {
    let path_buf = root.join(relative_path);
    create_dir_all_wc(path_buf.parent_wc().unwrap()).unwrap();
    write_wc(&path_buf, contents).unwrap();
}

/// Returns `output`'s stdout with `\\` rewritten to `/`.
///
/// The cases below name paths the way the sources do, `src/lib.rs`. Necessist prints a path it
/// cannot make relative to the current directory as an absolute one, and each case runs in a
/// temporary directory, so what is printed is an absolute path spelled with the platform's
/// separator. Rewriting it here lets one spelling of an expectation serve every platform.
///
/// Nothing else in this file's output contains a backslash: the sources written by [`Project`]
/// have none, so the rewrite cannot touch a fragment an expectation matches on.
fn stdout_of(output: &Output) -> String {
    #[allow(clippy::unwrap_used)]
    std::str::from_utf8(&output.stdout)
        .unwrap()
        .replace('\\', "/")
}

/// Creates `project` in a fresh temporary directory and runs Necessist in it to populate
/// `necessist.db`
fn dump_filtered_setup(project: Project) -> (TempDir, PathBuf) {
    dump_filtered_setup_with(project, |_| {})
}

/// Like [`dump_filtered_setup`], but calls `before_run` on the project before the run that
/// populates `necessist.db`. Use it when the entries themselves have to differ.
fn dump_filtered_setup_with(
    project: Project,
    before_run: impl FnOnce(&Path),
) -> (TempDir, PathBuf) {
    let tempdir = tempdir().unwrap();

    let root = tempdir.path().join(project.dir_name());
    project.create(&root);

    before_run(&root);

    let assert = cargo_bin_cmd!("necessist")
        .args([
            "--root",
            &root.to_string_lossy(),
            "--timeout",
            project.timeout(),
        ])
        .assert()
        .success();

    // A failed dry run is only a warning, but it would record every entry as `Skipped` and make
    // the outcomes these cases assert on meaningless. Fail here instead, where the cause is
    // visible.
    let stdout = stdout_of(assert.get_output());
    assert!(
        stdout.contains("mutilating"),
        "the run that populates `necessist.db` did not get past its dry run: {stdout:?}"
    );

    (tempdir, root)
}

// Entries whose spans lie beyond the end of a shortened file must be dropped rather than shown.
// `Span::source_text` returns an error for such a span, so this also pins that the error is
// treated as a mismatch rather than propagated, and that the run still succeeds.
#[test]
fn dump_filtered_following_source_change() {
    let (_tempdir, root) = dump_filtered_setup(Project::Basic);

    // The absence of a trailing newline is deliberate: it is what makes the stored offsets exceed
    // the file's length.
    write_wc(
        root.join("src/lib.rs"),
        "\
#[test]
fn passed() {
    let mut n = 0;
    n += 1;
    noop();
}

fn noop() {}",
    )
    .unwrap();

    let assert = cargo_bin_cmd!("necessist")
        .args([
            "--root",
            &root.to_string_lossy(),
            "--dump",
            "--filtered",
            "--verbose",
        ])
        .assert()
        .success();

    let stdout = stdout_of(assert.get_output());
    assert_eq!(1, stdout.matches("`n += 1;` passed").count(), "{stdout:?}");
    assert!(!stdout.contains("timed-out"), "{stdout:?}");
    assert!(!stdout.contains("failed"), "{stdout:?}");
    assert!(!stdout.contains("nonbuildable"), "{stdout:?}");
    assert!(
        stdout.contains("3 entries were hidden by filtering"),
        "{stdout:?}"
    );
}

// Without `--verbose`, the "more output would be produced" note must be tied to what the filter
// let through, not to what the database happens to contain. Here every entry whose outcome is
// not `passed` is filtered out, so `--verbose` would add nothing and the note must not appear.
#[test]
fn dump_filtered_does_not_suggest_verbose_when_it_would_add_nothing() {
    let (_tempdir, root) = dump_filtered_setup(Project::Basic);

    write_wc(
        root.join("necessist.toml"),
        "ignored_tests = [\"timed_out\", \"failed\", \"nonbuildable\"]\n",
    )
    .unwrap();

    let assert = cargo_bin_cmd!("necessist")
        .args(["--root", &root.to_string_lossy(), "--dump", "--filtered"])
        .assert()
        .success();

    let stdout = stdout_of(assert.get_output());
    assert!(stdout.contains("`n += 1;` passed"), "{stdout:?}");
    assert!(
        !stdout.contains("more output would be produced"),
        "{stdout:?}"
    );
    // The three filtered entries would not have printed anyway, so none of them is "hidden".
    assert!(!stdout.contains("hidden by filtering"), "{stdout:?}");
}

// Entries excluded by configuration must be dropped even though their spans are unchanged and
// their text still matches. This is the case that an implementation checking only the text would
// get wrong.
#[test]
fn dump_filtered_excludes_configured_out_candidates() {
    let (_tempdir, root) = dump_filtered_setup(Project::Basic);

    write_wc(
        root.join("necessist.toml"),
        "ignored_tests = [\"timed_out\", \"failed\"]\n",
    )
    .unwrap();

    let assert = cargo_bin_cmd!("necessist")
        .args([
            "--root",
            &root.to_string_lossy(),
            "--dump",
            "--filtered",
            "--verbose",
        ])
        .assert()
        .success();

    let stdout = stdout_of(assert.get_output());
    assert!(stdout.contains("src/lib.rs:4:5-4:12"), "{stdout:?}");
    assert!(stdout.contains("src/lib.rs:28:18-28:27"), "{stdout:?}");
    assert!(!stdout.contains("src/lib.rs:14:9-14:16"), "{stdout:?}");
    assert!(!stdout.contains("src/lib.rs:21:5-21:12"), "{stdout:?}");
    assert!(
        stdout.contains("2 entries were hidden by filtering"),
        "{stdout:?}"
    );
}

// An entry whose span is unchanged but whose text was rewritten in place must be dropped. This is
// the case that an implementation checking only the span would get wrong.
#[test]
fn dump_filtered_excludes_rewritten_span() {
    let (_tempdir, root) = dump_filtered_setup(Project::Basic);

    let path_buf = root.join("src/lib.rs");
    let contents = read_to_string_wc(&path_buf).unwrap();
    // `n += 2;` occupies exactly the columns that `n += 1;` did.
    let contents = contents.replacen("    n += 1;\n    noop();", "    n += 2;\n    noop();", 1);
    write_wc(&path_buf, &contents).unwrap();

    let assert = cargo_bin_cmd!("necessist")
        .args([
            "--root",
            &root.to_string_lossy(),
            "--dump",
            "--filtered",
            "--verbose",
        ])
        .assert()
        .success();

    let stdout = stdout_of(assert.get_output());
    assert!(!stdout.contains("src/lib.rs:4:5-4:12"), "{stdout:?}");
    assert!(stdout.contains("src/lib.rs:14:9-14:16"), "{stdout:?}");
    assert!(stdout.contains("src/lib.rs:21:5-21:12"), "{stdout:?}");
    assert!(stdout.contains("src/lib.rs:28:18-28:27"), "{stdout:?}");
    assert!(
        stdout.contains("1 entry was hidden by filtering"),
        "{stdout:?}"
    );
}

// Entries that `skip_present_spans` recorded as `Skipped` must be shown like any other.
#[test]
fn dump_filtered_includes_skipped_outcomes() {
    let (_tempdir, root) = dump_filtered_setup(Project::DryRunFailure);

    let assert = cargo_bin_cmd!("necessist")
        .args([
            "--root",
            &root.to_string_lossy(),
            "--dump",
            "--filtered",
            "--verbose",
        ])
        .assert()
        .success();

    let stdout = stdout_of(assert.get_output());
    assert!(stdout.contains("`n += 1;` skipped"), "{stdout:?}");
    assert!(stdout.contains("`n += 1;` passed"), "{stdout:?}");
    assert!(!stdout.contains("hidden by filtering"), "{stdout:?}");
}

// The candidate set is whatever the invocation parses: a named file narrows it, a named directory
// brings in what the backend selects under it, and no path at all falls back to the project root.
// A test of the first alone would pass against a help text that promised only it.
//
// What a directory contributes is the backend's own business -- extensions, `vendor`, `.gitignore`
// -- and that is not exercised here; this runs on the Rust backend only.
#[test]
fn dump_filtered_is_limited_to_the_files_passed() {
    let (_tempdir, root) = dump_filtered_setup(Project::DryRunFailure);

    let dump_filtered = |paths: &[&str]| {
        let mut command = cargo_bin_cmd!("necessist");
        command.args([
            "--root",
            &root.to_string_lossy(),
            "--dump",
            "--filtered",
            "--verbose",
        ]);
        command.args(paths);
        let assert = command.assert().success();
        stdout_of(assert.get_output())
    };

    // A single file: the entries of the other file are hidden.
    let stdout = dump_filtered(&["tests/b.rs"]);
    assert!(stdout.contains("tests/b.rs:4:5-4:12"), "{stdout:?}");
    assert!(!stdout.contains("tests/a.rs"), "{stdout:?}");
    assert!(
        stdout.contains("2 entries were hidden by filtering"),
        "{stdout:?}"
    );

    // A directory: what the backend selects under it, which here is both files.
    let stdout = dump_filtered(&["tests"]);
    assert!(stdout.contains("tests/a.rs:4:5-4:12"), "{stdout:?}");
    assert!(stdout.contains("tests/b.rs:4:5-4:12"), "{stdout:?}");
    assert!(!stdout.contains("hidden by filtering"), "{stdout:?}");

    // No path at all: the project root, so again both files.
    let stdout = dump_filtered(&[]);
    assert!(stdout.contains("tests/a.rs:4:5-4:12"), "{stdout:?}");
    assert!(stdout.contains("tests/b.rs:4:5-4:12"), "{stdout:?}");
    assert!(!stdout.contains("hidden by filtering"), "{stdout:?}");

    // A directory with no test file under it: all three entries are hidden.
    let stdout = dump_filtered(&["src"]);
    assert!(!stdout.contains(":4:5-4:12"), "{stdout:?}");
    assert!(
        stdout.contains("3 entries were hidden by filtering"),
        "{stdout:?}"
    );
}

// A method call excluded by configuration must be dropped even though its span and text are
// unchanged. `fixtures/basic` records one method call, `.join("")`; without a case like this,
// every exclusion the suite exercises is a statement.
#[test]
fn dump_filtered_excludes_configured_out_method_call() {
    let (_tempdir, root) = dump_filtered_setup(Project::Basic);

    write_wc(
        root.join("necessist.toml"),
        "ignored_methods = [\"join\"]\n",
    )
    .unwrap();

    let assert = cargo_bin_cmd!("necessist")
        .args([
            "--root",
            &root.to_string_lossy(),
            "--dump",
            "--filtered",
            "--verbose",
        ])
        .assert()
        .success();

    let stdout = stdout_of(assert.get_output());
    assert!(!stdout.contains("src/lib.rs:28:18-28:27"), "{stdout:?}");
    assert!(stdout.contains("src/lib.rs:4:5-4:12"), "{stdout:?}");
    assert!(stdout.contains("src/lib.rs:14:9-14:16"), "{stdout:?}");
    assert!(stdout.contains("src/lib.rs:21:5-21:12"), "{stdout:?}");
    assert!(
        stdout.contains("1 entry was hidden by filtering"),
        "{stdout:?}"
    );
}

// A method call rewritten within its own columns must be dropped, just as a statement is.
// `.concat()` occupies exactly the columns that `.join("")` did.
#[test]
fn dump_filtered_excludes_rewritten_method_call() {
    let (_tempdir, root) = dump_filtered_setup(Project::Basic);

    let path_buf = root.join("src/lib.rs");
    let contents = read_to_string_wc(&path_buf).unwrap();
    assert!(contents.contains(".join(\"\")"), "{contents:?}");
    let contents = contents.replacen(".join(\"\")", ".concat()", 1);
    write_wc(&path_buf, &contents).unwrap();

    let assert = cargo_bin_cmd!("necessist")
        .args([
            "--root",
            &root.to_string_lossy(),
            "--dump",
            "--filtered",
            "--verbose",
        ])
        .assert()
        .success();

    let stdout = stdout_of(assert.get_output());
    assert!(!stdout.contains("src/lib.rs:28:18-28:27"), "{stdout:?}");
    assert!(stdout.contains("src/lib.rs:4:5-4:12"), "{stdout:?}");
    assert!(
        stdout.contains("1 entry was hidden by filtering"),
        "{stdout:?}"
    );
}

// Without `--verbose`, the hidden count must still cover a filtered-out `passed` entry, because
// that entry is one that would have been printed. Every other hidden-count case in this suite
// runs with `--verbose`.
#[test]
fn dump_filtered_counts_hidden_passed_without_verbose() {
    let (_tempdir, root) = dump_filtered_setup(Project::Basic);

    write_wc(
        root.join("necessist.toml"),
        "ignored_tests = [\"passed\"]\n",
    )
    .unwrap();

    let assert = cargo_bin_cmd!("necessist")
        .args(["--root", &root.to_string_lossy(), "--dump", "--filtered"])
        .assert()
        .success();

    let stdout = stdout_of(assert.get_output());
    assert!(
        stdout.contains("1 entry was hidden by filtering"),
        "{stdout:?}"
    );
    assert!(!stdout.contains("src/lib.rs:4:5-4:12"), "{stdout:?}");
    assert!(
        stdout.contains("more output would be produced with --verbose"),
        "{stdout:?}"
    );
}

// The note explaining what a shown entry means must be printed even when nothing is shown and
// nothing is hidden.
#[test]
fn dump_filtered_notes_meaning_with_no_entries() {
    let tempdir = tempdir().unwrap();
    let root = tempdir.path().join(Project::Basic.dir_name());
    Project::Basic.create(&root);

    write_wc(
        root.join("necessist.toml"),
        "ignored_tests = [\"passed\", \"timed_out\", \"failed\", \"nonbuildable\"]\n",
    )
    .unwrap();

    // With every test ignored there are no candidates, so the run records no entries. This is the
    // one case that cannot use `dump_filtered_setup`, whose guard requires the run to mutilate.
    cargo_bin_cmd!("necessist")
        .args(["--root", &root.to_string_lossy(), "--timeout", TIMEOUT])
        .assert()
        .success();

    let assert = cargo_bin_cmd!("necessist")
        .args(["--root", &root.to_string_lossy(), "--dump", "--filtered"])
        .assert()
        .success();

    let stdout = stdout_of(assert.get_output());
    assert!(
        stdout.contains("an entry is shown when its span is still a removal candidate"),
        "{stdout:?}"
    );
    assert!(!stdout.contains("hidden by filtering"), "{stdout:?}");
    assert!(!stdout.contains("--verbose"), "{stdout:?}");
}

// A source file that no longer parses is not an error: its entries simply stop being candidates.
// This is the case that distinguishes `--dump --filtered` from a plain `--dump`.
#[test]
fn dump_filtered_hides_everything_when_parsing_fails() {
    let (_tempdir, root) = dump_filtered_setup(Project::Basic);

    let path_buf = root.join("src/lib.rs");
    let contents = read_to_string_wc(&path_buf).unwrap();
    write_wc(&path_buf, contents + "\nfn (((\n").unwrap();

    let assert = cargo_bin_cmd!("necessist")
        .args([
            "--root",
            &root.to_string_lossy(),
            "--dump",
            "--filtered",
            "--verbose",
        ])
        .assert()
        .success();

    let stdout = stdout_of(assert.get_output());
    assert!(stdout.contains("failed to parse"), "{stdout:?}");
    assert!(!stdout.contains("src/lib.rs:4:5-4:12"), "{stdout:?}");
    assert!(
        stdout.contains("4 entries were hidden by filtering"),
        "{stdout:?}"
    );
}

// `--dump --filtered` inherits a plain `--dump`'s behavior on a deleted source file: loading the
// database reads every recorded file, so the run fails. Recorded here so that no one "fixes" it by
// adding a fallback.
#[test]
fn dump_filtered_fails_on_deleted_source_file() {
    let (_tempdir, root) = dump_filtered_setup(Project::Basic);

    remove_file_wc(root.join("src/lib.rs")).unwrap();

    let outcome = |flags: &[&str]| {
        let output = cargo_bin_cmd!("necessist")
            .args(["--root", &root.to_string_lossy()])
            .args(flags)
            .assert()
            .failure()
            .get_output()
            .clone();
        let code = output.status.code_wc().unwrap();
        let stderr = std::str::from_utf8(&output.stderr).unwrap().to_owned();
        let stdout = stdout_of(&output);
        (code, stdout, stderr)
    };

    let (filtered_code, filtered_stdout, filtered_stderr) = outcome(&["--dump", "--filtered"]);
    let (dump_code, dump_stdout, dump_stderr) = outcome(&["--dump"]);

    // "Inherits the limitation" is a claim about equality, so compare, do not merely look for a
    // substring in each.
    assert_eq!(dump_code, filtered_code, "{filtered_stderr:?}");
    assert_eq!(2, filtered_code, "{filtered_stderr:?}");
    assert_eq!(dump_stdout, filtered_stdout, "{filtered_stdout:?}");
    assert_eq!("", filtered_stdout, "{filtered_stdout:?}");
    assert_eq!(dump_stderr, filtered_stderr, "{filtered_stderr:?}");
    assert!(
        filtered_stderr.contains("read_to_string"),
        "{filtered_stderr:?}"
    );
}

// The help text has to say how the candidate set is chosen, including what happens when no path
// is given. `readme_contains_usage` only proves that README and `--help` agree; it would pass
// just as well if both omitted this, or if both described a rule the code does not follow.
#[test]
fn filtered_help_states_the_file_limitation() {
    let assert = cargo_bin_cmd!("necessist").arg("--help").assert().success();

    let stdout = stdout_of(assert.get_output());
    let line = stdout
        .lines()
        .find(|line| line.trim_start().starts_with("--filtered"))
        .unwrap();
    // Compared whole, not by substrings: a wording that kept every substring while saying
    // something misleading around them would pass a substring check.
    assert_eq!(
        "--filtered               Restrict --dump to entries that are still removal candidates \
         and whose recorded text still matches what Necessist read at their span; the candidates \
         are the ones the framework finds in the paths given on the command line, or under the \
         project root when none is given, by the same file selection and ignore rules a normal \
         run uses, so naming fewer paths can hide more entries",
        line.trim_start()
    );
}

// The suite's only fixture method call is `.join("")`. A check that happens to accept that one
// spelling would pass everything else, so this case adds a second method call of a different
// shape and excludes only it.
#[test]
fn dump_filtered_excludes_configured_out_method_call_of_another_shape() {
    let (_tempdir, root) = dump_filtered_setup_with(Project::Basic, |root| {
        let path_buf = root.join("src/lib.rs");
        let contents = read_to_string_wc(&path_buf).unwrap();
        write_wc(
            &path_buf,
            contents
                + "
#[test]
fn appended() {
    let mut s = String::new();
    s.push_str(\"x\");
    assert!(!s.is_empty());
}
",
        )
        .unwrap();
    });

    write_wc(
        root.join("necessist.toml"),
        "ignored_methods = [\"push_str\"]\n",
    )
    .unwrap();

    let assert = cargo_bin_cmd!("necessist")
        .args([
            "--root",
            &root.to_string_lossy(),
            "--dump",
            "--filtered",
            "--verbose",
        ])
        .assert()
        .success();

    let stdout = stdout_of(assert.get_output());
    // `.join("")` is not excluded, so it must still be shown ...
    assert!(stdout.contains("`.join(\"\")` nonbuildable"), "{stdout:?}");
    // ... while neither the excluded method call nor the statement containing it may be.
    assert!(!stdout.contains("push_str"), "{stdout:?}");
    assert!(
        stdout.contains("2 entries were hidden by filtering"),
        "{stdout:?}"
    );
}

// The same requirement as the CRLF case, for a different kind of whitespace: expanding the tab
// that indents the interior line leaves both spans alone and changes only their bytes.
#[test]
fn dump_filtered_excludes_a_candidate_whose_indentation_changed() {
    let (_tempdir, root) = dump_filtered_setup(Project::Whitespace);

    let path_buf = root.join("src/lib.rs");
    let contents = read_to_string_wc(&path_buf).unwrap();
    assert!(
        contents.contains('\t'),
        "the project must start out tab-indented"
    );
    write_wc(&path_buf, contents.replace('\t', "    ")).unwrap();

    let assert = cargo_bin_cmd!("necessist")
        .args([
            "--root",
            &root.to_string_lossy(),
            "--dump",
            "--filtered",
            "--verbose",
        ])
        .assert()
        .success();

    let stdout = stdout_of(assert.get_output());
    // Only the pair whose interior line held the tab changed ...
    assert!(!stdout.contains("src/lib.rs:4:5-6:7"), "{stdout:?}");
    assert!(!stdout.contains("src/lib.rs:4:6-6:6"), "{stdout:?}");
    // ... the other pair is untouched and must still be shown.
    assert!(stdout.contains("src/lib.rs:7:5-9:7"), "{stdout:?}");
    assert!(stdout.contains("src/lib.rs:7:6-9:6"), "{stdout:?}");
    assert!(
        stdout.contains("2 entries were hidden by filtering"),
        "{stdout:?}"
    );
}

// Without `--verbose`, a filtered-out `Skipped` entry is not "hidden": it would not have been
// printed either way. Every other case involving `Skipped` runs with `--verbose`.
#[test]
fn dump_filtered_does_not_count_a_hidden_skipped_entry_without_verbose() {
    let (_tempdir, root) = dump_filtered_setup(Project::DryRunFailure);

    // `tests/a.rs` is the file whose dry run fails, so its entry is the `Skipped` one. Passing
    // only `tests/b.rs` filters it out.
    let assert = cargo_bin_cmd!("necessist")
        .args([
            "--root",
            &root.to_string_lossy(),
            "--dump",
            "--filtered",
            "tests/b.rs",
        ])
        .assert()
        .success();

    let stdout = stdout_of(assert.get_output());
    assert!(stdout.contains("tests/b.rs:4:5-4:12"), "{stdout:?}");
    assert!(!stdout.contains("tests/a.rs"), "{stdout:?}");
    assert!(!stdout.contains("hidden by filtering"), "{stdout:?}");
    assert!(
        !stdout.contains("more output would be produced"),
        "{stdout:?}"
    );
}

// Byte-for-byte means byte-for-byte: a difference of ASCII case alone is still a difference.
// Every other rejection case in this module differs in something else -- a digit, a method name,
// line endings, indentation -- so none of them would notice a case-insensitive comparison.
#[test]
fn dump_filtered_excludes_a_candidate_whose_case_changed() {
    let (_tempdir, root) = dump_filtered_setup(Project::Case);

    let path_buf = root.join("src/lib.rs");
    let contents = read_to_string_wc(&path_buf).unwrap();
    // `"AB"` occupies exactly the columns `"ab"` did, and still compiles.
    write_wc(&path_buf, contents.replace("\"ab\"", "\"AB\"")).unwrap();

    let assert = cargo_bin_cmd!("necessist")
        .args([
            "--root",
            &root.to_string_lossy(),
            "--dump",
            "--filtered",
            "--verbose",
        ])
        .assert()
        .success();

    let stdout = stdout_of(assert.get_output());
    // The pair whose literal changed case ...
    assert!(!stdout.contains("src/lib.rs:4:5-4:22"), "{stdout:?}");
    assert!(!stdout.contains("src/lib.rs:4:6-4:21"), "{stdout:?}");
    // ... and the pair that did not, which shares the same shape and must still be shown.
    assert!(stdout.contains("src/lib.rs:5:5-5:22"), "{stdout:?}");
    assert!(stdout.contains("src/lib.rs:5:6-5:21"), "{stdout:?}");
    assert!(
        stdout.contains("2 entries were hidden by filtering"),
        "{stdout:?}"
    );
}

// The positive counterpart to the whitespace cases. Every multi-line candidate elsewhere in this
// module is deliberately altered and must be hidden, so an implementation that rejected every
// candidate containing a newline would pass all of them. These are untouched and must be shown --
// all four, including the `v.push(..)` pair, which shares no token with the `v.extend(..)` pair,
// so accepting multi-line candidates by some property of the latter would not do either.
#[test]
fn dump_filtered_includes_an_unchanged_multi_line_candidate() {
    let (_tempdir, root) = dump_filtered_setup(Project::Whitespace);

    let assert = cargo_bin_cmd!("necessist")
        .args([
            "--root",
            &root.to_string_lossy(),
            "--dump",
            "--filtered",
            "--verbose",
        ])
        .assert()
        .success();

    let stdout = stdout_of(assert.get_output());
    for span in ["4:5-6:7", "4:6-6:6", "7:5-9:7", "7:6-9:6"] {
        assert!(stdout.contains(span), "{span}: {stdout:?}");
    }
    assert!(!stdout.contains("hidden by filtering"), "{stdout:?}");
}

// A `Skipped` entry is filtered by the same text rule as any other. Nothing else exercises that:
// the case that shows `Skipped` entries leaves their text alone, and the case that hides one
// removes its file from the candidate map before the text is ever compared.
#[test]
fn dump_filtered_excludes_a_rewritten_skipped_entry() {
    let (_tempdir, root) = dump_filtered_setup(Project::DryRunFailure);

    // `tests/a.rs` is the file whose dry run fails, so its entry is the `Skipped` one. `n += 2;`
    // occupies exactly the columns `n += 1;` did, so the span stays a candidate.
    let path_buf = root.join("tests/a.rs");
    let contents = read_to_string_wc(&path_buf).unwrap();
    write_wc(&path_buf, contents.replace("n += 1;", "n += 2;")).unwrap();

    let assert = cargo_bin_cmd!("necessist")
        .args([
            "--root",
            &root.to_string_lossy(),
            "--dump",
            "--filtered",
            "--verbose",
        ])
        .assert()
        .success();

    let stdout = stdout_of(assert.get_output());
    // The rewritten statement is gone ...
    assert!(!stdout.contains("tests/a.rs:4:5-4:12"), "{stdout:?}");
    // ... while the untouched method call in the same file, and `tests/b.rs`, remain.
    assert!(stdout.contains("tests/a.rs:5:18-5:27"), "{stdout:?}");
    assert!(stdout.contains("tests/b.rs:4:5-4:12"), "{stdout:?}");
    assert!(
        stdout.contains("1 entry was hidden by filtering"),
        "{stdout:?}"
    );
}

// The text rule applies to a skipped method call too, not just a skipped statement. Without
// this, a check that waved through `Skipped` method calls would pass everything: every other
// `Skipped` candidate in this module is a statement.
#[test]
fn dump_filtered_excludes_a_rewritten_skipped_method_call() {
    let (_tempdir, root) = dump_filtered_setup(Project::DryRunFailure);

    // `.concat()` occupies exactly the columns `.join("")` did, and still compiles.
    let path_buf = root.join("tests/a.rs");
    let contents = read_to_string_wc(&path_buf).unwrap();
    write_wc(&path_buf, contents.replace(".join(\"\")", ".concat()")).unwrap();

    let assert = cargo_bin_cmd!("necessist")
        .args([
            "--root",
            &root.to_string_lossy(),
            "--dump",
            "--filtered",
            "--verbose",
        ])
        .assert()
        .success();

    let stdout = stdout_of(assert.get_output());
    // The rewritten method call is gone ...
    assert!(!stdout.contains("tests/a.rs:5:18-5:27"), "{stdout:?}");
    // ... while the untouched statement in the same file, and `tests/b.rs`, remain.
    assert!(stdout.contains("tests/a.rs:4:5-4:12"), "{stdout:?}");
    assert!(stdout.contains("tests/b.rs:4:5-4:12"), "{stdout:?}");
    assert!(
        stdout.contains("1 entry was hidden by filtering"),
        "{stdout:?}"
    );
}

// A third filtering mechanism, `ignored_functions`, reaches the flag as well. Without this the
// suite would exercise only `ignored_tests` and `ignored_methods`, and an implementation whose
// parse skipped just `ignored_functions` would pass everything.
#[test]
fn dump_filtered_excludes_a_configured_out_function_call() {
    let (_tempdir, root) = dump_filtered_setup(Project::IgnoredFunction);

    write_wc(
        root.join("necessist.toml"),
        "ignored_functions = [\"ignored_function\"]\n",
    )
    .unwrap();

    let assert = cargo_bin_cmd!("necessist")
        .args([
            "--root",
            &root.to_string_lossy(),
            "--dump",
            "--filtered",
            "--verbose",
        ])
        .assert()
        .success();

    let stdout = stdout_of(assert.get_output());
    assert!(!stdout.contains("ignored_function(foo());"), "{stdout:?}");
    assert!(stdout.contains("`n += 1;`"), "{stdout:?}");
    assert!(
        stdout.contains("1 entry was hidden by filtering"),
        "{stdout:?}"
    );
}

// The candidate set must come from whatever backend is active, not from Rust alone. Every other
// case in this module runs on a Rust project, so a check that quietly only handled `.rs` files
// would pass all of them.
//
// Skipped on Windows for the same reason `trycmd` skips `php_basic.toml` there.
#[cfg(not(windows))]
#[test]
fn dump_filtered_filters_a_php_fixture() {
    let (_tempdir, root) = dump_filtered_setup(Project::Php);

    // `$n += 2;` occupies exactly the columns that `$n += 1;` did on line 10.
    let path_buf = root.join("tests/BasicTest.php");
    let contents = read_to_string_wc(&path_buf).unwrap();
    let contents = contents.replacen(
        "        $n += 1;\n        noop();",
        "        $n += 2;\n        noop();",
        1,
    );
    write_wc(&path_buf, &contents).unwrap();

    let assert = cargo_bin_cmd!("necessist")
        .args([
            "--root",
            &root.to_string_lossy(),
            "--dump",
            "--filtered",
            "--verbose",
        ])
        .assert()
        .success();

    let stdout = stdout_of(assert.get_output());
    // Entries of a non-Rust backend are shown ...
    assert!(stdout.contains("BasicTest.php:9:9-9:16"), "{stdout:?}");
    assert!(stdout.contains("BasicTest.php:16:9-16:16"), "{stdout:?}");
    assert!(stdout.contains("BasicTest.php:17:9-17:17"), "{stdout:?}");
    // ... and filtered by the same rules.
    assert!(!stdout.contains("BasicTest.php:10:9-10:17"), "{stdout:?}");
    assert!(
        stdout.contains("1 entry was hidden by filtering"),
        "{stdout:?}"
    );
}

// Nothing is re-executed, and that is checked by execution, not only by output: the project's
// test writes a marker file every time it runs. Deleting the marker after the populating run and
// requiring `--dump --filtered` not to recreate it rules out an implementation that reached `run`
// and merely suppressed its output.
#[test]
fn dump_filtered_runs_no_tests() {
    let (_tempdir, root) = dump_filtered_setup(Project::Marker);

    // The populating run executed the test, which proves the marker works. The marker sits beside
    // the project, not inside it, so neither running the project from a copy nor deleting the
    // project afterwards could hide it.
    let marker = PathBuf::from(marker_path(&root).replace("\\\\", "\\"));
    assert!(
        marker.try_exists_wc().unwrap(),
        "the marker was never created"
    );
    remove_file_wc(&marker).unwrap();

    let assert = cargo_bin_cmd!("necessist")
        .args([
            "--root",
            &root.to_string_lossy(),
            "--dump",
            "--filtered",
            "--verbose",
        ])
        .assert()
        .success();

    assert!(
        !marker.try_exists_wc().unwrap(),
        "`--dump --filtered` executed the project's test"
    );

    let stdout = stdout_of(assert.get_output());
    // `run` announces every source file before it does anything, and `prepare` prints a summary
    // before returning; none of that may appear either.
    assert!(!stdout.contains("dry running"), "{stdout:?}");
    assert!(!stdout.contains("mutilating"), "{stdout:?}");
    assert!(!stdout.contains("candidates in"), "{stdout:?}");
    // The entries are still all there, so nothing was skipped by accident.
    assert!(stdout.contains("`n += 1;` failed"), "{stdout:?}");
}

// Filtering must not change which outcomes count as "more output". Here `timed_out` and `failed`
// are filtered out but `nonbuildable` survives, so `--verbose` would add a line and the note has
// to appear -- even though no `failed` entry is left to trigger it.
#[test]
fn dump_filtered_suggests_verbose_for_any_surviving_non_passed_outcome() {
    let (_tempdir, root) = dump_filtered_setup(Project::Basic);

    write_wc(
        root.join("necessist.toml"),
        "ignored_tests = [\"timed_out\", \"failed\"]\n",
    )
    .unwrap();

    let assert = cargo_bin_cmd!("necessist")
        .args(["--root", &root.to_string_lossy(), "--dump", "--filtered"])
        .assert()
        .success();

    let stdout = stdout_of(assert.get_output());
    assert!(stdout.contains("`n += 1;` passed"), "{stdout:?}");
    // The surviving extra entry is `nonbuildable`, not `failed`.
    assert!(
        stdout.contains("more output would be produced with --verbose"),
        "{stdout:?}"
    );
    assert!(!stdout.contains("hidden by filtering"), "{stdout:?}");
}

// The texts have to match byte for byte. Rewriting a multi-line candidate's file with CRLF line
// endings leaves its span alone and changes only the bytes between the lines, so a comparison
// that normalized line endings would still show it.
#[test]
fn dump_filtered_excludes_a_candidate_whose_line_endings_changed() {
    const SPANS: [&str; 4] = ["4:5-6:7", "4:6-6:6", "7:5-9:7", "7:6-9:6"];

    let (_tempdir, root) = dump_filtered_setup(Project::Whitespace);

    let path_buf = root.join("src/lib.rs");
    let contents = read_to_string_wc(&path_buf).unwrap();
    assert!(
        !contents.contains('\r'),
        "the project must start out LF-only"
    );
    write_wc(&path_buf, contents.replace('\n', "\r\n")).unwrap();

    // The spans are unchanged, so every entry is still a candidate ...
    let assert = cargo_bin_cmd!("necessist")
        .args([
            "--root",
            &root.to_string_lossy(),
            "--dump-candidates",
            "--no-sqlite",
        ])
        .assert()
        .success();
    let stdout = stdout_of(assert.get_output());
    for span in SPANS {
        assert!(stdout.contains(span), "{span}: {stdout:?}");
    }

    // ... and every one must nonetheless be hidden, because their bytes differ.
    let assert = cargo_bin_cmd!("necessist")
        .args([
            "--root",
            &root.to_string_lossy(),
            "--dump",
            "--filtered",
            "--verbose",
        ])
        .assert()
        .success();

    let stdout = stdout_of(assert.get_output());
    for span in SPANS {
        assert!(!stdout.contains(span), "{span}: {stdout:?}");
    }
    assert!(
        stdout.contains("4 entries were hidden by filtering"),
        "{stdout:?}"
    );
}

// `--filtered` appears in none of the `incompatible!` groups: as a modifier it is meant to inherit
// every restriction `--dump` carries. That is a claim about behavior, and nothing in the compiler
// checks it, so each flag `process_options` declares incompatible with `--dump` is passed here
// alongside `--dump --filtered`. `process_options` runs before anything touches the project, so an
// empty temporary directory serves as `--root`.
#[test]
fn dump_filtered_inherits_dumps_incompatibilities() {
    const CASES: &[(&str, &str)] = &[
        (
            "--dump-candidates",
            "--dump and --dump-candidates are incompatible",
        ),
        (
            "--dump-candidate-counts",
            "--dump and --dump-candidate-counts are incompatible",
        ),
        (
            "--default-config",
            "--default-config and --dump are incompatible",
        ),
        ("--find-skill", "--dump and --find-skill are incompatible"),
        ("--quiet", "--dump and --quiet are incompatible"),
        ("--reset", "--dump and --reset are incompatible"),
        ("--resume", "--dump and --resume are incompatible"),
        ("--no-sqlite", "--dump and --no-sqlite are incompatible"),
    ];

    let tempdir = tempdir().unwrap();
    let root = tempdir.path().to_string_lossy().to_string();

    for (flag, message) in CASES {
        let assert = cargo_bin_cmd!("necessist")
            .args(["--root", &root, "--dump", "--filtered", flag])
            .assert()
            .code(2);

        let stderr = std::str::from_utf8(&assert.get_output().stderr).unwrap();
        assert!(stderr.contains(message), "{flag}: {stderr:?}");
        assert!(assert.get_output().stdout.is_empty(), "{flag}");
    }

    // `--check-skill` takes a value and cannot be passed with `--root`.
    let assert = cargo_bin_cmd!("necessist")
        .args(["--dump", "--filtered", "--check-skill", "/nonexistent"])
        .assert()
        .code(2);
    let stderr = std::str::from_utf8(&assert.get_output().stderr).unwrap();
    assert!(
        stderr.contains("--check-skill and --dump are incompatible"),
        "{stderr:?}"
    );

    // `--filtered` on its own is rejected too, but for the opposite reason: it modifies `--dump`
    // and has nothing to act on without it.
    let assert = cargo_bin_cmd!("necessist")
        .args(["--root", &root, "--filtered"])
        .assert()
        .code(2);
    let stderr = std::str::from_utf8(&assert.get_output().stderr).unwrap();
    assert!(
        stderr.contains("--filtered can be used only with --dump"),
        "{stderr:?}"
    );

    // `--verbose` is *not* incompatible: the run gets past `process_options` and fails later, for
    // a different reason.
    cargo_bin_cmd!("necessist")
        .args(["--root", &root, "--dump", "--filtered", "--verbose"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("incompatible").not());
}

// `--dump --filtered` prints through `emit_to_console`, so `--no-lines-or-columns` has to work.
#[test]
fn dump_filtered_honors_no_lines_or_columns() {
    let (_tempdir, root) = dump_filtered_setup(Project::Basic);

    let assert = cargo_bin_cmd!("necessist")
        .args([
            "--root",
            &root.to_string_lossy(),
            "--dump",
            "--filtered",
            "--verbose",
            "--no-lines-or-columns",
        ])
        .assert()
        .success();

    let stdout = stdout_of(assert.get_output());
    // All four recorded outcomes are shown under `--verbose`, and none of them may carry a
    // line or column. Checking the `passed` entry alone would pass a version that suppressed
    // coordinates for `passed` only.
    for expected in [
        "src/lib.rs: `n += 1;` passed",
        "src/lib.rs: `n += 1;` timed-out",
        "src/lib.rs: `n += 1;` failed",
        "src/lib.rs: `.join(\"\")` nonbuildable",
    ] {
        assert!(stdout.contains(expected), "{expected:?}: {stdout:?}");
    }
    for line in stdout.lines().filter(|line| !line.starts_with("Note: ")) {
        assert!(
            !line.contains(".rs:") || line.contains(".rs: `"),
            "{line:?}"
        );
    }
}
