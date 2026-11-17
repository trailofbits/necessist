//! Fixture copies for the cases in `tests/necessist_db_filtered`.
//!
//! `trycmd` cannot choose a case's current directory at run time, so each case that needs a
//! modified fixture runs in a copy, in a directory named after the case, under a
//! directory called `necessist_db_filtered` in the workspace's target directory; its `toml` file
//! names that path in `fs.cwd`. [`prepare`] creates every such copy before `trycmd` loads the
//! cases.
//!
//! A copy is made from a fixture in which Necessist has already run, so that `necessist.db`
//! exists. The edit that the case is about is applied to the copy after that run, so the database
//! records the unedited project.

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

/// Where the copies live, relative to the `necessist` package directory (the current
/// directory of `cargo test`). The `toml` files reach the same place through `fs.cwd`.
const COPIES: &str = "../target/necessist_db_filtered";

/// A `trycmd` case that runs in its own copy of a master
struct Case {
    /// The stem of the case's `toml` file and the last component of its `fs.cwd`
    name: &'static str,
    /// The directory in `fixtures` that the copy is made from
    fixture: &'static str,
    /// Applied to the copy after Necessist has populated the master's database
    edit: fn(&Path),
}

const CASES: &[Case] = &[
    Case {
        name: "basic_following_source_change",
        fixture: "basic",
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
        fixture: "basic",
        edit: |root| {
            write_config(
                root,
                "ignored_tests = [\"timed_out\", \"failed\", \"nonbuildable\"]\n",
            );
        },
    },
    Case {
        name: "basic_ignored_tests",
        fixture: "basic",
        edit: |root| write_config(root, "ignored_tests = [\"timed_out\", \"failed\"]\n"),
    },
    Case {
        name: "basic_rewritten_span",
        fixture: "basic",
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
        fixture: "dry_run_failure_method_call",
        edit: |_| {},
    },
    Case {
        name: "dry_run_failure_file",
        fixture: "dry_run_failure_method_call",
        edit: |_| {},
    },
    Case {
        name: "dry_run_failure_dir",
        fixture: "dry_run_failure_method_call",
        edit: |_| {},
    },
    Case {
        name: "dry_run_failure_no_path",
        fixture: "dry_run_failure_method_call",
        edit: |_| {},
    },
    Case {
        name: "dry_run_failure_src_dir",
        fixture: "dry_run_failure_method_call",
        edit: |_| {},
    },
    Case {
        name: "basic_ignored_methods",
        fixture: "basic",
        edit: |root| write_config(root, "ignored_methods = [\"join\"]\n"),
    },
    Case {
        name: "basic_rewritten_method_call",
        fixture: "basic",
        // `.concat()` occupies exactly the columns that `.join("")` did.
        edit: |root| replace(root, "src/lib.rs", ".join(\"\")", ".concat()"),
    },
    Case {
        name: "basic_hidden_passed_no_verbose",
        fixture: "basic",
        edit: |root| write_config(root, "ignored_tests = [\"passed\"]\n"),
    },
    Case {
        name: "basic_no_entries",
        fixture: "basic_no_candidates",
        edit: |_| {},
    },
    Case {
        name: "basic_parse_failure",
        fixture: "basic",
        edit: |root| append(root, "src/lib.rs", "\nfn (((\n"),
    },
    Case {
        name: "basic_deleted_source_file",
        fixture: "basic",
        edit: |root| remove_file_wc(root.join("src/lib.rs")).unwrap(),
    },
    Case {
        name: "basic_deleted_source_file_plain_dump",
        fixture: "basic",
        edit: |root| remove_file_wc(root.join("src/lib.rs")).unwrap(),
    },
    Case {
        name: "basic_ignored_methods_other_shape",
        fixture: "basic_appended",
        edit: |root| write_config(root, "ignored_methods = [\"push_str\"]\n"),
    },
    Case {
        name: "whitespace_indentation",
        fixture: "whitespace",
        // Expanding the tab leaves every span alone and changes only the bytes.
        edit: |root| replace(root, "src/lib.rs", "\t", "    "),
    },
    Case {
        name: "dry_run_failure_hidden_skipped_no_verbose",
        fixture: "dry_run_failure_method_call",
        edit: |_| {},
    },
    Case {
        name: "case_changed",
        fixture: "case",
        // `"AB"` occupies exactly the columns `"ab"` did, and still compiles.
        edit: |root| replace(root, "src/lib.rs", "\"ab\"", "\"AB\""),
    },
    Case {
        name: "whitespace_unchanged",
        fixture: "whitespace",
        edit: |_| {},
    },
    Case {
        name: "dry_run_failure_a_rewritten_skipped",
        fixture: "dry_run_failure_method_call",
        edit: |root| replace(root, "tests/a.rs", "n += 1;", "n += 2;"),
    },
    Case {
        name: "dry_run_failure_a_rewritten_skipped_method_call",
        fixture: "dry_run_failure_method_call",
        edit: |root| replace(root, "tests/a.rs", ".join(\"\")", ".concat()"),
    },
    Case {
        name: "ignored_function_call",
        fixture: "ignored_function",
        edit: |root| write_config(root, "ignored_functions = [\"ignored_function\"]\n"),
    },
    Case {
        name: "php_basic_rewritten",
        fixture: "php_basic",
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
        fixture: "marker",
        edit: |_| {},
    },
    Case {
        name: "basic_verbose_hint_surviving",
        fixture: "basic",
        edit: |root| write_config(root, "ignored_tests = [\"timed_out\", \"failed\"]\n"),
    },
    Case {
        name: "whitespace_crlf_candidates",
        fixture: "whitespace",
        edit: to_crlf,
    },
    Case {
        name: "whitespace_crlf_filtered",
        fixture: "whitespace",
        edit: to_crlf,
    },
];

/// The names of the cases that run in a copy, in the order of [`CASES`]
pub fn case_names() -> impl Iterator<Item = &'static str> {
    CASES.iter().map(|case| case.name)
}

/// The fixtures that the cases are copied from, each once, in the order of [`CASES`]
pub fn fixtures() -> Vec<&'static str> {
    let mut fixtures = Vec::new();
    for case in CASES {
        if !fixtures.contains(&case.fixture) {
            fixtures.push(case.fixture);
        }
    }
    fixtures
}

/// Whether `fixture` can be used on the platform running the tests
///
/// The PHP cases are skipped on Windows, as `trycmd.rs` skips them.
pub fn is_available(fixture: &str) -> bool {
    !(cfg!(windows) && fixture == "php_basic")
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

/// Creates a copy of the fixture for every case in [`CASES`]
///
/// Every fixture in [`fixtures`] that [`is_available`] must already hold a `necessist.db`
/// produced by a run of Necessist.
pub fn prepare() -> Copies {
    let copies = PathBuf::from(COPIES);
    if copies.try_exists_wc().unwrap() {
        remove_dir_all_wc(&copies).unwrap();
    }
    create_dir_all_wc(&copies).unwrap();

    for case in CASES.iter().filter(|case| is_available(case.fixture)) {
        let root = copies.join(case.name);
        copy_fixture(&fixture_path(case.fixture), &root);
        (case.edit)(&root);
    }

    Copies(copies)
}

fn fixture_path(name: &str) -> PathBuf {
    Path::new("../fixtures").join(name)
}

/// Copies the directory `from` to `to`
///
/// `target` and `NECESSIST_LOCK` are left out at every level. The fixtures contain no symbolic
/// links.
fn copy_fixture(from: &Path, to: &Path) {
    create_dir_all_wc(to).unwrap();
    for entry in read_dir_wc(from).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if name == "target" || name == "NECESSIST_LOCK" {
            continue;
        }
        let dest = to.join(&name);
        if entry.file_type_wc().unwrap().is_dir() {
            copy_fixture(&entry.path(), &dest);
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
