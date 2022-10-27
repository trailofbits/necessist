use std::io::{stderr, Write};

#[test]
fn ci_nightly_is_disabled() {
    #[allow(clippy::explicit_write)]
    writeln!(stderr(), "Warning: feature `ci_nightly` is disabled").unwrap();
}
