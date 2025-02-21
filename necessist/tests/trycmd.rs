use assert_cmd::prelude::*;
use necessist_core::util;
use regex::Regex;
use std::{
    env::remove_var,
    ffi::OsStr,
    fs::{read_dir, read_to_string},
    path::PathBuf,
    process::Command,
};
use trycmd::TestCases;

const ROOT: &str = "../fixtures/basic";
const TIMEOUT: &str = "5";

#[ctor::ctor]
fn initialize() {
    unsafe {
        remove_var("CARGO_TERM_COLOR");
    }
}

#[test]
fn trycmd() {
    TestCases::new()
        .env("TRYCMD", "1")
        .case("tests/necessist_db_absent/*.toml");

    Command::cargo_bin("necessist")
        .unwrap()
        .args(["--root", ROOT, "--timeout", TIMEOUT])
        .assert()
        .success();

    let _remove_file = util::RemoveFile(PathBuf::from(ROOT).join("necessist.db"));

    TestCases::new()
        .env("TRYCMD", "1")
        .case("tests/necessist_db_present/*.toml");
}

#[test]
fn check_stdout_files() {
    let re = Regex::new(r"\b[0-9]+\.[0-9]+s\b").unwrap();

    let necessist_db_absent = read_dir("tests/necessist_db_absent").unwrap();
    let necessist_db_present = read_dir("tests/necessist_db_present").unwrap();
    for entry in necessist_db_absent.chain(necessist_db_present) {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.extension() != Some(OsStr::new("stdout")) {
            continue;
        }

        let contents = read_to_string(&path).unwrap();

        assert!(!re.is_match(&contents), "{path:?} matches");
    }
}

#[test]
fn check_stderr_annotations() {
    let necessist_db_absent = read_dir("tests/necessist_db_absent").unwrap();
    let necessist_db_present = read_dir("tests/necessist_db_present").unwrap();
    for entry in necessist_db_absent.chain(necessist_db_present) {
        let entry = entry.unwrap();
        let path = entry.path();

        if !["stdout", "stderr"]
            .into_iter()
            .any(|s| path.extension() == Some(OsStr::new(s)))
        {
            continue;
        }

        let contents = read_to_string(&path).unwrap();

        let lines = contents.lines().collect::<Vec<_>>();
        assert!(
            lines
                .windows(2)
                .all(|w| w[0] != "stderr=```" || w[1] == "..."),
            "failed for {path:?}"
        );
    }
}

#[test]
fn check_toml_files() {
    let necessist_db_absent = read_dir("tests/necessist_db_absent").unwrap();
    let necessist_db_present = read_dir("tests/necessist_db_present").unwrap();
    for entry in necessist_db_absent.chain(necessist_db_present) {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.extension() != Some(OsStr::new("toml")) {
            continue;
        }

        let contents = read_to_string(&path).unwrap();
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

        if path.parent().unwrap().file_name() == Some(OsStr::new("no_necessist_db")) {
            assert_eq!(Some(&"--no-sqlite"), args.first());
        }

        let file_stem = &*path.file_stem().unwrap().to_string_lossy();
        let example = args
            .iter()
            .find_map(|arg| arg.strip_prefix("--root=fixtures/"))
            .unwrap();
        assert!(file_stem.starts_with(example));

        let stderr = document.as_table().and_then(|table| table.get("stderr"));
        assert!(stderr.is_some() || path.with_extension("stderr").try_exists().unwrap());

        let bin_name = document
            .as_table()
            .and_then(|table| table.get("bin"))
            .and_then(toml::Value::as_table)
            .and_then(|table| table.get("name"))
            .and_then(toml::Value::as_str)
            .unwrap();
        assert_eq!("necessist", bin_name);

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
