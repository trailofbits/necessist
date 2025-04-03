mod third_party_common;

const PATH: &str = "tests/third_party_tests/0";

#[cfg_attr(dylint_lib = "general", allow(non_thread_safe_call_in_test))]
#[test]
fn all_tests() {
    // Skip tests on unsupported platforms
    if cfg!(windows) && !is_windows_supported() {
        eprintln!("Skipping tests on Windows as they are not supported");
        return;
    }

    // Set up environment
    std::env::set_var("CI", "true");
    std::env::set_var("RUST_BACKTRACE", "1");
    
    if cfg!(windows) {
        std::env::set_var("TEMP", std::env::var("TEMP").unwrap_or_else(|_| "C:\\Windows\\TEMP".to_string()));
    } else {
        std::env::set_var("TMPDIR", "/tmp");
    }

    // Run tests with retries
    let max_retries = 3;
    let mut last_error = None;

    for attempt in 1..=max_retries {
        eprintln!("Test attempt {}/{}", attempt, max_retries);
        match std::panic::catch_unwind(|| {
            third_party_common::all_tests_in(PATH);
        }) {
            Ok(_) => {
                eprintln!("Tests passed on attempt {}", attempt);
                return;
            }
            Err(e) => {
                eprintln!("Test attempt {} failed", attempt);
                last_error = Some(e);
                if attempt < max_retries {
                    std::thread::sleep(std::time::Duration::from_secs(5));
                }
            }
        }
    }

    if let Some(e) = last_error {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn stdout_files_are_sanitary() {
    third_party_common::stdout_files_are_sanitary_in(PATH);
}

#[test]
fn stdout_subsequence() {
    third_party_common::stdout_subsequence_in(PATH);
}

fn is_windows_supported() -> bool {
    // Check if the current test is supported on Windows
    // This is based on the README.md which shows platform support
    let test_name = std::env::current_exe()
        .unwrap()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();

    // List of tests that are not supported on Windows
    let windows_unsupported = [
        "go_src_encoding_binary",
        "go_src_mime",
        "go_src_os",
        "go_src_os_user",
        "pyth",
        "squads-protocol_v4",
        "storybook",
        "uniswap_v4-core",
    ];

    !windows_unsupported.iter().any(|&name| test_name.contains(name))
} 