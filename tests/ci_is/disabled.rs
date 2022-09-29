use std::io::{stderr, Write};

#[test]
fn ci_is_disabled() {
    #[allow(clippy::explicit_write)]
    writeln!(stderr(), "Warning: feature `ci` is disabled").unwrap();
}
