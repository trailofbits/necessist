use super::ParseHigh;
use anyhow::{Context, Result};
use necessist_core::{util, Config, LightContext, Span};
use std::{cell::RefCell, path::Path, rc::Rc};

pub type WalkDirResult = walkdir::Result<walkdir::DirEntry>;

pub trait ParseLow {
    type File;
    fn check_config(context: &LightContext, config: &Config) -> Result<()>;
    fn walk_dir(root: &Path) -> Box<dyn Iterator<Item = WalkDirResult>>;
    fn parse_file(&self, test_file: &Path) -> Result<Self::File>;
    fn visit(
        &mut self,
        context: &LightContext,
        config: &Config,
        test_file: &Path,
        file: &Self::File,
    ) -> Result<Vec<Span>>;
}

impl<T: ParseLow> ParseLow for Rc<RefCell<T>> {
    type File = T::File;
    fn check_config(context: &LightContext, config: &Config) -> Result<()> {
        T::check_config(context, config)
    }
    fn walk_dir(root: &Path) -> Box<dyn Iterator<Item = WalkDirResult>> {
        T::walk_dir(root)
    }
    fn parse_file(&self, test_file: &Path) -> Result<Self::File> {
        self.borrow().parse_file(test_file)
    }
    fn visit(
        &mut self,
        context: &LightContext,
        config: &Config,
        test_file: &Path,
        file: &Self::File,
    ) -> Result<Vec<Span>> {
        self.borrow_mut().visit(context, config, test_file, file)
    }
}

pub struct ParseAdapter<T>(pub T);

impl<T: ParseLow> ParseHigh for ParseAdapter<T> {
    fn parse(
        &mut self,
        context: &LightContext,
        config: &Config,
        test_files: &[&Path],
    ) -> Result<Vec<Span>> {
        T::check_config(context, config)?;

        let mut spans = Vec::new();

        let mut visit_test_file = |test_file: &Path| -> Result<()> {
            assert!(test_file.is_absolute());
            assert!(test_file.starts_with(context.root.as_path()));

            #[allow(clippy::unwrap_used)]
            let file = self.0.parse_file(test_file).with_context(|| {
                format!(
                    "Failed to parse {:?}",
                    util::strip_prefix(test_file, context.root).unwrap()
                )
            })?;

            let spans_visited = self.0.visit(context, config, test_file, &file)?;
            spans.extend(spans_visited);

            Ok(())
        };

        if test_files.is_empty() {
            for entry in T::walk_dir(context.root) {
                let entry = entry?;
                let path = entry.path();

                if !path.is_file() {
                    continue;
                }

                visit_test_file(path)?;
            }
        } else {
            for path in test_files {
                visit_test_file(path)?;
            }
        }

        Ok(spans)
    }
}
