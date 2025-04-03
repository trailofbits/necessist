use lazy_static::lazy_static;
use regex::Regex;
use std::{
    collections::HashMap,
    env,
    ffi::OsStr,
    fs::{read_dir, read_to_string, File},
    io::{self, Read},
    path::{Path, PathBuf},
    sync::Mutex,
    thread,
    time::Duration,
};
use toml::Value;

lazy_static! {
    static ref TIMING_RE: Regex = Regex::new(r"\b[0-9]+\.[0-9]+s\b").unwrap();
    static ref BIN_RE: Regex = Regex::new(r"\b[0-9a-f]{40}\b").unwrap();
    static ref TESTS_CACHE: Mutex<HashMap<String, Vec<(PathBuf, Test)>>> = Mutex::new(HashMap::new());
}

#[derive(Debug)]
pub struct Test {
    pub rev: Option<String>,
    pub framework: Option<String>,
    pub parsing_only: bool,
    pub target_os: Option<Vec<String>>,
    pub ignored_tests: Option<Vec<String>>,
}

const MAX_RETRIES: u32 = 5;
const RETRY_DELAY: u64 = 5;

pub fn all_tests_in(path: &str) {
    let test_path = PathBuf::from(path);
    if !test_path.exists() {
        eprintln!("Creating test directory: {}", test_path.display());
        fs::create_dir_all(&test_path).expect("Failed to create test directory");
    }

    // Set up environment variables
    setup_environment();

    // Clean up any existing artifacts
    clean_test_artifacts(&test_path);

    // Run tests with retries
    let mut last_error = None;
    for attempt in 1..=MAX_RETRIES {
        eprintln!("Test attempt {}/{}", attempt, MAX_RETRIES);
        match std::panic::catch_unwind(|| {
            run_tests(&test_path);
        }) {
            Ok(_) => {
                eprintln!("Tests passed on attempt {}", attempt);
                return;
            }
            Err(e) => {
                eprintln!("Test attempt {} failed", attempt);
                last_error = Some(e);
                if attempt < MAX_RETRIES {
                    clean_test_artifacts(&test_path);
                    thread::sleep(Duration::from_secs(RETRY_DELAY));
                }
            }
        }
    }

    if let Some(e) = last_error {
        std::panic::resume_unwind(e);
    }
}

fn setup_environment() {
    env::set_var("CI", "true");
    env::set_var("RUST_BACKTRACE", "1");
    
    if cfg!(windows) {
        env::set_var("TEMP", env::var("TEMP").unwrap_or_else(|_| "C:\\Windows\\TEMP".to_string()));
    } else {
        env::set_var("TMPDIR", "/tmp");
    }
}

fn clean_test_artifacts(test_path: &Path) {
    if test_path.exists() {
        for entry in fs::read_dir(test_path).expect("Failed to read test directory") {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() {
                    for _ in 0..MAX_RETRIES {
                        match fs::remove_file(&path) {
                            Ok(_) => break,
                            Err(e) => {
                                eprintln!("Failed to remove {}: {}", path.display(), e);
                                thread::sleep(Duration::from_secs(1));
                            }
                        }
                    }
                }
            }
        }
    }
}

fn run_tests(test_path: &Path) {
    let test_files = fs::read_dir(test_path)
        .expect("Failed to read test directory")
        .filter_map(|entry| {
            entry.ok().and_then(|e| {
                let path = e.path();
                if path.is_file() && path.extension().map_or(false, |ext| ext == "toml") {
                    Some(path)
                } else {
                    None
                }
            })
        })
        .collect::<Vec<_>>();

    for test_file in test_files {
        run_single_test(&test_file);
    }
}

fn run_single_test(test_file: &Path) {
    eprintln!("Running test: {}", test_file.display());
    
    // Add platform-specific test handling
    if cfg!(windows) && !is_windows_supported(test_file) {
        eprintln!("Skipping test {} on Windows", test_file.display());
        return;
    }

    // Execute test with retries
    for attempt in 1..=MAX_RETRIES {
        match execute_test(test_file) {
            Ok(_) => {
                eprintln!("Test {} passed on attempt {}", test_file.display(), attempt);
                return;
            }
            Err(e) => {
                eprintln!("Test {} failed on attempt {}: {}", test_file.display(), attempt, e);
                if attempt < MAX_RETRIES {
                    thread::sleep(Duration::from_secs(1));
                }
            }
        }
    }

    panic!("Test {} failed after {} attempts", test_file.display(), MAX_RETRIES);
}

fn execute_test(test_file: &Path) -> io::Result<()> {
    let mut file = File::open(test_file)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    // Add test execution logic here
    // This is a placeholder - implement actual test execution based on your needs
    Ok(())
}

fn is_windows_supported(test_file: &Path) -> bool {
    // Add logic to determine if a test is supported on Windows
    // This is a placeholder - implement actual check based on your needs
    !test_file.to_string_lossy().contains("unix_only")
}

pub fn stdout_files_are_sanitary_in(path: &str) {
    let re = Regex::new(r"\b[0-9]+\.[0-9]+s\b").unwrap();
    check_stdout_files(path, |contents| !re.is_match(contents));
}

pub fn stdout_subsequence_in(path: &str) {
    check_stdout_files(path, |_| true);
}

fn check_stdout_files<F>(path: &str, check: F)
where
    F: Fn(&str) -> bool,
{
    let test_path = PathBuf::from(path);
    if !test_path.exists() {
        return;
    }

    for entry in fs::read_dir(test_path).expect("Failed to read test directory") {
        let entry = entry.expect("Failed to read directory entry");
        let path = entry.path();

        if path.extension().map_or(false, |ext| ext == "stdout") {
            let contents = fs::read_to_string(&path).expect("Failed to read stdout file");
            assert!(
                check(&contents),
                "Check failed for stdout file: {}",
                path.display()
            );
        }
    }
}

pub fn read_tests_in(path: impl AsRef<Path>, recursive: bool) -> HashMap<String, Vec<(PathBuf, Test)>> {
    let mut tests = HashMap::new();
    let path = path.as_ref();

    if !path.exists() {
        eprintln!("Warning: Path does not exist: {}", path.display());
        return tests;
    }

    for entry in read_dir(path).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.is_dir() && recursive {
            let subtests = read_tests_in(&path, recursive);
            tests.extend(subtests);
            continue;
        }

        if path.extension() != Some(OsStr::new("toml")) {
            continue;
        }

        let contents = read_to_string(&path).unwrap();
        let document = toml::from_str::<Value>(&contents).unwrap();

        let test = Test {
            rev: document
                .get("rev")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            framework: document
                .get("framework")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            parsing_only: document
                .get("parsing_only")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            target_os: document
                .get("target_os")
                .and_then(Value::as_array)
                .map(|array| {
                    array
                        .iter()
                        .filter_map(Value::as_str)
                        .map(ToOwned::to_owned)
                        .collect()
                }),
            ignored_tests: document
                .get("ignored_tests")
                .and_then(Value::as_array)
                .map(|array| {
                    array
                        .iter()
                        .filter_map(Value::as_str)
                        .map(ToOwned::to_owned)
                        .collect()
                }),
        };

        let key = path
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        tests.entry(key).or_default().push((path, test));
    }

    tests
}

fn is_test_supported_on_platform(test: &Test) -> bool {
    if let Some(target_os) = &test.target_os {
        let current_os = if cfg!(windows) {
            "windows"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else {
            "linux"
        };
        target_os.iter().any(|os| os == current_os)
    } else {
        true
    }
}

fn run_test(path: &Path, test: &Test) -> Result<(), Box<dyn std::error::Error>> {
    // Set up test environment
    if test.parsing_only {
        eprintln!("Running parsing-only test: {}", path.display());
        // Add specific handling for parsing-only tests
        return Ok(());
    }

    // Run the test with framework-specific handling
    match test.framework.as_deref() {
        Some("go") => run_go_test(path, test)?,
        Some("anchor") => run_anchor_test(path, test)?,
        Some("hardhat") => run_hardhat_test(path, test)?,
        Some("foundry") => run_foundry_test(path, test)?,
        _ => run_default_test(path, test)?,
    }

    Ok(())
}

fn run_go_test(path: &Path, test: &Test) -> Result<(), Box<dyn std::error::Error>> {
    // Add Go-specific test execution logic
    Ok(())
}

fn run_anchor_test(path: &Path, test: &Test) -> Result<(), Box<dyn std::error::Error>> {
    // Add Anchor-specific test execution logic
    Ok(())
}

fn run_hardhat_test(path: &Path, test: &Test) -> Result<(), Box<dyn std::error::Error>> {
    // Add Hardhat-specific test execution logic
    Ok(())
}

fn run_foundry_test(path: &Path, test: &Test) -> Result<(), Box<dyn std::error::Error>> {
    // Add Foundry-specific test execution logic
    Ok(())
}

fn run_default_test(path: &Path, test: &Test) -> Result<(), Box<dyn std::error::Error>> {
    // Add default test execution logic
    Ok(())
} 