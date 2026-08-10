use assert_cmd::cargo::cargo_bin_cmd;
use elaborate::std::{
    fs::{read_dir_wc, read_to_string_wc},
    path::PathContext,
};
use necessist_core::util;
use regex::Regex;
use std::{env::remove_var, ffi::OsStr, path::PathBuf};
use trycmd::TestCases;

const ROOT: &str = "../fixtures/basic";
const TIMEOUT: &str = "5";

#[ctor::ctor(unsafe)]
fn initialize() {
    unsafe {
        remove_var("CARGO_TERM_COLOR");
    }
}

#[test]
fn trycmd() {
    let cases = TestCases::new();

    cases
        .default_bin_name("necessist")
        .env("TRYCMD", "1")
        .case("tests/necessist_db_absent/*.toml");

    #[cfg(windows)]
    cases.skip("tests/necessist_db_absent/php_basic.toml");

    cargo_bin_cmd!("necessist")
        .args(["--root", ROOT, "--timeout", TIMEOUT])
        .assert()
        .success();

    let _remove_file = util::RemoveFile(PathBuf::from(ROOT).join("necessist.db"));

    TestCases::new()
        .default_bin_name("necessist")
        .env("TRYCMD", "1")
        .case("tests/necessist_db_present/*.toml");
}

#[test]
fn check_stdout_files() {
    let re = Regex::new(r"\b[0-9]+\.[0-9]+s\b").unwrap();

    let necessist_db_absent = read_dir_wc("tests/necessist_db_absent").unwrap();
    let necessist_db_present = read_dir_wc("tests/necessist_db_present").unwrap();
    for entry in necessist_db_absent.chain(necessist_db_present) {
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
    let necessist_db_absent = read_dir_wc("tests/necessist_db_absent").unwrap();
    let necessist_db_present = read_dir_wc("tests/necessist_db_present").unwrap();
    for entry in necessist_db_absent.chain(necessist_db_present) {
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
    let necessist_db_absent = read_dir_wc("tests/necessist_db_absent").unwrap();
    let necessist_db_present = read_dir_wc("tests/necessist_db_present").unwrap();
    for entry in necessist_db_absent.chain(necessist_db_present) {
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

        let file_stem = &*path.file_stem_wc().unwrap().to_string_lossy();
        let example = args
            .iter()
            .find_map(|arg| arg.strip_prefix("--root=fixtures/"))
            .unwrap();
        assert!(file_stem.starts_with(example));

        let status = document.as_table().and_then(|table| table.get("status"));
        let stderr = document.as_table().and_then(|table| table.get("stderr"));
        assert!(status.is_some() || stderr.is_some());

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

        let fs_cwd = document
            .as_table()
            .and_then(|table| table.get("fs"))
            .and_then(toml::Value::as_table)
            .and_then(|table| table.get("cwd"))
            .and_then(toml::Value::as_str)
            .unwrap();
        assert_eq!("../../..", fs_cwd);
    }
}
