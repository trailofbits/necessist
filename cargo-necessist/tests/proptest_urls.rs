use assert_cmd::prelude::*;
use std::process::Command;

#[test]
fn proptest_urls() {
    Command::new("tests/proptest_urls.sh").assert().success();
}
