use anyhow::Result;
use necessist_core::{Config, Interface, LightContext, Postprocess, Span};
use std::{
    path::{Path, PathBuf},
    rc::Rc,
};
use subprocess::Exec;

#[derive(Debug, Default)]
pub struct Foundry {
    _root: Rc<PathBuf>,
}

impl Foundry {
    pub fn applicable(context: &LightContext) -> Result<Option<Self>> {
        if context.root.join("foundry.toml").try_exists()? {
            Ok(Some(Self::new(context)))
        } else {
            Ok(None)
        }
    }

    fn new(context: &LightContext) -> Self {
        Self {
            _root: Rc::new(context.root.to_path_buf()),
        }
    }
}

impl Interface for Foundry {
    fn parse(
        &mut self,
        _context: &LightContext,
        _config: &Config,
        _test_files: &[&Path],
    ) -> Result<Vec<Span>> {
        Ok(vec![])
    }

    fn dry_run(&self, _context: &LightContext, _test_file: &Path) -> Result<()> {
        Ok(())
    }

    fn exec(
        &self,
        _context: &LightContext,
        _span: &Span,
    ) -> Result<Option<(Exec, Option<Box<Postprocess>>)>> {
        Ok(None)
    }
}
