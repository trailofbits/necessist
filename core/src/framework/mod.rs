use crate::{Config, LightContext, Span};
use anyhow::Result;
use heck::ToKebabCase;
use std::{any::type_name, path::Path};
use subprocess::{Exec, Popen};

mod hardhat_ts;
mod rust;

type Postprocess = dyn Fn(&LightContext, Popen) -> Result<bool>;

pub(crate) trait Interface: std::fmt::Debug {
    #[allow(clippy::unwrap_used)]
    fn name(&self) -> String {
        let (_, type_name) = type_name::<Self>().rsplit_once("::").unwrap();
        type_name.to_kebab_case()
    }
    fn applicable(&self, context: &LightContext) -> Result<bool>;
    fn parse(
        &mut self,
        context: &LightContext,
        config: &Config,
        test_files: &[&Path],
    ) -> Result<Vec<Span>>;
    fn dry_run(&self, context: &LightContext, test_file: &Path) -> Result<()>;
    fn exec(
        &self,
        context: &LightContext,
        span: &Span,
    ) -> Result<Option<(Exec, Option<Box<Postprocess>>)>>;
}

pub(crate) fn frameworks() -> Vec<Box<dyn Interface>> {
    vec![
        Box::new(hardhat_ts::HardhatTs::default()),
        Box::new(rust::Rust::default()),
    ]
}
