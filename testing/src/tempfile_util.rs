//! Temporary-directory helpers for tests that compare or pass around paths.
//!
//! On macOS, `/tmp` is a symlink to `/private/tmp`. A temporary directory created beneath the
//! former can therefore have a different lexical path from the path reported by a subprocess or
//! returned after canonicalization. That difference can make otherwise equivalent paths fail
//! equality and prefix checks.
//!
//! This module canonicalizes the system temporary-directory root before creating directories, so
//! the paths used by the tests and their subprocesses have a consistent spelling. Tests should use
//! [`tempdir`] instead of [`tempfile::tempdir`] for that reason.

use std::{env::temp_dir, io::Result, path::PathBuf, sync::LazyLock};
use tempfile::tempdir_in;

pub use tempfile::TempDir;

// Cache the canonical root both to avoid repeated filesystem work and to ensure all temporary
// directories created during a test run use the same path representation.
#[allow(clippy::disallowed_methods)]
static TEMPDIR_ROOT: LazyLock<PathBuf> = LazyLock::new(|| dunce::canonicalize(temp_dir()).unwrap());

/// Canonicalizes [`std::env::temp_dir`] and creates a directory therein.
///
/// Canonicalizing early can be useful if one wants to avoid canonicalizing later on.
pub fn tempdir() -> Result<TempDir> {
    #[allow(clippy::disallowed_methods)]
    tempdir_in(&*TEMPDIR_ROOT)
}
