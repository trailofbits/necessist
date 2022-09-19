// smoelius: Items within this module are semver exempt and could change at any time.

use anyhow::{Context, Result};
use std::{env::current_dir, path::Path};

#[must_use]
pub fn strip_current_dir(path: &Path) -> &Path {
    current_dir()
        .ok()
        .and_then(|dir| strip_prefix(path, &dir).ok())
        .unwrap_or(path)
}

#[allow(clippy::disallowed_methods)]
pub fn strip_prefix<'a>(path: &'a Path, base: &Path) -> Result<&'a Path> {
    path.strip_prefix(base)
        .with_context(|| format!("{:?} is not a prefix of {:?}", base, path))
}
