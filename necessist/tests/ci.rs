use assert_cmd::{assert::OutputAssertExt, cargo::cargo_bin_cmd};
use cargo_metadata::MetadataCommand;
use elaborate::std::{
    env::{set_current_dir_wc, var_wc},
    fs::read_to_string_wc,
    path::PathContext,
};
use regex::Regex;
use std::{
    env::remove_var,
    ffi::OsStr,
    path::Path,
    process::{Command, exit},
};
use testing::tempfile_util::tempdir;
use walkdir::WalkDir;

#[ctor::ctor(unsafe)]
fn initialize() {
    // smoelius: Run the CI tests if either the target OS is Linux or we are running locally, i.e.,
    // `CI` is _not_ set.
    if cfg!(not(target_os = "linux")) && var_wc("CI").is_ok() {
        exit(0);
    }
    unsafe {
        remove_var("CARGO_TERM_COLOR");
    }
    set_current_dir_wc("..").unwrap();
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
fn elaborate_disallowed_methods() {
    elaborate::disallowed_methods()
        .args(["--all-features", "--all-targets"])
        .assert()
        .success();
}

#[test]
fn fmt() {
    Command::new("cargo")
        .args(["+nightly", "fmt", "--check"])
        .assert()
        .success();
}

#[test]
fn github() {
    const EXCEPTIONS: &[&str] = &[
        "ci",
        "ci_is_disabled",
        "dogfood",
        "general",
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

    let ci_yml = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../.github/workflows/ci.yml"
    ));
    let contents = read_to_string_wc(ci_yml).unwrap();
    let document = yaml_serde::from_str::<yaml_serde::Value>(&contents).unwrap();
    let test_sequence = document
        .get("jobs")
        .and_then(|value| value.get("test"))
        .and_then(|value| value.get("strategy"))
        .and_then(|value| value.get("matrix"))
        .and_then(|value| value.get("test"))
        .and_then(yaml_serde::Value::as_sequence)
        .unwrap();
    let ci_tests = test_sequence
        .iter()
        .map(|value| value.as_str())
        .collect::<Option<Vec<_>>>()
        .unwrap();

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
            "--all-targets",
        ])
        .assert()
        .success();
}

#[test]
fn license() {
    let re = Regex::new(
        r"^[^:]*\b(Apache-2.0|0BSD|BSD-\d-Clause|CC0-1.0|MIT|MPL-2\.0|Unicode-3.0|Zlib)\b",
    )
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
        if line == "AGPL-3.0 (4): necessist, necessist-backends, necessist-core, testing" {
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
    let config = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/markdown_link_check.json"
    ));

    let readme_md = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../README.md"));

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
// smoelius: I am disabling this test. It started failing with the addition of PHP support and I
// don't feel like debugging it.
#[test]
#[ignore = "started failing when PHP support was added"]
fn noninvasive_siblings() {
    let re = Regex::new(r"use super::\{([^}]|\}[^;])*::").unwrap();

    for entry in WalkDir::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
        .into_iter()
        .filter_entry(|entry| entry.file_name() != "target")
    {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.extension_wc().ok() != Some(OsStr::new("rs")) {
            continue;
        }

        // smoelius: The regex matches its own declaration. Ignore.
        if path.ends_with(file!()) {
            continue;
        }

        let contents = read_to_string_wc(path).unwrap();

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
        "!{}/backends/src/anchor/rfc8032_test_vector.json",
        "!{}/fixtures/**",
        "!{}/necessist/tests/supply_chain.json",
        "!{}/target/**",
    ];

    // smoelius: Prettier's handling of `..` seems to have changed between versions 3.4 and 3.5.
    // Manually collapsing the `..` avoids the problem.
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).parent_wc().unwrap();

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
    let readme = read_to_string_wc("README.md").unwrap();

    let assert = cargo_bin_cmd!("necessist").arg("--help").assert();
    let stdout = &assert.get_output().stdout;

    let usage = std::str::from_utf8(stdout).unwrap();

    assert!(readme.contains(usage));
}

#[test]
fn readme_reference_links_are_sorted() {
    let re = Regex::new(r"^\[[^\]]*\]:").unwrap();
    let readme = read_to_string_wc("README.md").unwrap();
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
    let readme = read_to_string_wc("README.md").unwrap();
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
    let readme = read_to_string_wc("README.md").unwrap();
    let expected_toc = readme.lines().filter_map(|line| {
        line.strip_prefix("## ").map(|suffix| {
            format!(
                "- [{suffix}](#{})",
                suffix
                    .to_lowercase()
                    .replace(' ', "-")
                    .replace(['(', ')'], "")
            )
        })
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
fn supply_chain() {
    supply_chain::check(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/supply_chain.json"
    ));
}

#[test]
fn unmaintained() {
    Command::new("cargo")
        .args(["unmaintained", "--color=never", "--fail-fast"])
        .assert()
        .success();
}
