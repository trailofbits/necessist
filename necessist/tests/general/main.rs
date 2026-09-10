use assert_cmd::{assert::OutputAssertExt, cargo::cargo_bin_cmd};
use elaborate::std::env::set_current_dir_wc;
use necessist_core::util;
use predicates::prelude::*;
use std::{path::PathBuf, process::Command, sync::Mutex};
use testing::tempfile_util::tempdir;

const TIMEOUT: &str = "5";

const BASIC_ROOT: &str = "fixtures/basic";

mod skill;

#[ctor::ctor(unsafe)]
fn initialize() {
    set_current_dir_wc("..").unwrap();
}

#[test]
fn large_output_does_not_block() {
    cargo_bin_cmd!("necessist")
        .args([
            "--no-sqlite",
            "--root=fixtures/large_output",
            "--timeout=5",
            "--verbose",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("`n += 1;` failed"))
        .stdout(predicate::str::contains("`n += 2;` failed"))
        .stdout(predicate::str::contains("timed-out").not());
}

#[test]
fn necessist_db_can_be_moved() {
    run_basic_test(|| {
        cargo_bin_cmd!("necessist")
            .args(["--root", BASIC_ROOT, "--timeout", TIMEOUT])
            .assert()
            .success();

        let tempdir = tempdir().unwrap();

        Command::new("cp")
            .args(["-r", BASIC_ROOT, &tempdir.path().to_string_lossy()])
            .assert()
            .success();

        cargo_bin_cmd!("necessist")
            .args([
                "--root",
                &tempdir.path().join("basic").to_string_lossy(),
                "--resume",
            ])
            .assert()
            .success()
            .stdout(predicate::eq("4 candidates in 4 tests in 1 source file\n"));
    });
}

#[test]
fn resume_following_dry_run_failure() {
    const DRF_ROOT: &str = "fixtures/dry_run_failure";

    let necessist_db = PathBuf::from(DRF_ROOT).join("necessist.db");

    let _remove_file = util::RemoveFile(necessist_db);

    let assert = cargo_bin_cmd!("necessist")
        .args(["--root", DRF_ROOT])
        .assert()
        .success();
    let stdout_normalized = std::str::from_utf8(&assert.get_output().stdout)
        .unwrap()
        .replace('\\', "/");
    assert!(
        stdout_normalized.starts_with(
            "\
2 candidates in 2 tests in 3 source files
fixtures/dry_run_failure/tests/a.rs: dry running
fixtures/dry_run_failure/tests/a.rs: Warning: dry run failed: code=101
"
        ),
        "{stdout_normalized:?}",
    );

    cargo_bin_cmd!("necessist")
        .args(["--root", DRF_ROOT, "--resume"])
        .assert()
        .success()
        .stdout(predicate::eq("2 candidates in 2 tests in 3 source files\n"));
}

// smoelius: Apparently, sending a ctrl-c on Windows is non-trivial:
// https://stackoverflow.com/questions/813086/can-i-send-a-ctrl-c-sigint-to-an-application-on-windows
// smoelius: Sending a ctrl-c allows the process to clean up after itself, e.g., to undo file
// rewrites.
#[cfg(not(windows))]
#[test]
fn resume_following_ctrl_c() {
    use elaborate::std::io::ReadContext;
    use similar_asserts::SimpleDiff;
    use std::io::{BufRead, BufReader};

    fn command() -> Command {
        let mut command = Command::new("cargo");
        command.args([
            "run",
            "--bin=necessist",
            "--quiet",
            "--",
            "--root",
            BASIC_ROOT,
            "--timeout",
            TIMEOUT,
            "--verbose",
        ]);
        command
    }

    run_basic_test(|| {
        let exec = util::exec_from_command(&command())
            .stdout(subprocess::Redirection::Pipe)
            .stderr(subprocess::Redirection::Pipe);
        let job = exec.start().unwrap();

        let stdout = job.stdout.as_ref().unwrap();
        let reader = BufReader::new(stdout);
        let _: String = reader
            .lines()
            .map(Result::unwrap)
            .find(|line| line == "fixtures/basic/src/lib.rs:4:5-4:12: `n += 1;` passed")
            .unwrap();

        let pid = job.pid();
        kill().arg(pid.to_string()).assert().success();

        let mut stderr = job.stderr.as_ref().unwrap();
        let mut buf = Vec::new();
        let _: usize = stderr.read_to_end_wc(&mut buf).unwrap();
        let stderr = String::from_utf8(buf).unwrap();
        assert!(stderr.ends_with("Ctrl-C detected\n"), "{stderr:?}");

        let _: subprocess::ExitStatus = job.wait().unwrap();

        let assert = command().arg("--resume").assert().success();

        // smoelius: N.B. `stdout_expected` intentionally lacks the following line:
        //   fixtures/basic/src/lib.rs:4:5-4:12: `n += 1;` passed
        let stdout_expected: &str = "\
4 candidates in 4 tests in 1 source file
fixtures/basic/src/lib.rs: dry running
fixtures/basic/src/lib.rs: mutilating
fixtures/basic/src/lib.rs:14:9-14:16: `n += 1;` timed-out
fixtures/basic/src/lib.rs:21:5-21:12: `n += 1;` failed
fixtures/basic/src/lib.rs:28:18-28:27: `.join(\"\")` nonbuildable
";

        let stdout_actual = std::str::from_utf8(&assert.get_output().stdout).unwrap();

        assert_eq!(
            stdout_expected,
            stdout_actual,
            "{}",
            SimpleDiff::from_str(stdout_expected, stdout_actual, "left", "right")
        );
    });
}

#[cfg(not(windows))]
fn kill() -> Command {
    let mut command = Command::new("kill");
    command.arg("-INT");
    command
}

// `Span::source_text` must report a span it cannot read rather than index past the contents. Two
// calls of `necessist` arrange one: `SOURCE_FILES` is never invalidated, so the second call slices
// the first call's contents while the backend reparses the file as it now stands, and the span's
// coordinates and text then come from different reads. The two reads within a single parse leave
// the same window open, and are not covered here; opening it on demand would need a hook in
// `SourceFile`.
#[test]
fn source_text_reports_a_span_exceeding_the_cached_contents() {
    use elaborate::std::fs::{create_dir_all_wc, write_wc};
    use necessist_backends::Identifier;
    use necessist_core::{Necessist, framework::Auto, necessist};

    const MANIFEST: &str = r#"[package]
name = "repro"
version = "0.1.0"
edition = "2024"
publish = false

[workspace]
"#;
    const SHORT: &str = r"#[test]
fn t() {
    let mut n = 0;
    n += 1;
    assert!(n > 0);
}
";
    const LONG: &str = r"#[test]
fn t() {
    let mut n = 0;
    n += 1;
    assert!(n > 0);
}

#[test]
fn u() {
    let mut n = 0;
    n += 1;
    assert!(n > 0);
}";

    let tempdir = tempdir().unwrap();
    let root = tempdir.path().join("repro");
    create_dir_all_wc(root.join("src")).unwrap();
    write_wc(root.join("Cargo.toml"), MANIFEST).unwrap();

    let opts = Necessist {
        root: Some(root.clone()),
        dump_candidates: true,
        no_sqlite: true,
        ..Default::default()
    };

    // First call reads and caches the short file.
    write_wc(root.join("src/lib.rs"), SHORT).unwrap();
    necessist(&opts, Auto::<Identifier>::default()).unwrap();

    // The file grows. The cache still holds the short contents, but the backend parses the long
    // ones, so `dump_candidates` reaches for a span that is not within what it holds.
    //
    // Before this change: `panicked at core/src/span.rs:165:41: range start index .. out of range`.
    // After it: an ordinary error.
    write_wc(root.join("src/lib.rs"), LONG).unwrap();
    let error = necessist(&opts, Auto::<Identifier>::default()).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("is not a valid range within the contents of"),
        "{error}"
    );
}

#[test]
fn tests_are_not_rebuilt() {
    run_basic_test(|| {
        cargo_bin_cmd!("necessist")
            .args(["--root", BASIC_ROOT, "--timeout", TIMEOUT])
            .env("NECESSIST_CHECK_MTIMES", "1")
            .assert()
            .success();
    });
}

fn run_basic_test(f: impl FnOnce()) {
    // smoelius: Three tests use the `basic` fixture, but only one can run at a time.
    static BASIC_MUTEX: Mutex<()> = Mutex::new(());

    let _lock = BASIC_MUTEX.lock().unwrap();

    let necessist_db = PathBuf::from(BASIC_ROOT).join("necessist.db");

    let _remove_file = util::RemoveFile(necessist_db);

    f();
}
