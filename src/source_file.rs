use std::{
    ops::Deref,
    path::{Path, PathBuf},
    rc::Rc,
};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SourceFile(pub Rc<PathBuf>);

impl SourceFile {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self(Rc::new(path.as_ref().to_path_buf()))
    }
}

impl<T> AsRef<T> for SourceFile
where
    Rc<PathBuf>: AsRef<T>,
{
    fn as_ref(&self) -> &T {
        self.0.as_ref()
    }
}

impl Deref for SourceFile {
    type Target = Path;
    #[allow(clippy::explicit_deref_methods)]
    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}
