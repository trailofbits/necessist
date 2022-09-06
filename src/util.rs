use std::{env::current_dir, path::Path};

pub(crate) fn strip_current_dir(path: &Path) -> &Path {
    current_dir()
        .ok()
        .and_then(|dir| path.strip_prefix(&dir).ok())
        .unwrap_or(path)
}
