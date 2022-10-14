use assert_cmd::prelude::*;
use fs_extra::dir::{copy, CopyOptions};
use necessist_core::util;
use predicates::prelude::*;
use std::{path::PathBuf, process::Command};
use tempfile::tempdir;

const ROOT: &str = "../examples/basic";
const TIMEOUT: &str = "5";

#[test]
fn necessist_db_can_be_moved() {
    Command::cargo_bin("necessist")
        .unwrap()
        .args(&["--root", ROOT, "--timeout", TIMEOUT])
        .assert()
        .success();

    let necessist_db = PathBuf::from(ROOT).join("necessist.db");

    let _remove_file = util::RemoveFile(necessist_db);

    let tempdir = tempdir().unwrap();

    copy(
        ROOT,
        &tempdir,
        &CopyOptions {
            content_only: true,
            ..Default::default()
        },
    )
    .unwrap();

    Command::cargo_bin("necessist")
        .unwrap()
        .args(&["--root", &tempdir.path().to_string_lossy(), "--resume"])
        .assert()
        .success()
        .stdout(predicate::eq("4 candidates in 1 test file\n"));
}
