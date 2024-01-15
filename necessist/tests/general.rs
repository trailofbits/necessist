use assert_cmd::prelude::*;
use fs_extra::dir::{copy, CopyOptions};
use necessist_core::util;
use predicates::prelude::*;
use std::{path::PathBuf, process::Command, sync::Mutex};

mod tempfile_util;
use tempfile_util::tempdir;

const TIMEOUT: &str = "5";

// smoelius: Two tests use the `basic` fixture, but only one can run at a time.
static BASIC_MUTEX: Mutex<()> = Mutex::new(());

#[test]
fn necessist_db_can_be_moved() {
    const ROOT: &str = "../examples/basic";

    let _lock = BASIC_MUTEX.lock().unwrap();

    Command::cargo_bin("necessist")
        .unwrap()
        .args(["--root", ROOT, "--timeout", TIMEOUT])
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
        .args(["--root", &tempdir.path().to_string_lossy(), "--resume"])
        .assert()
        .success()
        .stdout(predicate::eq("4 candidates in 1 test file\n"));
}

#[test]
fn resume_following_dry_run_failure() {
    const ROOT: &str = "examples/dry_run_failure";

    let assert = Command::cargo_bin("necessist")
        .unwrap()
        .current_dir("..")
        .args(["--root", ROOT])
        .assert()
        .success();
    let stdout_normalized = std::str::from_utf8(&assert.get_output().stdout)
        .unwrap()
        .replace('\\', "/");
    assert!(
        stdout_normalized.starts_with(
            "\
2 candidates in 2 test files
examples/dry_run_failure/tests/a.rs: dry running
examples/dry_run_failure/tests/a.rs: Warning: dry run failed: code=101
"
        ),
        "{stdout_normalized:?}",
    );

    let necessist_db = PathBuf::from("..").join(ROOT).join("necessist.db");

    let _remove_file = util::RemoveFile(necessist_db);

    Command::cargo_bin("necessist")
        .unwrap()
        .current_dir("..")
        .args(["--root", ROOT, "--resume"])
        .assert()
        .success()
        .stdout(predicate::eq("2 candidates in 2 test files\n"));
}

// smoelius: Apparently, sending a ctrl-c on Windows is non-trivial:
// https://stackoverflow.com/questions/813086/can-i-send-a-ctrl-c-sigint-to-an-application-on-windows
// smoelius: Sending a ctrl-c allows the process to clean up after itself, e.g., to undo file
// rewrites.
#[cfg(not(windows))]
#[test]
fn resume_following_ctrl_c() {
    use similar_asserts::SimpleDiff;
    use std::io::{BufRead, BufReader, Read};
    use subprocess::Redirection;

    const ROOT: &str = "examples/basic";

    fn command() -> Command {
        let mut command = Command::cargo_bin("necessist").unwrap();
        command
            .current_dir("..")
            .args(["--root", ROOT, "--timeout", TIMEOUT, "--verbose"]);
        command
    }

    let _lock = BASIC_MUTEX.lock().unwrap();

    let exec = util::exec_from_command(&command())
        .stdout(Redirection::Pipe)
        .stderr(Redirection::Pipe);
    let mut popen = exec.popen().unwrap();

    let stdout = popen.stdout.as_ref().unwrap();
    let reader = BufReader::new(stdout);
    let _: String = reader
        .lines()
        .map(Result::unwrap)
        .find(|line| line == "examples/basic/src/lib.rs:4:5-4:12: `n += 1;` passed")
        .unwrap();

    let pid = popen.pid().unwrap();
    kill().arg(pid.to_string()).assert().success();

    let mut stderr = popen.stderr.as_ref().unwrap();
    let mut buf = Vec::new();
    let _ = stderr.read_to_end(&mut buf).unwrap();
    let stderr = String::from_utf8(buf).unwrap();
    assert!(stderr.ends_with("Ctrl-C detected\n"), "{stderr:?}");

    let _ = popen.wait().unwrap();

    let necessist_db = PathBuf::from("..").join(ROOT).join("necessist.db");

    let _remove_file = util::RemoveFile(necessist_db);

    let assert = command().arg("--resume").assert().success();

    // smoelius: N.B. `stdout_expected` intentionally lacks the following line:
    //   examples/basic/src/lib.rs:4:5-4:12: `n += 1;` passed
    let stdout_expected: &str = "\
4 candidates in 1 test file
examples/basic/src/lib.rs: dry running
examples/basic/src/lib.rs: mutilating
examples/basic/src/lib.rs:14:9-14:16: `n += 1;` timed-out
examples/basic/src/lib.rs:21:5-21:12: `n += 1;` failed
examples/basic/src/lib.rs:28:18-28:27: `.join(\"\")` nonbuildable
";

    let stdout_actual = std::str::from_utf8(&assert.get_output().stdout).unwrap();

    assert_eq!(
        stdout_expected,
        stdout_actual,
        "{}",
        SimpleDiff::from_str(stdout_expected, stdout_actual, "left", "right")
    );
}

#[cfg(not(windows))]
fn kill() -> Command {
    let mut command = Command::new("kill");
    command.arg("-INT");
    command
}
