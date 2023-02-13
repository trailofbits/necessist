#![cfg(unix)]

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

const ROOT: &str = "../examples/basic";
const TIMEOUT: &str = "5";

#[ctor::ctor]
fn initialize() {
    remove_var("CARGO_TERM_COLOR");
}

#[test]
fn trycmd() {
    // smoelius: Ensure `necessist` binary is up to date.
    Command::new("cargo")
        .args(["build", "--bin", "necessist"])
        .current_dir("..")
        .assert()
        .success();

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
fn check_stdout() {
    let re = Regex::new(r#"\b[0-9]+\.[0-9]+s\b"#).unwrap();

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
fn check_toml() {
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
        let example = file_stem.split_once('_').map_or(file_stem, |(s, _)| s);
        assert!(args.contains(&format!("--root=examples/{example}").as_str()));

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
