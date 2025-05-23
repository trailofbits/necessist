use assert_cmd::assert::OutputAssertExt;
use cargo_metadata::MetadataCommand;
use regex::Regex;
use std::{
    env::{remove_var, set_current_dir, var},
    ffi::OsStr,
    fs::read_to_string,
    io::{Write, stderr},
    path::Path,
    process::{Command, exit},
    sync::Mutex,
};
use walkdir::WalkDir;

mod tempfile_util;
use tempfile_util::tempdir;

#[ctor::ctor]
fn initialize() {
    // smoelius: Run the CI tests if either the target OS is Linux or we are running locally, i.e.,
    // `CI` is _not_ set.
    if cfg!(not(target_os = "linux")) && var("CI").is_ok() {
        exit(0);
    }
    unsafe {
        remove_var("CARGO_TERM_COLOR");
    }
    set_current_dir("..").unwrap();
}

#[test]
fn clippy() {
    let mut command = Command::new("cargo");
    // smoelius: Remove `CARGO` environment variable to work around:
    // https://github.com/rust-lang/rust/pull/131729
    command.env_remove("CARGO");
    command
        .args(["+nightly", "clippy", "--all-features", "--all-targets"])
        .args(["--", "--deny=warnings"]);
    command.assert().success();
}

#[test]
fn doc() {
    Command::new("cargo")
        .args(["doc", "--document-private-items"])
        .env("RUSTDOCFLAGS", "-D warnings")
        .assert()
        .success();
}

#[test]
fn dylint() {
    Command::new("cargo")
        .args(["dylint", "--all", "--", "--all-features", "--all-targets"])
        .env("DYLINT_RUSTFLAGS", "--deny warnings")
        .assert()
        .success();
}

#[test]
fn format() {
    preserves_cleanliness("format", || {
        Command::new("cargo")
            .args(["+nightly", "fmt"])
            .assert()
            .success();
    });
}

#[test]
fn github() {
    const EXCEPTIONS: &[&str] = &[
        "ci",
        "ci_is_disabled",
        "dogfood",
        "general",
        "tempfile_util",
        "third_party_common",
    ];

    let metadata = MetadataCommand::new().no_deps().exec().unwrap();
    let package = metadata
        .packages
        .into_iter()
        .find(|package| package.name == "necessist")
        .unwrap();
    let mut metadata_tests = package
        .targets
        .into_iter()
        .filter_map(|target| {
            if target.is_test() && !EXCEPTIONS.contains(&target.name.as_str()) {
                Some(target.name)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    metadata_tests.sort();
    metadata_tests.push(String::from("other"));

    let ci_yml = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.github/workflows/ci.yml");
    let contents = read_to_string(ci_yml).unwrap();
    let test_array = contents
        .lines()
        .find_map(|line| line.trim_start().strip_prefix("test: "))
        .unwrap();
    let ci_tests = test_array
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap()
        .split(", ")
        .collect::<Vec<_>>();

    assert_eq!(metadata_tests, ci_tests);
}

#[test]
fn hack_feature_powerset_udeps() {
    Command::new("rustup")
        .env("RUSTFLAGS", "-D warnings")
        .args([
            "run",
            "nightly",
            "cargo",
            "hack",
            "--feature-powerset",
            "udeps",
        ])
        .assert()
        .success();
}

#[test]
fn license() {
    let re =
        Regex::new(r"^[^:]*\b(Apache-2.0|0BSD|BSD-\d-Clause|CC0-1.0|MIT|MPL-2\.0|Unicode-3.0)\b")
            .unwrap();

    for line in std::str::from_utf8(
        &Command::new("cargo")
            .arg("license")
            .assert()
            .success()
            .get_output()
            .stdout,
    )
    .unwrap()
    .lines()
    {
        if line == "AGPL-3.0 (3): necessist, necessist-backends, necessist-core" {
            continue;
        }
        assert!(re.is_match(line), "{line:?} does not match");
    }
}

#[test]
fn markdown_link_check() {
    let tempdir = tempdir().unwrap();

    Command::new("npm")
        .args(["install", "markdown-link-check"])
        .current_dir(&tempdir)
        .assert()
        .success();

    // smoelius: https://github.com/rust-lang/crates.io/issues/788
    let config = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/markdown_link_check.json");

    let readme_md = Path::new(env!("CARGO_MANIFEST_DIR")).join("../README.md");

    Command::new("npx")
        .args([
            "markdown-link-check",
            "--config",
            &config.to_string_lossy(),
            &readme_md.to_string_lossy(),
        ])
        .current_dir(&tempdir)
        .assert()
        .success();
}

/// `noninvasive_siblings` helps to expose circular module dependencies.
#[test]
fn noninvasive_siblings() {
    let re = Regex::new(r"use super::\{([^}]|\}[^;])*::").unwrap();

    for entry in WalkDir::new(Path::new(env!("CARGO_MANIFEST_DIR")).join(".."))
        .into_iter()
        .filter_entry(|entry| entry.path().file_name() != Some(OsStr::new("target")))
    {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.extension() != Some(OsStr::new("rs")) {
            continue;
        }

        // smoelius: The regex matches its own declaration. Ignore.
        if path.ends_with(file!()) {
            continue;
        }

        let contents = read_to_string(path).unwrap();

        if contents.contains("use super::{") {
            assert!(!re.is_match(&contents), "failed for `{}`", path.display());
        }
    }
}

#[test]
fn prettier() {
    const ARGS: &[&str] = &[
        "{}/**/*.json",
        "{}/**/*.md",
        "{}/**/*.yml",
        "!{}/fixtures/**",
        "!{}/backends/src/anchor/rfc8032_test_vector.json",
        "!{}/target/**",
    ];

    // smoelius: Prettier's handling of `..` seems to have changed between versions 3.4 and 3.5.
    // Manually collapsing the `..` avoids the problem.
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();

    let tempdir = tempdir().unwrap();

    Command::new("npm")
        .args(["install", "prettier"])
        .current_dir(&tempdir)
        .assert()
        .success();

    Command::new("npx")
        .args(["prettier", "--check"])
        .args(
            ARGS.iter()
                .map(|s| s.replace("{}", &parent.to_string_lossy())),
        )
        .current_dir(&tempdir)
        .assert()
        .success();
}

#[test]
fn readme_contains_usage() {
    let readme = read_to_string("README.md").unwrap();

    let assert = assert_cmd::Command::cargo_bin("necessist")
        .unwrap()
        .arg("--help")
        .assert();
    let stdout = &assert.get_output().stdout;

    let usage = std::str::from_utf8(stdout).unwrap();

    assert!(readme.contains(usage));
}

#[test]
fn readme_reference_links_are_sorted() {
    let re = Regex::new(r"^\[[^\]]*\]:").unwrap();
    let readme = read_to_string("README.md").unwrap();
    let links = readme
        .lines()
        .filter(|line| re.is_match(line))
        .collect::<Vec<_>>();
    let mut links_sorted = links.clone();
    links_sorted.sort_unstable();
    assert_eq!(links_sorted, links);
}

#[test]
fn readme_reference_links_are_used() {
    let re = Regex::new(r"(?m)^(\[[^\]]*\]):").unwrap();
    let readme = read_to_string("README.md").unwrap();
    for captures in re.captures_iter(&readme) {
        assert_eq!(2, captures.len());
        let m = captures.get(1).unwrap();
        assert!(
            readme[..m.start()].contains(m.as_str()),
            "{} is unused",
            m.as_str()
        );
    }
}

#[test]
fn readme_toc_is_accurate() {
    let readme = read_to_string("README.md").unwrap();
    let expected_toc = readme.lines().filter_map(|line| {
        line.strip_prefix("## ")
            .map(|suffix| format!("- [{suffix}](#{})", suffix.to_lowercase().replace(' ', "-")))
    });
    assert!(
        readme.contains(
            &std::iter::once(String::new())
                .chain(expected_toc)
                .chain(std::iter::once(String::new()))
                .map(|s| format!("{s}\n"))
                .collect::<String>()
        )
    );
}

#[test]
fn sort() {
    Command::new("cargo")
        .args(["sort", "--check", "--grouped"])
        .assert()
        .success();
}

#[test]
fn unmaintained() {
    Command::new("cargo")
        .args(["unmaintained", "--color=never", "--fail-fast"])
        .assert()
        .success();
}

static MUTEX: Mutex<()> = Mutex::new(());

fn preserves_cleanliness(test_name: &str, f: impl FnOnce()) {
    let _lock = MUTEX.lock().unwrap();

    if var("CI").is_err() && dirty().is_some() {
        #[allow(clippy::explicit_write)]
        writeln!(
            stderr(),
            "Skipping `{test_name}` test as repository is dirty"
        )
        .unwrap();
        return;
    }

    f();

    if let Some(stdout) = dirty() {
        panic!("{}", stdout);
    }
}

fn dirty() -> Option<String> {
    let output = Command::new("git")
        .args(["diff", "--exit-code"])
        .output()
        .unwrap();

    if output.status.success() {
        None
    } else {
        Some(String::from_utf8(output.stdout).unwrap())
    }
}
