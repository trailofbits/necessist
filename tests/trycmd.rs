use assert_cmd::prelude::*;
use necessist_core::util;
use regex::Regex;
use std::{
    env::{remove_var, set_var},
    ffi::OsStr,
    fs::{create_dir_all, read_dir, read_to_string, remove_file},
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};
use trycmd::TestCases;

const TIMEOUT: &str = "300";
const MAX_RETRIES: u32 = 5;
const RETRY_DELAY: u64 = 5;

#[ctor::ctor]
fn initialize() {
    unsafe {
        remove_var("CARGO_TERM_COLOR");
        set_var("CI", "true");
        set_var("TRYCMD_TIMEOUT_MS", "600000");
        set_var("RUST_BACKTRACE", "1");
        
        // Set temp directory for cross-platform compatibility
        if cfg!(windows) {
            set_var("TEMP", std::env::var("TEMP").unwrap_or_else(|_| "C:\\Windows\\TEMP".to_string()));
        } else {
            set_var("TMPDIR", "/tmp");
        }
        
        // Ensure we're in the correct directory
        if let Err(e) = std::env::set_current_dir(env!("CARGO_MANIFEST_DIR")) {
            eprintln!("Failed to set working directory: {}", e);
        }
    }
}

fn get_test_directories() -> Vec<PathBuf> {
    let base_dirs = [
        "fixtures/basic",
        "fixtures/dry_run_failure",
        "fixtures/cfg",
    ];

    let test_subdirs = [
        "test",
        "test/__snapshots__",
        "test/findings",
        "test/utils",
        "test/utils/reports",
        "SOIR/test",
        "SOIR/test/__snapshots__",
        "SOIR/test/findings",
        "SOIR/test/utils/reports",
    ];

    let mut dirs = Vec::new();
    for base in base_dirs.iter() {
        let base_path = PathBuf::from(base);
        for subdir in test_subdirs.iter() {
            dirs.push(base_path.join(subdir.replace('/', std::path::MAIN_SEPARATOR_STR)));
        }
    }
    dirs
}

fn clean_test_artifacts() -> Result<(), std::io::Error> {
    let db_files = [
        "fixtures/basic/necessist.db",
        "fixtures/dry_run_failure/necessist.db",
        "fixtures/cfg/necessist.db",
    ];

    for db_file in db_files {
        let path = PathBuf::from(db_file);
        if path.exists() {
            eprintln!("Removing file: {}", path.display());
            for attempt in 1..=MAX_RETRIES {
                match remove_file(&path) {
                    Ok(_) => {
                        eprintln!("Successfully removed: {}", path.display());
                        break;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                        eprintln!("Attempt {} - Permission denied, retrying: {}", attempt, path.display());
                        thread::sleep(Duration::from_secs(2));
                        continue;
                    }
                    Err(e) => {
                        eprintln!("Failed to remove {} (attempt {}): {}", path.display(), attempt, e);
                        if attempt == MAX_RETRIES {
                            return Err(e);
                        }
                        thread::sleep(Duration::from_secs(2));
                    }
                }
            }
        }
    }
    Ok(())
}

fn create_test_directories() -> std::io::Result<()> {
    // Create base fixture directories first
    let fixture_dirs = [
        "fixtures",
        "fixtures/basic",
        "fixtures/dry_run_failure",
        "fixtures/cfg",
        "tests",
        "tests/necessist_db_absent",
        "tests/necessist_db_present",
    ];

    for dir in fixture_dirs.iter() {
        let path = PathBuf::from(dir);
        if !path.exists() {
            eprintln!("Creating fixture directory: {}", path.display());
            create_dir_all(&path)?;
        }
    }

    // Then create all test directories
    for dir in get_test_directories() {
        eprintln!("Creating test directory: {}", dir.display());
        match create_dir_all(&dir) {
            Ok(_) => eprintln!("Successfully created: {}", dir.display()),
            Err(e) => {
                eprintln!("Error creating {}: {}", dir.display(), e);
                // Only fail if the error is not AlreadyExists
                if e.kind() != std::io::ErrorKind::AlreadyExists {
                    return Err(e);
                }
            }
        }
    }
    Ok(())
}

fn setup_test_environment() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("Setting up test environment...");
    eprintln!("Current directory: {:?}", std::env::current_dir()?);
    
    clean_test_artifacts()?;
    create_test_directories()?;
    
    // Verify all required directories exist
    for dir in get_test_directories() {
        if !dir.exists() {
            eprintln!("Warning: Required directory not found: {}", dir.display());
            // Try to create it one more time
            create_dir_all(&dir)?;
        }
    }
    
    thread::sleep(Duration::from_secs(2));
    Ok(())
}

fn run_test_cases(pattern: &str) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("Running test cases: {}", pattern);
    
    for attempt in 1..=MAX_RETRIES {
        eprintln!("Attempt {}/{}", attempt, MAX_RETRIES);
        setup_test_environment()?;
        
        let mut test_cases = TestCases::new();
        test_cases = test_cases
            .env("TRYCMD", "1")
            .env("CI", "true")
            .env("RUST_BACKTRACE", "1");

        if cfg!(windows) {
            test_cases = test_cases.env("TEMP", std::env::var("TEMP").unwrap_or_else(|_| "C:\\Windows\\TEMP".to_string()));
        } else {
            test_cases = test_cases.env("TMPDIR", "/tmp");
        }

        let result = test_cases
            .timeout(Duration::from_secs(300))
            .case(pattern)
            .run();

        match result {
            Ok(_) => {
                eprintln!("Test cases successful");
                return Ok(());
            }
            Err(e) => {
                eprintln!("Test attempt {} failed: {}", attempt, e);
                if attempt < MAX_RETRIES {
                    eprintln!("Cleaning up and retrying...");
                    clean_test_artifacts()?;
                    create_test_directories()?;
                    thread::sleep(Duration::from_secs(10));
                } else {
                    eprintln!("All attempts failed: {}", e);
                    return Err(e.into());
                }
            }
        }
    }
    Ok(())
}

#[test]
fn trycmd() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("Starting trycmd test...");
    eprintln!("Current directory: {:?}", std::env::current_dir()?);
    
    // Initial setup
    setup_test_environment()?;

    // Run necessist command first
    eprintln!("Running necessist command...");
    let mut cmd = Command::cargo_bin("necessist")?;
    cmd.args([
        "--root", 
        "fixtures/basic",
        "--timeout", 
        TIMEOUT,
        "--no-progress",
    ])
    .env("CI", "true")
    .env("RUST_BACKTRACE", "1");

    // Set platform-specific environment variables
    if cfg!(windows) {
        cmd.env("TEMP", std::env::var("TEMP").unwrap_or_else(|_| "C:\\Windows\\TEMP".to_string()));
    } else {
        cmd.env("TMPDIR", "/tmp");
    }

    cmd.assert().success();

    // Run tests in sequence with proper cleanup between each set
    eprintln!("Running necessist_db_absent tests...");
    run_test_cases("tests/necessist_db_absent/*.toml")?;

    // Clean up and recreate environment
    setup_test_environment()?;

    eprintln!("Running necessist_db_present tests...");
    run_test_cases("tests/necessist_db_present/*.toml")?;

    eprintln!("Cleaning up...");
    clean_test_artifacts()?;
    
    Ok(())
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

        assert!(!re.is_match(&contents), "`{}` matches", path.display());
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
            "failed for `{}`",
            path.display()
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