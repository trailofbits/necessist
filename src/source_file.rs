use crate::util;
use std::{
    ops::Deref,
    path::{Path, PathBuf},
    rc::Rc,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceFile {
    root: Rc<PathBuf>,
    path: Rc<PathBuf>,
}

impl SourceFile {
    pub fn new(root: Rc<PathBuf>, path: Rc<PathBuf>) -> Self {
        assert!(root.is_absolute());
        assert!(path.starts_with(&*root));
        Self { root, path }
    }

    #[allow(clippy::unwrap_used)]
    fn relative_path(&self) -> &Path {
        util::strip_prefix(&self.path, &self.root).unwrap()
    }
}

/// Gives the path relative to the project root
impl std::fmt::Display for SourceFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.relative_path().to_string_lossy())
    }
}

impl Ord for SourceFile {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.relative_path().cmp(other.relative_path())
    }
}

impl PartialOrd for SourceFile {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> AsRef<T> for SourceFile
where
    Rc<PathBuf>: AsRef<T>,
{
    fn as_ref(&self) -> &T {
        self.path.as_ref()
    }
}

impl Deref for SourceFile {
    type Target = Path;
    #[allow(clippy::explicit_deref_methods)]
    fn deref(&self) -> &Self::Target {
        self.path.deref()
    }
}
