use crate::{config, rewriter::Rewriter, LightContext, SourceFile, Span};
use anyhow::Result;
use indexmap::IndexMap;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
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

pub type TestFileTestSpanMap = BTreeMap<SourceFile, TestSpanMaps>;

// smoelius: The `statement` and `method_call` maps are expected to have the same sets of keys. This
// is a little ugly, but it simplifies the implementation of `necessist_core`.
#[derive(Default)]
pub struct TestSpanMaps {
    pub statement: TestSpanMap,
    pub method_call: TestSpanMap,
}

impl TestSpanMaps {
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Span, SpanKind)> {
        self.statement
            .iter()
            .flat_map(|(test_name, spans)| {
                spans
                    .iter()
                    .map(|span| (test_name.as_str(), span, SpanKind::Statement))
            })
            .chain(self.method_call.iter().flat_map(|(test_name, spans)| {
                spans
                    .iter()
                    .map(|span| (test_name.as_str(), span, SpanKind::MethodCall))
            }))
    }
}

pub type TestSpanMap = IndexMap<String, BTreeSet<Span>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpanKind {
    Statement,
    MethodCall,
}

pub trait Parse {
    fn parse(
        &mut self,
        context: &LightContext,
        config: &config::Toml,
        test_files: &[&Path],
    ) -> Result<TestFileTestSpanMap>;
}

pub type Postprocess = dyn Fn(&LightContext, Popen) -> Result<bool>;

pub trait Run {
    fn dry_run(&self, context: &LightContext, test_file: &Path) -> Result<()>;
    fn instrument_file(
        &self,
        context: &LightContext,
        rewriter: &mut Rewriter,
        source_file: &SourceFile,
        n_instrumentable_statements: usize,
    ) -> Result<()>;
    fn statement_prefix_and_suffix(&self, span: &Span) -> Result<(String, String)>;
    fn build_file(&self, context: &LightContext, source_file: &Path) -> Result<()>;
    fn exec(
        &self,
        context: &LightContext,
        test_name: &str,
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
    #[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
    fn parse(
        &mut self,
        context: &LightContext,
        config: &config::Toml,
        test_files: &[&Path],
    ) -> Result<TestFileTestSpanMap> {
        self.as_parse_mut().parse(context, config, test_files)
    }
}

impl<T: AsRun> Run for T {
    fn dry_run(&self, context: &LightContext, test_file: &Path) -> Result<()> {
        self.as_run().dry_run(context, test_file)
    }
    fn instrument_file(
        &self,
        context: &LightContext,
        rewriter: &mut Rewriter,
        source_file: &SourceFile,
        n_instrumentable_statements: usize,
    ) -> Result<()> {
        self.as_run()
            .instrument_file(context, rewriter, source_file, n_instrumentable_statements)
    }
    fn statement_prefix_and_suffix(&self, span: &Span) -> Result<(String, String)> {
        self.as_run().statement_prefix_and_suffix(span)
    }
    fn build_file(&self, context: &LightContext, source_file: &Path) -> Result<()> {
        self.as_run().build_file(context, source_file)
    }
    fn exec(
        &self,
        context: &LightContext,
        test_name: &str,
        span: &Span,
    ) -> Result<Option<(Exec, Option<Box<Postprocess>>)>> {
        self.as_run().exec(context, test_name, span)
    }
}
