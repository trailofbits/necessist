use crate::{config, rewriter::Rewriter, LightContext, SourceFile, Span};
use anyhow::Result;
use indexmap::IndexSet;
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

pub type TestSet = BTreeSet<String>;

pub type SourceFileSpanTestMap = BTreeMap<SourceFile, SpanTestMaps>;

#[derive(Default)]
pub struct SpanTestMaps {
    pub statement: SpanTestMap,
    pub method_call: SpanTestMap,
}

impl SpanTestMaps {
    pub fn iter(&self) -> impl Iterator<Item = (&Span, SpanKind, &IndexSet<String>)> {
        self.statement
            .iter()
            .map(|(span, test_names)| (span, SpanKind::Statement, test_names))
            .chain(
                self.method_call
                    .iter()
                    .map(|(span, test_names)| (span, SpanKind::MethodCall, test_names)),
            )
    }
}

/// Maps a [`Span`] to the names of the tests that exercise it
///
/// The test names are needed because they are passed to [`Run::exec`].
pub type SpanTestMap = BTreeMap<Span, IndexSet<String>>;

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
        source_files: &[&Path],
    ) -> Result<(usize, SourceFileSpanTestMap)>;
}

pub type Postprocess = dyn Fn(&LightContext, Popen) -> Result<bool>;

pub trait Run {
    fn dry_run(&self, context: &LightContext, source_file: &Path) -> Result<()>;
    fn instrument_source_file(
        &self,
        context: &LightContext,
        rewriter: &mut Rewriter,
        source_file: &SourceFile,
        n_instrumentable_statements: usize,
    ) -> Result<()>;
    fn statement_prefix_and_suffix(&self, span: &Span) -> Result<(String, String)>;
    fn build_source_file(&self, context: &LightContext, source_file: &Path) -> Result<()>;
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
        source_files: &[&Path],
    ) -> Result<(usize, SourceFileSpanTestMap)> {
        self.as_parse_mut().parse(context, config, source_files)
    }
}

impl<T: AsRun> Run for T {
    fn dry_run(&self, context: &LightContext, source_file: &Path) -> Result<()> {
        self.as_run().dry_run(context, source_file)
    }
    fn instrument_source_file(
        &self,
        context: &LightContext,
        rewriter: &mut Rewriter,
        source_file: &SourceFile,
        n_instrumentable_statements: usize,
    ) -> Result<()> {
        self.as_run().instrument_source_file(
            context,
            rewriter,
            source_file,
            n_instrumentable_statements,
        )
    }
    fn statement_prefix_and_suffix(&self, span: &Span) -> Result<(String, String)> {
        self.as_run().statement_prefix_and_suffix(span)
    }
    fn build_source_file(&self, context: &LightContext, source_file: &Path) -> Result<()> {
        self.as_run().build_source_file(context, source_file)
    }
    /// Execute test `test_name` with `span` removed. Returns `Ok(None)` if the test could not be
    /// built.
    ///
    /// In most cases, just `span.source_file` is used. But in the implementation of `RunLow::exec`,
    /// `span` is used for error reporting.
    fn exec(
        &self,
        context: &LightContext,
        test_name: &str,
        span: &Span,
    ) -> Result<Option<(Exec, Option<Box<Postprocess>>)>> {
        self.as_run().exec(context, test_name, span)
    }
}
