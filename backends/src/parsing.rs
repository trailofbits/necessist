use super::{GenericVisitor, ParseHigh};
use anyhow::{Context, Result};
use heck::ToKebabCase;
use necessist_core::{
    config,
    framework::{SourceFileSpanTestMap, SpanTestMaps},
    util, warn, LightContext, SourceFile, Span, WarnFlags, Warning,
};
use paste::paste;
use std::{any::type_name, cell::RefCell, convert::Infallible, path::Path, rc::Rc};

// smoelius: Some of the key data structures used during parsing:
//
// - `File`: framework-specific abstract abstract syntax tree representing a file
//
// - `Storage`: framework-specific "scratch space." `Storage` is allowed to hold references to parts
//   of the `File`. The lifetime of a `Storage` is only what it takes to parse the `File`. `Storage`
//   is wrapped in a `RefCell`.
//
// - framework: Rust, Hardhat, etc. Implements the `ParseLow` trait, i.e., contains callbacks such
//   `statement_is_call`, which are used by the `GenericVisitor` (below). Most callbacks are passed
//   a reference to the `Storage`.
//
// - `GenericVisitor`: contains callbacks such as `visit_statement`/`visit_statement_post`, which
//   are used by the framework-specific visitor (below). Holds a reference to the framework (among
//   other things).
//
// - framework-specific visitor: wraps a `GenericVisitor` and calls into it while traversing the
//   `File`. Holds a reference to the `Storage`, which it passes to the `GenericVisitor`, who then
//   forwards it on to the framework.

pub trait Named {
    fn name(&self) -> String;
}

impl Named for Infallible {
    fn name(&self) -> String {
        unreachable!()
    }
}

pub trait MaybeNamed {
    fn name(&self) -> Option<String>;
}

impl MaybeNamed for Infallible {
    fn name(&self) -> Option<String> {
        unreachable!()
    }
}

pub trait Spanned {
    fn span(&self, source_file: &SourceFile) -> Span;
}

impl Spanned for Infallible {
    fn span(&self, _source_file: &SourceFile) -> Span {
        unreachable!()
    }
}

// smoelius: When there is ambiguity, try to use names used by Rust/`syn`.
pub trait AbstractTypes {
    type Storage<'ast>;
    type File;
    type Test<'ast>: Copy + Named + 'ast;
    type Statement<'ast>: Copy + Eq + Spanned;
    // smoelius: `<Expression as MaybeNamed>::name` is allowed to return `None` when the expression
    // is one of the other known types, e.g., `Await`, `Call`, etc.
    type Expression<'ast>: Copy + MaybeNamed + Spanned;
    type Await<'ast>: Copy;
    type Field<'ast>: Copy + MaybeNamed + Spanned + 'ast;
    type Call<'ast>: Copy + MaybeNamed + Spanned + 'ast;
    type MacroCall<'ast>: Copy + Named + Spanned + 'ast;
}

pub type WalkDirResult = walkdir::Result<walkdir::DirEntry>;

pub trait ParseLow: Sized {
    type Types: AbstractTypes;

    const IGNORED_FUNCTIONS: Option<&'static [&'static str]>;
    const IGNORED_MACROS: Option<&'static [&'static str]>;
    const IGNORED_METHODS: Option<&'static [&'static str]>;

    fn name() -> String {
        #[allow(clippy::unwrap_used)]
        let (_, type_name) = type_name::<Self>().rsplit_once("::").unwrap();
        type_name.to_kebab_case()
    }
    fn walk_dir(&self, root: &Path) -> Box<dyn Iterator<Item = WalkDirResult>>;
    fn parse_source_file(&self, source_file: &Path)
        -> Result<<Self::Types as AbstractTypes>::File>;
    fn storage_from_file<'ast>(
        &self,
        file: &'ast <Self::Types as AbstractTypes>::File,
    ) -> <Self::Types as AbstractTypes>::Storage<'ast>;
    // smoelius: `visit_file` cannot have a `&self` argument because `generic_visitor` holds a
    // mutable reference to `self`.
    fn visit_file<'ast>(
        generic_visitor: GenericVisitor<'_, '_, '_, 'ast, Self>,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        file: &'ast <Self::Types as AbstractTypes>::File,
    ) -> Result<SpanTestMaps>;

    fn test_statements<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        test: <Self::Types as AbstractTypes>::Test<'ast>,
    ) -> Vec<<Self::Types as AbstractTypes>::Statement<'ast>>;

    fn statement_is_removable(
        &self,
        statement: <Self::Types as AbstractTypes>::Statement<'_>,
    ) -> bool;
    fn statement_is_expression<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        statement: <Self::Types as AbstractTypes>::Statement<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Expression<'ast>>;
    fn statement_is_control<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        statement: <Self::Types as AbstractTypes>::Statement<'ast>,
    ) -> bool;
    fn statement_is_declaration<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        statement: <Self::Types as AbstractTypes>::Statement<'ast>,
    ) -> bool;

    fn expression_is_await<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        expression: <Self::Types as AbstractTypes>::Expression<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Await<'ast>>;
    fn expression_is_field<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        expression: <Self::Types as AbstractTypes>::Expression<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Field<'ast>>;
    fn expression_is_call<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        expression: <Self::Types as AbstractTypes>::Expression<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Call<'ast>>;
    fn expression_is_macro_call<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        expression: <Self::Types as AbstractTypes>::Expression<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::MacroCall<'ast>>;

    fn await_arg<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        await_: <Self::Types as AbstractTypes>::Await<'ast>,
    ) -> <Self::Types as AbstractTypes>::Expression<'ast>;
    fn field_base<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        field: <Self::Types as AbstractTypes>::Field<'ast>,
    ) -> <Self::Types as AbstractTypes>::Expression<'ast>;
    fn call_callee<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        call: <Self::Types as AbstractTypes>::Call<'ast>,
    ) -> <Self::Types as AbstractTypes>::Expression<'ast>;
    fn macro_call_callee<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        macro_call: <Self::Types as AbstractTypes>::MacroCall<'ast>,
    ) -> <Self::Types as AbstractTypes>::Expression<'ast>;
}

impl<T: ParseLow> ParseLow for Rc<RefCell<T>> {
    type Types = T::Types;
    const IGNORED_FUNCTIONS: Option<&'static [&'static str]> = T::IGNORED_FUNCTIONS;
    const IGNORED_MACROS: Option<&'static [&'static str]> = T::IGNORED_MACROS;
    const IGNORED_METHODS: Option<&'static [&'static str]> = T::IGNORED_METHODS;
    fn walk_dir(&self, root: &Path) -> Box<dyn Iterator<Item = WalkDirResult>> {
        self.borrow().walk_dir(root)
    }
    fn parse_source_file(
        &self,
        source_file: &Path,
    ) -> Result<<Self::Types as AbstractTypes>::File> {
        self.borrow().parse_source_file(source_file)
    }
    fn storage_from_file<'ast>(
        &self,
        file: &'ast <Self::Types as AbstractTypes>::File,
    ) -> <Self::Types as AbstractTypes>::Storage<'ast> {
        self.borrow().storage_from_file(file)
    }
    fn visit_file<'ast>(
        generic_visitor: GenericVisitor<'_, '_, '_, 'ast, Self>,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        file: &'ast <Self::Types as AbstractTypes>::File,
    ) -> Result<SpanTestMaps> {
        let GenericVisitor {
            context,
            config,
            backend,
            source_file,
            test_name,
            last_statement_in_test,
            n_before,
            n_statement_leaves_visited,
            call_statement,
            span_test_maps,
        } = generic_visitor;
        let mut backend = backend.borrow_mut();
        let generic_visitor = GenericVisitor::<'_, '_, '_, 'ast, T> {
            context,
            config,
            backend: &mut backend,
            source_file,
            test_name,
            last_statement_in_test,
            n_before,
            n_statement_leaves_visited,
            call_statement,
            span_test_maps,
        };
        T::visit_file(generic_visitor, storage, file)
    }
    fn test_statements<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        test: <Self::Types as AbstractTypes>::Test<'ast>,
    ) -> Vec<<Self::Types as AbstractTypes>::Statement<'ast>> {
        self.borrow().test_statements(storage, test)
    }
    fn statement_is_removable(
        &self,
        statement: <Self::Types as AbstractTypes>::Statement<'_>,
    ) -> bool {
        self.borrow().statement_is_removable(statement)
    }
    fn statement_is_expression<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        statement: <Self::Types as AbstractTypes>::Statement<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Expression<'ast>> {
        self.borrow().statement_is_expression(storage, statement)
    }
    fn statement_is_control<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        statement: <Self::Types as AbstractTypes>::Statement<'ast>,
    ) -> bool {
        self.borrow().statement_is_control(storage, statement)
    }
    fn statement_is_declaration<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        statement: <Self::Types as AbstractTypes>::Statement<'ast>,
    ) -> bool {
        self.borrow().statement_is_declaration(storage, statement)
    }
    fn expression_is_await<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        expression: <Self::Types as AbstractTypes>::Expression<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Await<'ast>> {
        self.borrow().expression_is_await(storage, expression)
    }
    fn expression_is_field<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        expression: <Self::Types as AbstractTypes>::Expression<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Field<'ast>> {
        self.borrow().expression_is_field(storage, expression)
    }
    fn expression_is_call<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        expression: <Self::Types as AbstractTypes>::Expression<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Call<'ast>> {
        self.borrow().expression_is_call(storage, expression)
    }
    fn expression_is_macro_call<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        expression: <Self::Types as AbstractTypes>::Expression<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::MacroCall<'ast>> {
        self.borrow().expression_is_macro_call(storage, expression)
    }
    fn await_arg<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        await_: <Self::Types as AbstractTypes>::Await<'ast>,
    ) -> <Self::Types as AbstractTypes>::Expression<'ast> {
        self.borrow().await_arg(storage, await_)
    }
    fn field_base<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        field: <Self::Types as AbstractTypes>::Field<'ast>,
    ) -> <Self::Types as AbstractTypes>::Expression<'ast> {
        self.borrow().field_base(storage, field)
    }
    fn call_callee<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        call: <Self::Types as AbstractTypes>::Call<'ast>,
    ) -> <Self::Types as AbstractTypes>::Expression<'ast> {
        self.borrow().call_callee(storage, call)
    }
    fn macro_call_callee<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        macro_call: <Self::Types as AbstractTypes>::MacroCall<'ast>,
    ) -> <Self::Types as AbstractTypes>::Expression<'ast> {
        self.borrow().macro_call_callee(storage, macro_call)
    }
}

pub struct ParseAdapter<T>(pub T);

impl<T: ParseLow> ParseHigh for ParseAdapter<T> {
    fn parse(
        &mut self,
        context: &LightContext,
        config: &config::Toml,
        source_files: &[&Path],
    ) -> Result<SourceFileSpanTestMap> {
        let config = Self::compile_config(context, config)?;

        let mut source_file_span_test_map = SourceFileSpanTestMap::new();

        let walk_dir_results = self.0.walk_dir(context.root);

        let mut visit_source_file = |source_file: &Path| -> Result<()> {
            assert!(source_file.is_absolute());
            assert!(source_file.starts_with(context.root.as_path()));

            #[allow(clippy::unwrap_used)]
            let file = match self.0.parse_source_file(source_file) {
                Ok(file) => file,
                Err(error) => {
                    warn(
                        context,
                        Warning::ParsingFailed,
                        // smoelius: Use `{error}` rather than `{error:?}`. A backtrace seems
                        // unnecessary.
                        &format!(
                            r#"Failed to parse "{}": {error}"#,
                            util::strip_prefix(source_file, context.root)
                                .unwrap()
                                .display(),
                        ),
                        WarnFlags::empty(),
                    )?;
                    return Ok(());
                }
            };

            let storage = RefCell::new(self.0.storage_from_file(&file));

            let source_file = SourceFile::new(context.root.clone(), source_file.to_path_buf())?;

            let generic_visitor = GenericVisitor {
                context,
                config: &config,
                backend: &mut self.0,
                source_file: source_file.clone(),
                test_name: None,
                last_statement_in_test: None,
                n_statement_leaves_visited: 0,
                n_before: Vec::new(),
                call_statement: None,
                span_test_maps: SpanTestMaps::default(),
            };

            let span_test_map = T::visit_file(generic_visitor, &storage, &file)?;
            extend(&mut source_file_span_test_map, source_file, span_test_map);

            Ok(())
        };

        if source_files.is_empty() {
            for entry in walk_dir_results {
                let entry = entry
                    .with_context(|| format!(r#"Failed to walk "{}""#, context.root.display()))?;
                let path = entry.path();

                if !path.is_file() {
                    continue;
                }

                visit_source_file(path)?;
            }
        } else {
            for path in source_files {
                visit_source_file(path)?;
            }
        }

        Ok(source_file_span_test_map)
    }
}

fn extend(
    source_file_span_test_map: &mut SourceFileSpanTestMap,
    source_file: SourceFile,
    span_test_maps_incoming: SpanTestMaps,
) {
    let span_test_maps = source_file_span_test_map.entry(source_file).or_default();
    for (test_name, spans_incoming) in span_test_maps_incoming.statement {
        let spans = span_test_maps.statement.entry(test_name).or_default();
        spans.extend(spans_incoming);
    }
    for (test_name, spans_incoming) in span_test_maps_incoming.method_call {
        let spans = span_test_maps.method_call.entry(test_name).or_default();
        spans.extend(spans_incoming);
    }
}

macro_rules! check_config {
    ($T:ty, $storage:expr, $config:expr, $name:expr, $x:ident) => {
        paste! {
            let unsupported = $T::[< IGNORED_ $x:snake:upper S>].is_none();
            let used = !$config.[< ignored_ $x:snake s >].is_empty();
            if unsupported && used {
                warn(
                    $storage,
                    Warning::[< Ignored $x:camel s Unsupported >],
                    &format!(
                        "The {} framework does not support the `{}` configuration",
                        $name,
                        stringify!([< ignored_ $x:snake s >]),
                    ),
                    WarnFlags::ONCE,
                )?;
            }
        }
    };
}

impl<T: ParseLow> ParseAdapter<T> {
    fn compile_config(context: &LightContext, config: &config::Toml) -> Result<config::Compiled> {
        let name = T::name();

        check_config!(T, context, config, name, function);
        check_config!(T, context, config, name, macro);
        check_config!(T, context, config, name, method);

        let ignored_functions = T::IGNORED_FUNCTIONS
            .unwrap_or_default()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let ignored_macros = T::IGNORED_MACROS
            .unwrap_or_default()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let ignored_methods = T::IGNORED_METHODS
            .unwrap_or_default()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();

        let mut builtins = config::Toml {
            ignored_functions,
            ignored_macros,
            ignored_methods,
            ..Default::default()
        };

        builtins.merge(config).unwrap();

        builtins.compile()
    }
}
