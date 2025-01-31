use std::{env::temp_dir, io::Result, path::PathBuf, sync::LazyLock};
use tempfile::tempdir_in;

pub use tempfile::TempDir;

// smoelius: macOS requires the paths to be canonicalized, because `/tmp` is symlinked to
// `private/tmp`.
#[allow(clippy::disallowed_methods)]
static TEMPDIR_ROOT: LazyLock<PathBuf> = LazyLock::new(|| dunce::canonicalize(temp_dir()).unwrap());

/// Canonicalizes [`std::env::temp_dir`] and creates a directory therein.
///
/// Canonicalizing early can be useful if one wants to avoid canonicalizing later on.
pub fn tempdir() -> Result<TempDir> {
    #[allow(clippy::disallowed_methods)]
    tempdir_in(&*TEMPDIR_ROOT)
}
