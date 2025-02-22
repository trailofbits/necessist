use std::{
    env::var,
    io::{Write, stderr},
    process::exit,
};

#[ctor::ctor]
fn initialize() {
    // smoelius: Run the CI tests if either the target OS is Linux or we are running locally, i.e.,
    // `CI` is _not_ set.
    if cfg!(target_os = "linux") || var("CI").is_err() {
        exit(0);
    }
}

#[test]
fn warn() {
    #[allow(clippy::explicit_write)]
    writeln!(stderr(), "Warning: the CI tests are disabled").unwrap();
}
