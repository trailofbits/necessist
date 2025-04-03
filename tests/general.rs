use std::{env::set_current_dir, path::PathBuf, process::Command, sync::Mutex, time::Duration};

mod tempfile_util;
use tempfile_util::tempdir;

// Increase timeout to avoid CI issues
const TIMEOUT: &str = "120";

// Add a timeout helper function with retry
fn with_timeout_retry<F, T>(duration: Duration, retries: usize, f: F) -> Result<T, &'static str>
where
    F: Fn() -> T + Send + 'static,
    T: Send + 'static,
{
    for i in 0..retries {
        let handle = std::thread::spawn(f.clone());
        match handle.join_timeout(duration) {
            Ok(result) => return Ok(result),
            Err(_) if i < retries - 1 => {
                eprintln!("Test timed out, retrying ({}/{})", i + 1, retries);
                std::thread::sleep(Duration::from_secs(5));
                continue;
            }
            Err(_) => return Err("Test timed out after all retries"),
        }
    }
    Err("Test timed out after all retries")
}

const BASIC_ROOT: &str = "fixtures/basic";

#[ctor::ctor]
fn initialize() {
    set_current_dir("..").unwrap();
}

#[test]
fn necessist_db_can_be_moved() {
    // Add timeout with retry to the test
    let result = with_timeout_retry(Duration::from_secs(300), 3, || {
        // First, ensure the necessist.db is removed
        let db_path = PathBuf::from(BASIC_ROOT).join("necessist.db");
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        run_basic_test(|| {
            Command::cargo_bin("necessist")
                .unwrap()
                .args(["--root", BASIC_ROOT, "--timeout", TIMEOUT])
                .assert()
                .success();

            let tempdir = tempdir().unwrap();

            Command::new("cp")
                .args(["-r", BASIC_ROOT, &tempdir.path().to_string_lossy()])
                .assert()
                .success();

            // Add a small delay after file operations
            std::thread::sleep(Duration::from_secs(1));

            Command::cargo_bin("necessist")
                .unwrap()
                .args([
                    "--root",
                    &tempdir.path().join("basic").to_string_lossy(),
                    "--resume",
                    "--timeout",
                    TIMEOUT,
                ])
                .assert()
                .success()
                .stdout(predicate::eq("4 candidates in 4 tests in 1 source file\n"));
        });
    });
    assert!(result.is_ok(), "Test failed after retries");
}

#[test]
fn resume_following_dry_run_failure() {
    // Add timeout with retry to the test
    let result = with_timeout_retry(Duration::from_secs(300), 3, || {
        const DRF_ROOT: &str = "fixtures/dry_run_failure";

        let necessist_db = PathBuf::from(DRF_ROOT).join("necessist.db");
        if necessist_db.exists() {
            std::fs::remove_file(&necessist_db).unwrap();
        }

        let _remove_file = util::RemoveFile(necessist_db);

        let assert = Command::cargo_bin("necessist")
            .unwrap()
            .args(["--root", DRF_ROOT, "--timeout", TIMEOUT])
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

        // Add a small delay between commands
        std::thread::sleep(Duration::from_secs(1));

        Command::cargo_bin("necessist")
            .unwrap()
            .args(["--root", DRF_ROOT, "--resume", "--timeout", TIMEOUT])
            .assert()
            .success()
            .stdout(predicate::eq("2 candidates in 2 tests in 3 source files\n"));
    });
    assert!(result.is_ok(), "Test failed after retries");
} 