use assert_cmd::cargo::cargo_bin_cmd;
use elaborate::std::{
    fs::{read_dir_wc, read_to_string_wc},
    path::PathContext,
};
use necessist_core::util;
use regex::Regex;
use std::{
    env::remove_var,
    ffi::OsStr,
    path::{Path, PathBuf},
};
use trycmd::TestCases;

mod filtered_cases;

const ROOT: &str = "../fixtures/basic";
const TIMEOUT: &str = "5";

/// The directories of `trycmd` cases, in the order in which they are checked
const CASE_DIRS: [&str; 3] = [
    "tests/necessist_db_absent",
    "tests/necessist_db_present",
    "tests/necessist_db_filtered",
];

/// What `fs.cwd` of a case that runs in its own copy of a fixture starts with. The copies are made
/// by [`filtered_cases::prepare`].
const COPY_CWD_PREFIX: &str = "../../../target/necessist_db_filtered/";

#[ctor::ctor(unsafe)]
fn initialize() {
    unsafe {
        remove_var("CARGO_TERM_COLOR");
    }
}

fn new_cases() -> TestCases {
    let cases = TestCases::new();
    cases.default_bin_name("necessist").env("TRYCMD", "1");
    cases
}

#[test]
fn trycmd() {
    let cases = new_cases();

    cases.case("tests/necessist_db_absent/*.toml");

    #[cfg(windows)]
    cases.skip("tests/necessist_db_absent/php_basic.toml");

    cargo_bin_cmd!("necessist")
        .args(["--root", ROOT, "--timeout", TIMEOUT])
        .assert()
        .success();

    let _remove_file = util::RemoveFile(PathBuf::from(ROOT).join("necessist.db"));

    // The copies are made from the database just produced and, like it, before any case can
    // modify `fixtures/basic`.
    let _copies = filtered_cases::prepare();

    new_cases().case("tests/necessist_db_present/*.toml");

    // The cases that run in a copy of a fixture come after those that use `necessist.db` in
    // `fixtures/basic`, and before the database is removed.
    let filtered = new_cases();

    filtered.case("tests/necessist_db_filtered/*.toml");

    #[cfg(windows)]
    filtered.skip("tests/necessist_db_filtered/php_basic_rewritten.toml");

    filtered.run();
}

#[test]
fn check_stdout_files() {
    let re = Regex::new(r"\b[0-9]+\.[0-9]+s\b").unwrap();

    for entry in CASE_DIRS.iter().flat_map(|dir| read_dir_wc(dir).unwrap()) {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.extension_wc().ok() != Some(OsStr::new("stdout")) {
            continue;
        }

        let contents = read_to_string_wc(&path).unwrap();

        assert!(!re.is_match(&contents), "`{}` matches", path.display());
    }
}

#[test]
fn check_stderr_annotations() {
    for entry in CASE_DIRS.iter().flat_map(|dir| read_dir_wc(dir).unwrap()) {
        let entry = entry.unwrap();
        let path = entry.path();

        if !["stdout", "stderr"]
            .into_iter()
            .any(|s| path.extension_wc().ok() == Some(OsStr::new(s)))
        {
            continue;
        }

        let contents = read_to_string_wc(&path).unwrap();

        let lines = contents.lines().collect::<Vec<_>>();
        assert!(
            lines
                .windows(2)
                .all(|w| w[0] != "stderr=```" || w[1] == "..."),
            "failed for `{}`",
            path.display()
        );
    }
}

#[test]
fn check_toml_files() {
    for entry in CASE_DIRS.iter().flat_map(|dir| read_dir_wc(dir).unwrap()) {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.extension_wc().ok() != Some(OsStr::new("toml")) {
            continue;
        }

        let contents = read_to_string_wc(&path).unwrap();
        let document = toml::from_str::<toml::Value>(&contents).unwrap();

        let args = document
            .as_table()
            .and_then(|table| table.get("args"))
            .and_then(toml::Value::as_array)
            .and_then(|array| {
                array
                    .iter()
                    .map(toml::Value::as_str)
                    .collect::<Option<Vec<_>>>()
            })
            .unwrap();

        if path.parent_wc().unwrap().file_name_wc().ok() == Some(OsStr::new("no_necessist_db")) {
            assert_eq!(Some(&"--no-sqlite"), args.first());
        }

        check_fs_shape(&path, &document, &args);

        let status = document.as_table().and_then(|table| table.get("status"));
        let stderr = document.as_table().and_then(|table| table.get("stderr"));
        assert!(status.is_some() || stderr.is_some());

        if let Some(status) = status {
            let code = status
                .as_table()
                .and_then(|table| table.get("code"))
                .and_then(toml::Value::as_integer)
                .unwrap();
            assert_eq!(2, code);
        }

        for stream in ["stdout", "stderr"] {
            let inline_empty = document
                .as_table()
                .and_then(|table| table.get(stream))
                .and_then(toml::Value::as_str)
                == Some("");
            let snapshot_exists = path.with_extension(stream).try_exists_wc().unwrap();
            assert!(
                !inline_empty || !snapshot_exists,
                r#"`{}` has both `{stream} = ""` and a `.{stream}` file"#,
                path.display()
            );
            let snapshot_nonempty = path
                .with_extension(stream)
                .metadata_wc()
                .is_ok_and(|metadata| metadata.len() > 0);
            assert!(
                inline_empty || snapshot_nonempty,
                r#"`{}` has neither `{stream} = ""` nor a non-empty `.{stream}` file"#,
                path.display()
            );
        }
    }
}

/// Checks that a case's `fs` table and `--root` argument agree with the directory it is in
///
/// There are three shapes, the second and third of which are allowed only in
/// `necessist_db_filtered`:
/// - the current directory is the workspace root, and the project is a fixture named by `--root`;
/// - the current directory is a copy of a fixture made for this case and named after it, and the
///   project is that directory, so there is no `--root`;
/// - there is no project, so neither `fs` nor `--root` (`--help`, `--check-skill`).
fn check_fs_shape(path: &Path, document: &toml::Value, args: &[&str]) {
    let file_stem = &*path.file_stem_wc().unwrap().to_string_lossy();
    let in_filtered_dir =
        path.parent_wc().unwrap().file_name_wc().ok() == Some(OsStr::new("necessist_db_filtered"));
    let has_root = args.iter().any(|arg| arg.starts_with("--root"));
    let fs_table = document.as_table().and_then(|table| table.get("fs"));
    let fs_cwd = fs_table
        .and_then(toml::Value::as_table)
        .and_then(|table| table.get("cwd"))
        .and_then(toml::Value::as_str);

    match fs_cwd {
        Some("../../..") => {
            let example = args
                .iter()
                .find_map(|arg| arg.strip_prefix("--root=fixtures/"))
                .unwrap();
            assert!(file_stem.starts_with(example));
        }
        Some(cwd) if in_filtered_dir => {
            assert_eq!(
                Some(file_stem),
                cwd.strip_prefix(COPY_CWD_PREFIX),
                "`{}` has an unexpected `fs.cwd`",
                path.display()
            );
            assert!(
                !has_root,
                "`{}` has a copy as `fs.cwd` and `--root`",
                path.display()
            );
        }
        None if in_filtered_dir => {
            assert!(
                fs_table.is_none(),
                "`{}` has `fs` but no `fs.cwd`",
                path.display()
            );
            assert!(
                !has_root
                    && args
                        .iter()
                        .any(|arg| ["--help", "--check-skill"].contains(arg)),
                "`{}` has neither `fs.cwd` nor a flag that needs no project",
                path.display()
            );
        }
        _ => panic!(
            "`{}` has an unexpected `fs.cwd`: {fs_cwd:?}",
            path.display()
        ),
    }

    // A `.out` directory makes `trycmd` run the case in a temporary copy of `fs.cwd`, which only
    // a copy made for the case can stand in for.
    if path.with_extension("out").try_exists_wc().unwrap() {
        assert!(
            fs_cwd.is_some_and(|cwd| cwd.starts_with(COPY_CWD_PREFIX)),
            "`{}` has a `.out` directory but no copy as `fs.cwd`",
            path.display()
        );
    }
}

#[test]
fn filtered_cases_match_tomls() {
    let mut in_tomls = read_dir_wc("tests/necessist_db_filtered")
        .unwrap()
        .filter_map(|entry| {
            let path = entry.unwrap().path();
            if path.extension_wc().ok() != Some(OsStr::new("toml")) {
                return None;
            }
            let contents = read_to_string_wc(&path).unwrap();
            let document = toml::from_str::<toml::Value>(&contents).unwrap();
            let cwd = document
                .as_table()
                .and_then(|table| table.get("fs"))
                .and_then(toml::Value::as_table)
                .and_then(|table| table.get("cwd"))
                .and_then(toml::Value::as_str)?;
            cwd.starts_with(COPY_CWD_PREFIX)
                .then(|| path.file_stem_wc().unwrap().to_string_lossy().into_owned())
        })
        .collect::<Vec<_>>();
    in_tomls.sort();

    let mut in_table = filtered_cases::case_names()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    in_table.sort();

    assert_eq!(in_table, in_tomls);
}
