#![cfg(not(feature = "ci"))]

use std::io::{stderr, Write};

#[test]
fn warn() {
    #[allow(clippy::explicit_write)]
    writeln!(stderr(), "Warning: feature `ci` is disabled").unwrap();
}
