use super::{GenericVisitor, ParseHigh};
use anyhow::{Context, Result};
use heck::ToKebabCase;
use necessist_core::{config, util, warn, LightContext, SourceFile, Span, WarnFlags, Warning};
use paste::paste;
use std::{any::type_name, cell::RefCell, convert::Infallible, path::Path, rc::Rc};

// smoelius: Some of the key data structures used during parsing:
//
// - `File`: framework-specific abstract abstract syntax tree representing a file
//
// - `Storage`: framework-specific "scratch space." `Storage` is allowed to hold references to parts
//   of the `File`. `Storage` is wrapped in a `RefCell`.
//
// - framework: Rust, HardhatTs, etc. Implements the `ParseLow` trait, i.e., contains callbacks such
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
    fn walk_dir(root: &Path) -> Box<dyn Iterator<Item = WalkDirResult>>;
    fn parse_file(&self, test_file: &Path) -> Result<<Self::Types as AbstractTypes>::File>;
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
    ) -> Result<Vec<Span>>;
    #[must_use]
    fn on_candidate_found(
        &mut self,
        context: &LightContext,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'_>>,
        test_name: &str,
        span: &Span,
    ) -> bool;

    fn test_statements<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        test: <Self::Types as AbstractTypes>::Test<'ast>,
    ) -> Vec<<Self::Types as AbstractTypes>::Statement<'ast>>;

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
    fn walk_dir(root: &Path) -> Box<dyn Iterator<Item = WalkDirResult>> {
        T::walk_dir(root)
    }
    fn parse_file(&self, test_file: &Path) -> Result<<Self::Types as AbstractTypes>::File> {
        self.borrow().parse_file(test_file)
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
    ) -> Result<Vec<Span>> {
        let GenericVisitor {
            context,
            config,
            framework,
            source_file,
            test_name,
            last_statement_in_test,
            n_before,
            n_statement_leaves_visited,
            call_statement,
            spans_visited,
        } = generic_visitor;
        let mut framework = framework.borrow_mut();
        let generic_visitor = GenericVisitor::<'_, '_, '_, 'ast, T> {
            context,
            config,
            framework: &mut framework,
            source_file,
            test_name,
            last_statement_in_test,
            n_before,
            n_statement_leaves_visited,
            call_statement,
            spans_visited,
        };
        T::visit_file(generic_visitor, storage, file)
    }
    fn on_candidate_found(
        &mut self,
        context: &LightContext,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'_>>,
        test_name: &str,
        span: &Span,
    ) -> bool {
        self.borrow_mut()
            .on_candidate_found(context, storage, test_name, span)
    }
    fn test_statements<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        test: <Self::Types as AbstractTypes>::Test<'ast>,
    ) -> Vec<<Self::Types as AbstractTypes>::Statement<'ast>> {
        self.borrow().test_statements(storage, test)
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
    fn name(&self) -> String {
        T::name()
    }
    fn parse(
        &mut self,
        context: &LightContext,
        config: &config::Toml,
        test_files: &[&Path],
    ) -> Result<Vec<Span>> {
        let config = Self::compile_config(context, config)?;

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

            let storage = RefCell::new(self.0.storage_from_file(&file));

            let source_file = SourceFile::new(context.root.clone(), test_file.to_path_buf())?;

            let generic_visitor = GenericVisitor {
                context,
                config: &config,
                framework: &mut self.0,
                source_file,
                test_name: None,
                last_statement_in_test: None,
                n_statement_leaves_visited: 0,
                n_before: Vec::new(),
                call_statement: None,
                spans_visited: Vec::new(),
            };

            let spans_visited = T::visit_file(generic_visitor, &storage, &file)?;
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

        builtins.merge(config).unwrap().compile()
    }
}
