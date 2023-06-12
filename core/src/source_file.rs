use crate::{offset_calculator::OffsetCalculator, to_console_string::ToConsoleString, util};
use anyhow::Result;
use std::{
    cell::RefCell,
    collections::HashMap,
    fs::read_to_string,
    ops::Deref,
    path::{Path, PathBuf},
    rc::Rc,
};

thread_local! {
    static ROOT: RefCell<Option<Rc<PathBuf>>> = RefCell::new(None);
    static SOURCE_FILES: RefCell<HashMap<PathBuf, SourceFile>> = RefCell::new(HashMap::new());
}

#[derive(Clone, Eq, PartialEq)]
pub struct SourceFile {
    inner: Rc<Inner>,
}

struct Inner {
    root: Rc<PathBuf>,
    path: PathBuf,
    contents: &'static str,
    offset_calculator: RefCell<OffsetCalculator<'static>>,
}

impl Eq for Inner {}

impl PartialEq for Inner {
    fn eq(&self, other: &Self) -> bool {
        self.root.eq(&other.root) && self.path.eq(&other.path)
    }
}

impl SourceFile {
    pub fn new(root: Rc<PathBuf>, path: PathBuf) -> Result<Self> {
        ROOT.with(|root_prev| {
            let mut root_prev = root_prev.borrow_mut();

            if let Some(root_prev) = root_prev.as_ref() {
                assert_eq!(*root_prev, root);
            } else {
                assert!(root.is_absolute());
                *root_prev = Some(root.clone());
            }
        });

        assert!(path.starts_with(&*root));

        SOURCE_FILES.with(|source_files| {
            let mut source_files = source_files.borrow_mut();

            if let Some(source_file) = source_files.get(&path) {
                Ok(source_file.clone())
            } else {
                let contents = read_to_string(&path)?;
                let leaked = Box::leak(contents.into_boxed_str());
                let source_file = Self {
                    inner: Rc::new(Inner {
                        root,
                        path: path.clone(),
                        contents: leaked,
                        offset_calculator: RefCell::new(OffsetCalculator::new(leaked)),
                    }),
                };
                source_files.insert(path, source_file.clone());
                Ok(source_file)
            }
        })
    }

    fn relative_path(&self) -> &Path {
        #[allow(clippy::unwrap_used)]
        util::strip_prefix(&self.inner.path, &self.inner.root).unwrap()
    }

    // smoelius: Leaking the file contents is a hack.
    #[must_use]
    pub fn contents(&self) -> &'static str {
        self.inner.contents
    }

    #[must_use]
    pub fn offset_calculator(&self) -> &RefCell<OffsetCalculator<'static>> {
        &self.inner.offset_calculator
    }
}

impl std::fmt::Debug for SourceFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <_ as std::fmt::Debug>::fmt(&self.inner.path, f)
    }
}

/// Gives the path relative to the project root
impl std::fmt::Display for SourceFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.relative_path().to_string_lossy())
    }
}

impl ToConsoleString for SourceFile {
    fn to_console_string(&self) -> String {
        util::strip_current_dir(self).to_string_lossy().to_string()
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

impl AsRef<PathBuf> for SourceFile {
    fn as_ref(&self) -> &PathBuf {
        &self.inner.path
    }
}

impl Deref for SourceFile {
    type Target = Path;
    fn deref(&self) -> &Self::Target {
        #[allow(clippy::explicit_deref_methods)]
        self.inner.path.deref()
    }
}
