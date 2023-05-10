use crate::{config, LightContext, Span};
use anyhow::Result;
use std::path::Path;
use subprocess::{Exec, Popen};

mod auto;
pub use auto::Auto;

mod empty;
pub use empty::Empty;

mod union;
pub use union::Union;

#[allow(dead_code)]
type AutoUnion<T, U> = Auto<Union<T, U>>;

pub trait Applicable {
    fn applicable(&self, context: &LightContext) -> Result<bool>;
}

pub trait ToImplementation {
    fn to_implementation(&self, context: &LightContext) -> Result<Option<Box<dyn Interface>>>;
}

pub trait Interface: Parse + Run {}

pub trait Parse {
    fn name(&self) -> String;
    fn parse(
        &mut self,
        context: &LightContext,
        config: &config::Toml,
        test_files: &[&Path],
    ) -> Result<Vec<Span>>;
}

pub type Postprocess = dyn Fn(&LightContext, Popen) -> Result<bool>;

pub trait Run {
    fn dry_run(&self, context: &LightContext, test_file: &Path) -> Result<()>;
    fn exec(
        &self,
        context: &LightContext,
        span: &Span,
    ) -> Result<Option<(Exec, Option<Box<Postprocess>>)>>;
}

pub trait AsParse {
    fn as_parse(&self) -> &dyn Parse;
    fn as_parse_mut(&mut self) -> &mut dyn Parse;
}

pub trait AsRun {
    fn as_run(&self) -> &dyn Run;
}

impl<T: AsParse> Parse for T {
    fn name(&self) -> String {
        self.as_parse().name()
    }
    #[cfg_attr(
        dylint_lib = "non_local_effect_before_error_return",
        allow(non_local_effect_before_error_return)
    )]
    fn parse(
        &mut self,
        context: &LightContext,
        config: &config::Toml,
        test_files: &[&Path],
    ) -> Result<Vec<Span>> {
        self.as_parse_mut().parse(context, config, test_files)
    }
}

impl<T: AsRun> Run for T {
    fn dry_run(&self, context: &LightContext, test_file: &Path) -> Result<()> {
        self.as_run().dry_run(context, test_file)
    }
    fn exec(
        &self,
        context: &LightContext,
        span: &Span,
    ) -> Result<Option<(Exec, Option<Box<Postprocess>>)>> {
        self.as_run().exec(context, span)
    }
}
