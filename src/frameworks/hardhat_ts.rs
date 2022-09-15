use super::{Interface, Postprocess};
use crate::{LightContext, Span};
use anyhow::Result;
use std::path::Path;
use subprocess::Exec;

#[derive(Debug, Default)]
pub(super) struct HardhatTs;

impl Interface for HardhatTs {
    fn applicable(&self, context: &LightContext) -> Result<bool> {
        Ok(context.root.join("hardhat.config.ts").exists())
    }

    fn parse(&mut self, _context: &LightContext, _test_files: &[&Path]) -> Result<Vec<Span>> {
        Ok(Vec::new())
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
