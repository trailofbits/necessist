//! Fixture copies for the cases in `tests/necessist_db_filtered`.
//!
//! `trycmd` cannot choose a case's current directory at run time, so each case that needs a
//! modified fixture runs in a copy, in a directory named after the case, under a
//! directory called `necessist_db_filtered` in the workspace's target directory; its `toml` file
//! names that path in `fs.cwd`. [`prepare`] creates every such copy before `trycmd` loads the
//! cases.
//!
//! A copy is made from a "master": a fixture, optionally edited, in which Necessist has already
//! run so that `necessist.db` exists. The edit that the case is about is applied to the copy
//! after that run, so the database records the unedited project. Masters live in a
//! hidden subdirectory of that directory, never in `fixtures/`, so an interrupted run cannot
//! leave a `necessist.db` behind in a fixture that another test expects to be free of one.

use assert_cmd::cargo::cargo_bin_cmd;
use elaborate::std::{
    fs::{
        DirEntryContext, copy_wc, create_dir_all_wc, read_dir_wc, read_to_string_wc,
        remove_dir_all_wc, remove_file_wc, write_wc,
    },
    path::PathContext,
};
use std::{
    path::{Path, PathBuf},
    thread,
};

/// Where the copies and masters live, relative to the `necessist` package directory (the current
/// directory of `cargo test`). The `toml` files reach the same place through `fs.cwd`.
const COPIES: &str = "../target/necessist_db_filtered";

const TIMEOUT: &str = "5";

/// A project in which Necessist has already run
#[derive(Clone, Copy, Eq, PartialEq)]
enum Master {
    /// `fixtures/basic`, whose database `trycmd` has just produced
    Basic,
    /// `fixtures/basic` with every test ignored, so that the run records no entries
    BasicNoCandidates,
    /// `fixtures/basic` with a second method call, `push_str`, added before the run
    BasicAppended,
    /// `fixtures/dry_run_failure` with a method call added to `tests/a.rs` before the run
    DryRunFailure,
    Whitespace,
    Case,
    IgnoredFunction,
    /// The test deletes `sentinel`; the master recreates it after the run
    Marker,
    Php,
}

const ALL_MASTERS: [Master; 9] = [
    Master::Basic,
    Master::BasicNoCandidates,
    Master::BasicAppended,
    Master::DryRunFailure,
    Master::Whitespace,
    Master::Case,
    Master::IgnoredFunction,
    Master::Marker,
    Master::Php,
];

impl Master {
    fn fixture(self) -> &'static str {
        match self {
            Self::Basic | Self::BasicNoCandidates | Self::BasicAppended => "basic",
            Self::DryRunFailure => "dry_run_failure",
            Self::Whitespace => "whitespace",
            Self::Case => "case",
            Self::IgnoredFunction => "ignored_function",
            Self::Marker => "marker",
            Self::Php => "php_basic",
        }
    }

    fn dir_name(self) -> &'static str {
        match self {
            Self::Basic => "basic",
            Self::BasicNoCandidates => "basic_no_candidates",
            Self::BasicAppended => "basic_appended",
            Self::DryRunFailure => "dry_run_failure",
            Self::Whitespace => "whitespace",
            Self::Case => "case",
            Self::IgnoredFunction => "ignored_function",
            Self::Marker => "marker",
            Self::Php => "php_basic",
        }
    }

    fn timeout(self) -> &'static str {
        match self {
            // PHPUnit through composer is slower to start than the Rust fixtures.
            Self::Php => "60",
            _ => TIMEOUT,
        }
    }

    /// Skipped where `trycmd.rs` skips the PHP cases
    fn is_available(self) -> bool {
        !(cfg!(windows) && self == Self::Php)
    }

    /// Builds the master at `root`, which does not exist yet
    fn build(self, root: &Path) {
        if self == Self::Basic {
            // `trycmd` has already run Necessist in `fixtures/basic`.
            copy_fixture(&fixture_path(self.fixture()), root, true);
            return;
        }

        copy_fixture(&fixture_path(self.fixture()), root, false);

        match self {
            Self::BasicNoCandidates => write_config(
                root,
                "ignored_tests = [\"passed\", \"timed_out\", \"failed\", \"nonbuildable\"]\n",
            ),
            Self::BasicAppended => append(
                root,
                "src/lib.rs",
                "
#[test]
fn appended() {
    let mut s = String::new();
    s.push_str(\"x\");
    assert!(!s.is_empty());
}
",
            ),
            Self::DryRunFailure => replace(
                root,
                "tests/a.rs",
                "    n += 1;\n    assert!",
                "    n += 1;\n    let _ = [\"x\"].join(\"\");\n    assert!",
            ),
            _ => {}
        }

        let stdout = cargo_bin_cmd!("necessist")
            .args([
                "--root",
                &root.to_string_lossy(),
                "--timeout",
                self.timeout(),
            ])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let stdout = String::from_utf8(stdout).unwrap();

        // A failed dry run is only a warning, but it records every entry as `Skipped`. The
        // fixtures that are meant to fail their dry run say so.
        if self != Self::DryRunFailure && self != Self::BasicNoCandidates {
            assert!(
                stdout.contains("mutilating"),
                "the run that populates `necessist.db` in `{}` did not get past its dry run: \
                 {stdout:?}",
                root.display()
            );
        }

        if self == Self::Marker {
            assert!(
                !root.join("sentinel").try_exists_wc().unwrap(),
                "the test in `{}` did not delete `sentinel`",
                root.display()
            );
            write_wc(root.join("sentinel"), "alive\n").unwrap();
        }
    }
}

/// A `trycmd` case that runs in its own copy of a master
struct Case {
    /// The stem of the case's `toml` file and the last component of its `fs.cwd`
    name: &'static str,
    master: Master,
    /// Applied to the copy after Necessist has populated the master's database
    edit: fn(&Path),
}

const CASES: &[Case] = &[
    Case {
        name: "basic_following_source_change",
        master: Master::Basic,
        // The absence of a trailing newline is what makes the stored offsets exceed the file's
        // length.
        edit: |root| {
            write_wc(
                root.join("src/lib.rs"),
                "#[test]\nfn passed() {\n    let mut n = 0;\n    n += 1;\n    noop();\n}\n\nfn \
                 noop() {}",
            )
            .unwrap();
        },
    },
    Case {
        name: "basic_no_verbose_hint_when_nothing_added",
        master: Master::Basic,
        edit: |root| {
            write_config(
                root,
                "ignored_tests = [\"timed_out\", \"failed\", \"nonbuildable\"]\n",
            );
        },
    },
    Case {
        name: "basic_ignored_tests",
        master: Master::Basic,
        edit: |root| write_config(root, "ignored_tests = [\"timed_out\", \"failed\"]\n"),
    },
    Case {
        name: "basic_rewritten_span",
        master: Master::Basic,
        // `n += 2;` occupies exactly the columns that `n += 1;` did.
        edit: |root| {
            replace(
                root,
                "src/lib.rs",
                "    n += 1;\n    noop();",
                "    n += 2;\n    noop();",
            );
        },
    },
    Case {
        name: "dry_run_failure_skipped",
        master: Master::DryRunFailure,
        edit: |_| {},
    },
    Case {
        name: "dry_run_failure_file",
        master: Master::DryRunFailure,
        edit: |_| {},
    },
    Case {
        name: "dry_run_failure_dir",
        master: Master::DryRunFailure,
        edit: |_| {},
    },
    Case {
        name: "dry_run_failure_no_path",
        master: Master::DryRunFailure,
        edit: |_| {},
    },
    Case {
        name: "dry_run_failure_src_dir",
        master: Master::DryRunFailure,
        edit: |_| {},
    },
    Case {
        name: "basic_ignored_methods",
        master: Master::Basic,
        edit: |root| write_config(root, "ignored_methods = [\"join\"]\n"),
    },
    Case {
        name: "basic_rewritten_method_call",
        master: Master::Basic,
        // `.concat()` occupies exactly the columns that `.join("")` did.
        edit: |root| replace(root, "src/lib.rs", ".join(\"\")", ".concat()"),
    },
    Case {
        name: "basic_hidden_passed_no_verbose",
        master: Master::Basic,
        edit: |root| write_config(root, "ignored_tests = [\"passed\"]\n"),
    },
    Case {
        name: "basic_no_entries",
        master: Master::BasicNoCandidates,
        edit: |_| {},
    },
    Case {
        name: "basic_parse_failure",
        master: Master::Basic,
        edit: |root| append(root, "src/lib.rs", "\nfn (((\n"),
    },
    Case {
        name: "basic_deleted_source_file",
        master: Master::Basic,
        edit: |root| remove_file_wc(root.join("src/lib.rs")).unwrap(),
    },
    Case {
        name: "basic_deleted_source_file_plain_dump",
        master: Master::Basic,
        edit: |root| remove_file_wc(root.join("src/lib.rs")).unwrap(),
    },
    Case {
        name: "basic_ignored_methods_other_shape",
        master: Master::BasicAppended,
        edit: |root| write_config(root, "ignored_methods = [\"push_str\"]\n"),
    },
    Case {
        name: "whitespace_indentation",
        master: Master::Whitespace,
        // Expanding the tab leaves every span alone and changes only the bytes.
        edit: |root| replace(root, "src/lib.rs", "\t", "    "),
    },
    Case {
        name: "dry_run_failure_hidden_skipped_no_verbose",
        master: Master::DryRunFailure,
        edit: |_| {},
    },
    Case {
        name: "case_changed",
        master: Master::Case,
        // `"AB"` occupies exactly the columns `"ab"` did, and still compiles.
        edit: |root| replace(root, "src/lib.rs", "\"ab\"", "\"AB\""),
    },
    Case {
        name: "whitespace_unchanged",
        master: Master::Whitespace,
        edit: |_| {},
    },
    Case {
        name: "dry_run_failure_a_rewritten_skipped",
        master: Master::DryRunFailure,
        edit: |root| replace(root, "tests/a.rs", "n += 1;", "n += 2;"),
    },
    Case {
        name: "dry_run_failure_a_rewritten_skipped_method_call",
        master: Master::DryRunFailure,
        edit: |root| replace(root, "tests/a.rs", ".join(\"\")", ".concat()"),
    },
    Case {
        name: "ignored_function_call",
        master: Master::IgnoredFunction,
        edit: |root| write_config(root, "ignored_functions = [\"ignored_function\"]\n"),
    },
    Case {
        name: "php_basic_rewritten",
        master: Master::Php,
        // `$n += 2;` occupies exactly the columns that `$n += 1;` did.
        edit: |root| {
            replace(
                root,
                "tests/BasicTest.php",
                "        $n += 1;\n        noop();",
                "        $n += 2;\n        noop();",
            );
        },
    },
    Case {
        name: "marker_runs_no_tests",
        master: Master::Marker,
        edit: |_| {},
    },
    Case {
        name: "basic_verbose_hint_surviving",
        master: Master::Basic,
        edit: |root| write_config(root, "ignored_tests = [\"timed_out\", \"failed\"]\n"),
    },
    Case {
        name: "whitespace_crlf_candidates",
        master: Master::Whitespace,
        edit: to_crlf,
    },
    Case {
        name: "whitespace_crlf_filtered",
        master: Master::Whitespace,
        edit: to_crlf,
    },
];

/// The names of the cases that run in a copy, in the order of [`CASES`]
pub fn case_names() -> impl Iterator<Item = &'static str> {
    CASES.iter().map(|case| case.name)
}

/// Removes `target/necessist_db_filtered` when dropped, unless the thread is panicking, so that
/// the copies of a failed run stay around for inspection. The next [`prepare`] clears them.
pub struct Copies(PathBuf);

impl Drop for Copies {
    fn drop(&mut self) {
        if !thread::panicking() {
            remove_dir_all_wc(&self.0).unwrap_or_default();
        }
    }
}

/// Creates a copy of the master for every case in [`CASES`]
///
/// `fixtures/basic` must already hold a `necessist.db` produced by a run of Necessist.
pub fn prepare() -> Copies {
    let copies = PathBuf::from(COPIES);
    if copies.try_exists_wc().unwrap() {
        remove_dir_all_wc(&copies).unwrap();
    }
    let masters = copies.join(".masters");
    create_dir_all_wc(&masters).unwrap();

    // The masters are independent, and the slowest ones spend most of their time waiting for a
    // test to time out.
    thread::scope(|scope| {
        for master in ALL_MASTERS
            .into_iter()
            .filter(|master| master.is_available())
        {
            let root = masters.join(master.dir_name());
            let _handle = scope.spawn(move || master.build(&root));
        }
    });

    for case in CASES.iter().filter(|case| case.master.is_available()) {
        let root = copies.join(case.name);
        copy_fixture(&masters.join(case.master.dir_name()), &root, true);
        (case.edit)(&root);
    }

    Copies(copies)
}

fn fixture_path(name: &str) -> PathBuf {
    Path::new("../fixtures").join(name)
}

/// Copies the directory `from` to `to`
///
/// `target` and `NECESSIST_LOCK` are left out at every level, as is `necessist.db` unless
/// `with_db` is set. The fixtures contain no symbolic links.
fn copy_fixture(from: &Path, to: &Path, with_db: bool) {
    create_dir_all_wc(to).unwrap();
    for entry in read_dir_wc(from).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if name == "target" || name == "NECESSIST_LOCK" || (!with_db && name == "necessist.db") {
            continue;
        }
        let dest = to.join(&name);
        if entry.file_type_wc().unwrap().is_dir() {
            copy_fixture(&entry.path(), &dest, with_db);
        } else {
            let _: u64 = copy_wc(entry.path(), &dest).unwrap();
        }
    }
}

fn write_config(root: &Path, contents: &str) {
    write_wc(root.join("necessist.toml"), contents).unwrap();
}

fn append(root: &Path, relative_path: &str, suffix: &str) {
    let path_buf = root.join(relative_path);
    let contents = read_to_string_wc(&path_buf).unwrap();
    write_wc(&path_buf, contents + suffix).unwrap();
}

/// Replaces the first occurrence of `from` in the file, which must contain it
fn replace(root: &Path, relative_path: &str, from: &str, to: &str) {
    let path_buf = root.join(relative_path);
    let contents = read_to_string_wc(&path_buf).unwrap();
    assert!(
        contents.contains(from),
        "`{}` does not contain {from:?}",
        path_buf.display()
    );
    write_wc(&path_buf, contents.replacen(from, to, 1)).unwrap();
}

/// Rewrites `src/lib.rs` with CRLF line endings; the file must start out LF-only
fn to_crlf(root: &Path) {
    let path_buf = root.join("src/lib.rs");
    let contents = read_to_string_wc(&path_buf).unwrap();
    assert!(
        !contents.contains('\r'),
        "the fixture must start out LF-only"
    );
    write_wc(&path_buf, contents.replace('\n', "\r\n")).unwrap();
}
