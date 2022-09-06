use assert_cmd::prelude::*;
use std::process::Command;

#[test]
fn proptest_urls_https() {
    Command::new("tests/proptest_urls.sh").args(&["https"]).assert().success();
}

#[test]
#[ignore]
fn proptest_urls_ssh() {
    Command::new("tests/proptest_urls.sh").args(&["ssh"]).assert().success();
}
