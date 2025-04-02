//! Backend parsing support
//!
//! Some of the key data structures used during parsing:
//!
//! - `File`: framework-specific abstract abstract syntax tree representing a file
//!
//! - `Storage`: framework-specific "scratch space." `Storage` is allowed to hold references to
//!   parts of the `File`. The lifetime of a `Storage` is only what it takes to parse the `File`.
//!   `Storage` is wrapped in a [`RefCell`].
//!
//! - framework: Rust, Hardhat, etc. Implements the [`ParseLow`] trait, i.e., contains callbacks
//!   such `statement_is_call`, which are used by the [`GenericVisitor`] (below). Most callbacks are
//!   passed a reference to the `Storage`.
//!
//! - [`GenericVisitor`]: contains callbacks such as `visit_statement`/`visit_statement_post`, which
//!   are used by the framework-specific visitor (below). Holds a reference to the framework (among
//!   other things).
//!
//! - framework-specific visitor: wraps a [`GenericVisitor`] and calls into it while traversing the
//!   `File`. Holds a reference to the `Storage`, which it passes to the [`GenericVisitor`], who
//!   then forwards it on to the framework.

use super::{GenericVisitor, ParseHigh};
use anyhow::{Context, Result};
use heck::ToKebabCase;
use indexmap::IndexMap;
use necessist_core::{
    LightContext, SourceFile, Span, WarnFlags, Warning, config,
    framework::{SourceFileSpanTestMap, SpanTestMaps, TestSet},
    util, warn,
};
use paste::paste;
use std::{
    any::type_name,
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, HashSet},
    convert::Infallible,
    hash::Hash,
    path::Path,
    rc::Rc,
};
use std::rc::Rc as Lrc;

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
    type LocalFunction<'ast>: Copy + Eq + Hash;
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
    // smoelius: A `local_functions` value can contain more than one `LocalFunction` when the one
    // that should be used cannot be determined. In such cases, the `GenericVisitor` will use the
    // first one and emit a warning.
    fn local_functions<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        file: &'ast <Self::Types as AbstractTypes>::File,
    ) -> Result<BTreeMap<String, Vec<<Self::Types as AbstractTypes>::LocalFunction<'ast>>>>;

    // smoelius: `visit_file` cannot have a `&self` argument because `generic_visitor` holds a
    // mutable reference to `self`.
    fn visit_file<'ast>(
        generic_visitor: GenericVisitor<'_, '_, '_, 'ast, Self>,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        file: &'ast <Self::Types as AbstractTypes>::File,
    ) -> Result<(TestSet, SpanTestMaps)>;

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
    /// Returns a [`BTreeMap`] mapping local function names to `LocalFunction`s as defined in the
    /// backend's [`AbstractTypes`]
    fn local_functions<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        file: &'ast <Self::Types as AbstractTypes>::File,
    ) -> Result<BTreeMap<String, Vec<<Self::Types as AbstractTypes>::LocalFunction<'ast>>>> {
        self.borrow().local_functions(storage, file)
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
        let mut backend = backend.borrow_mut();
        let generic_visitor = GenericVisitor::<'_, '_, '_, 'ast, T> {
            context,
            config,
            backend: &mut backend,
            walkable_functions,
            source_file,
            test_names,
            last_statement_in_test,
            n_before,
            n_statement_leaves_visited,
            call_statement,
            test_set,
            span_test_maps,
            local_functions_pending,
            local_functions_returned,
            local_functions_needing_warnings,
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
    ) -> Result<(usize, SourceFileSpanTestMap)> {
        let config = Self::compile_config(context, config)?;

        let mut n_tests = 0;
        let mut source_file_span_test_map = SourceFileSpanTestMap::new();

        let visit_source_file = |source_file: &Path, backend: &mut T, n_tests: &mut usize, source_file_span_test_map: &mut SourceFileSpanTestMap, context: &LightContext, config: &config::Compiled| -> Result<()> {
            assert!(source_file.is_absolute());
            assert!(source_file.starts_with(context.root.as_path()));

            #[allow(clippy::unwrap_used)]
            let file = match backend.parse_source_file(source_file) {
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

            let storage = RefCell::new(backend.storage_from_file(&file));

            let walkable_functions = {
                let mut local_functions = backend.local_functions(&storage, &file)?;
                local_functions.retain(|name, _| config.is_walkable_function(name));
                local_functions
            };

            let source_file = SourceFile::new(context.root.clone(), source_file.to_path_buf())?;

            let generic_visitor = GenericVisitor {
                context,
                config,
                backend,
                walkable_functions,
                source_file: source_file.clone(),
                test_names: BTreeSet::default(),
                last_statement_in_test: None,
                n_statement_leaves_visited: 0,
                n_before: Vec::new(),
                call_statement: None,
                test_set: TestSet::default(),
                span_test_maps: SpanTestMaps::default(),
                local_functions_pending: IndexMap::default(),
                local_functions_returned: HashSet::default(),
                local_functions_needing_warnings: BTreeSet::default(),
            };

            let (test_set, span_test_map) = T::visit_file(generic_visitor, &storage, &file)?;

            *n_tests += test_set.len();
            extend(source_file_span_test_map, source_file, span_test_map);

            Ok(())
        };

        let mut queue = Vec::new();
        if source_files.is_empty() {
            queue.push(context.root.clone());
        } else {
            queue.extend(source_files.iter().map(|p| Lrc::new(p.to_path_buf())));
        }

        while let Some(path) = queue.pop() {
            if path.is_file() {
                visit_source_file(&path, &mut self.0, &mut n_tests, &mut source_file_span_test_map, context, &config)?;
            } else if path.is_dir() {
                let walk_dir_results = self.0.walk_dir(&path);
                for entry in walk_dir_results {
                    let entry = entry.with_context(|| format!(r#"Failed to walk "{}""#, path.display()))?;
                    queue.push(Lrc::new(entry.path().to_path_buf()));
                }
            }
        }

        Ok((n_tests, source_file_span_test_map))
    }
}

fn extend(
    source_file_span_test_map: &mut SourceFileSpanTestMap,
    source_file: SourceFile,
    span_test_maps_incoming: SpanTestMaps,
) {
    let span_test_maps = source_file_span_test_map.entry(source_file).or_default();
    for (span, test_names_incoming) in span_test_maps_incoming.statement {
        let test_names = span_test_maps.statement.entry(span).or_default();
        test_names.extend(test_names_incoming);
    }
    for (span, test_names_incoming) in span_test_maps_incoming.method_call {
        let test_names = span_test_maps.method_call.entry(span).or_default();
        test_names.extend(test_names_incoming);
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
