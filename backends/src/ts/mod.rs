use crate::{
    generic_visitor::GenericVisitor,
    parsing::{AbstractTypes, ParseLow, WalkDirResult},
};
use anyhow::Result;
use necessist_core::{
    LightContext, Span,
    framework::{Postprocess, SpanTestMaps, TestSet},
};
use std::{cell::RefCell, collections::BTreeMap, path::Path, process::Command};
use subprocess::Exec;

mod inner;
pub use inner::Inner;

pub mod utils;

mod mocha;
pub use mocha::Mocha;

mod vitest;
pub use vitest::{VITEST_COMMAND_SUFFIX, Vitest};

pub trait MochaLike {
    fn as_inner(&self) -> &Inner;

    fn as_inner_mut(&mut self) -> &mut Inner;

    fn dry_run(&self, context: &LightContext, source_file: &Path, command: Command) -> Result<()>;

    fn statement_prefix_and_suffix(&self, span: &Span) -> Result<(String, String)>;

    fn exec(
        &self,
        context: &LightContext,
        test_name: &str,
        span: &Span,
        command: &Command,
    ) -> Result<Option<(Exec, Option<Box<Postprocess>>)>>;
}

impl ParseLow for Box<dyn MochaLike> {
    type Types = <Inner as ParseLow>::Types;
    const IGNORED_FUNCTIONS: Option<&'static [&'static str]> = Inner::IGNORED_FUNCTIONS;
    const IGNORED_MACROS: Option<&'static [&'static str]> = Inner::IGNORED_MACROS;
    const IGNORED_METHODS: Option<&'static [&'static str]> = Inner::IGNORED_METHODS;
    fn walk_dir(&self, root: &Path) -> Box<dyn Iterator<Item = WalkDirResult>> {
        self.as_inner().walk_dir(root)
    }
    fn parse_source_file(
        &self,
        source_file: &Path,
    ) -> Result<<Self::Types as AbstractTypes>::File> {
        self.as_inner().parse_source_file(source_file)
    }
    fn storage_from_file<'ast>(
        &self,
        file: &'ast <Self::Types as AbstractTypes>::File,
    ) -> <Self::Types as AbstractTypes>::Storage<'ast> {
        self.as_inner().storage_from_file(file)
    }
    fn local_functions<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        file: &'ast <Self::Types as AbstractTypes>::File,
    ) -> Result<BTreeMap<String, Vec<<Self::Types as AbstractTypes>::LocalFunction<'ast>>>> {
        self.as_inner().local_functions(storage, file)
    }
    fn visit_file<'ast>(
        generic_visitor: GenericVisitor<'_, '_, '_, 'ast, Self>,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        file: &'ast <Self::Types as AbstractTypes>::File,
    ) -> Result<(TestSet, SpanTestMaps)> {
        let GenericVisitor {
            context,
            config,
            backend,
            walkable_functions,
            source_file,
            test_names,
            last_statement_in_test,
            n_statement_leaves_visited,
            n_before,
            call_statement,
            test_set,
            span_test_maps,
            local_functions_pending,
            local_functions_returned,
            local_functions_needing_warnings,
        } = generic_visitor;
        let backend = backend.as_inner_mut();
        let generic_visitor = GenericVisitor::<'_, '_, '_, 'ast, _> {
            context,
            config,
            backend,
            walkable_functions,
            source_file,
            test_names,
            last_statement_in_test,
            n_statement_leaves_visited,
            n_before,
            call_statement,
            test_set,
            span_test_maps,
            local_functions_pending,
            local_functions_returned,
            local_functions_needing_warnings,
        };
        <Inner as ParseLow>::visit_file(generic_visitor, storage, file)
    }
    fn test_statements<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        test: <Self::Types as AbstractTypes>::Test<'ast>,
    ) -> Vec<<Self::Types as AbstractTypes>::Statement<'ast>> {
        self.as_inner().test_statements(storage, test)
    }
    fn statement_is_removable(
        &self,
        statement: <Self::Types as AbstractTypes>::Statement<'_>,
    ) -> bool {
        self.as_inner().statement_is_removable(statement)
    }
    fn statement_is_expression<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        statement: <Self::Types as AbstractTypes>::Statement<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Expression<'ast>> {
        self.as_inner().statement_is_expression(storage, statement)
    }
    fn statement_is_control<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        statement: <Self::Types as AbstractTypes>::Statement<'ast>,
    ) -> bool {
        self.as_inner().statement_is_control(storage, statement)
    }
    fn statement_is_declaration<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        statement: <Self::Types as AbstractTypes>::Statement<'ast>,
    ) -> bool {
        self.as_inner().statement_is_declaration(storage, statement)
    }
    fn expression_is_await<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        expression: <Self::Types as AbstractTypes>::Expression<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Await<'ast>> {
        self.as_inner().expression_is_await(storage, expression)
    }
    fn expression_is_field<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        expression: <Self::Types as AbstractTypes>::Expression<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Field<'ast>> {
        self.as_inner().expression_is_field(storage, expression)
    }
    fn expression_is_call<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        expression: <Self::Types as AbstractTypes>::Expression<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Call<'ast>> {
        self.as_inner().expression_is_call(storage, expression)
    }
    fn expression_is_macro_call<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        expression: <Self::Types as AbstractTypes>::Expression<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::MacroCall<'ast>> {
        self.as_inner()
            .expression_is_macro_call(storage, expression)
    }
    fn await_arg<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        await_: <Self::Types as AbstractTypes>::Await<'ast>,
    ) -> <Self::Types as AbstractTypes>::Expression<'ast> {
        self.as_inner().await_arg(storage, await_)
    }
    fn field_base<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        field: <Self::Types as AbstractTypes>::Field<'ast>,
    ) -> <Self::Types as AbstractTypes>::Expression<'ast> {
        self.as_inner().field_base(storage, field)
    }
    fn call_callee<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        call: <Self::Types as AbstractTypes>::Call<'ast>,
    ) -> <Self::Types as AbstractTypes>::Expression<'ast> {
        self.as_inner().call_callee(storage, call)
    }
    fn macro_call_callee<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        macro_call: <Self::Types as AbstractTypes>::MacroCall<'ast>,
    ) -> <Self::Types as AbstractTypes>::Expression<'ast> {
        self.as_inner().macro_call_callee(storage, macro_call)
    }
}
