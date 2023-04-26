use super::{GenericVisitor, ParseHigh};
use anyhow::{Context, Result};
use heck::ToKebabCase;
use necessist_core::{util, warn, Config, LightContext, SourceFile, Span, WarnFlags, Warning};
use paste::paste;
use std::{any::type_name, cell::RefCell, path::Path, rc::Rc};

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

pub trait MaybeNamed {
    fn name(&self) -> Option<String>;
}

pub trait Spanned {
    fn span(&self, source_file: &SourceFile) -> Span;
}

pub trait AbstractTypes {
    type Storage<'ast>;
    type File;
    type Test<'ast>: Copy + Named + 'ast;
    type Statement<'ast>: Copy + Eq + Spanned + 'ast;
    type FieldAccess<'ast>: Copy + MaybeNamed + 'ast;
    type FunctionCall<'ast>: Copy + MaybeNamed + Spanned + 'ast;
    type MacroCall<'ast>: Copy + MaybeNamed + Spanned + 'ast;
    // smoelius: The span returned `<MethodCall as Spanned>::span` should exclude the receiver,
    // i.e., include only the dot through the arguments closing paren.
    type MethodCall<'ast>: Copy + MaybeNamed + Spanned + 'ast;
}

pub type WalkDirResult = walkdir::Result<walkdir::DirEntry>;

pub trait ParseLow: Sized {
    type Types: AbstractTypes;

    const IGNORED_FUNCTIONS: &'static [&'static str];
    const IGNORED_MACROS: &'static [&'static str];
    const IGNORED_METHODS: &'static [&'static str];

    fn name() -> String {
        #[allow(clippy::unwrap_used)]
        let (_, type_name) = type_name::<Self>().rsplit_once("::").unwrap();
        type_name.to_kebab_case()
    }
    fn walk_dir(root: &Path) -> Box<dyn Iterator<Item = WalkDirResult>>;
    fn parse_file(&self, test_file: &Path) -> Result<<Self::Types as AbstractTypes>::File>;
    fn storage_from_file(
        file: &<Self::Types as AbstractTypes>::File,
    ) -> <Self::Types as AbstractTypes>::Storage<'_>;
    fn visit_file<'ast>(
        generic_visitor: GenericVisitor<'_, '_, '_, 'ast, Self>,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        file: &'ast <Self::Types as AbstractTypes>::File,
    ) -> Result<Vec<Span>>;
    fn on_candidate_found(
        &mut self,
        context: &LightContext,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'_>>,
        test_name: &str,
        span: &Span,
    );

    fn test_statements<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        test: <Self::Types as AbstractTypes>::Test<'ast>,
    ) -> Vec<<Self::Types as AbstractTypes>::Statement<'ast>>;

    fn statement_is_call<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        statement: <Self::Types as AbstractTypes>::Statement<'ast>,
    ) -> bool;
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

    fn function_call_is_statement<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        function_call: <Self::Types as AbstractTypes>::FunctionCall<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Statement<'ast>>;
    fn macro_call_is_statement<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        macro_call: <Self::Types as AbstractTypes>::MacroCall<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Statement<'ast>>;
    fn method_call_is_statement<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        method_call: <Self::Types as AbstractTypes>::MethodCall<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Statement<'ast>>;

    fn field_access_has_inner_field_access<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        field_access: <Self::Types as AbstractTypes>::FieldAccess<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::FieldAccess<'ast>>;
    fn function_call_has_inner_field_access<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        function_call: <Self::Types as AbstractTypes>::FunctionCall<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::FieldAccess<'ast>>;
    fn method_call_has_inner_field_access<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        method_call: <Self::Types as AbstractTypes>::MethodCall<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::FieldAccess<'ast>>;

    fn function_call_is_method_call_receiver<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        function_call: <Self::Types as AbstractTypes>::FunctionCall<'ast>,
    ) -> bool;
    fn macro_call_is_method_call_receiver<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        macro_call: <Self::Types as AbstractTypes>::MacroCall<'ast>,
    ) -> bool;
}

impl<T: ParseLow> ParseLow for Rc<RefCell<T>> {
    type Types = T::Types;
    const IGNORED_FUNCTIONS: &'static [&'static str] = T::IGNORED_FUNCTIONS;
    const IGNORED_MACROS: &'static [&'static str] = T::IGNORED_MACROS;
    const IGNORED_METHODS: &'static [&'static str] = T::IGNORED_METHODS;
    fn walk_dir(root: &Path) -> Box<dyn Iterator<Item = WalkDirResult>> {
        T::walk_dir(root)
    }
    fn parse_file(&self, test_file: &Path) -> Result<<Self::Types as AbstractTypes>::File> {
        self.borrow().parse_file(test_file)
    }
    fn storage_from_file(
        file: &<Self::Types as AbstractTypes>::File,
    ) -> <Self::Types as AbstractTypes>::Storage<'_> {
        T::storage_from_file(file)
    }
    fn visit_file<'ast, 'storage>(
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
    ) {
        self.borrow_mut()
            .on_candidate_found(context, storage, test_name, span);
    }
    fn test_statements<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        test: <Self::Types as AbstractTypes>::Test<'ast>,
    ) -> Vec<<Self::Types as AbstractTypes>::Statement<'ast>> {
        self.borrow().test_statements(storage, test)
    }
    fn statement_is_call<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        statement: <Self::Types as AbstractTypes>::Statement<'ast>,
    ) -> bool {
        self.borrow().statement_is_call(storage, statement)
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
    fn function_call_is_statement<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        function_call: <Self::Types as AbstractTypes>::FunctionCall<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Statement<'ast>> {
        self.borrow()
            .function_call_is_statement(storage, function_call)
    }
    fn macro_call_is_statement<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        macro_call: <Self::Types as AbstractTypes>::MacroCall<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Statement<'ast>> {
        self.borrow().macro_call_is_statement(storage, macro_call)
    }
    fn method_call_is_statement<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        method_call: <Self::Types as AbstractTypes>::MethodCall<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Statement<'ast>> {
        self.borrow().method_call_is_statement(storage, method_call)
    }
    fn field_access_has_inner_field_access<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        field_access: <Self::Types as AbstractTypes>::FieldAccess<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::FieldAccess<'ast>> {
        self.borrow()
            .field_access_has_inner_field_access(storage, field_access)
    }
    fn function_call_has_inner_field_access<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        function_call: <Self::Types as AbstractTypes>::FunctionCall<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::FieldAccess<'ast>> {
        self.borrow()
            .function_call_has_inner_field_access(storage, function_call)
    }
    fn method_call_has_inner_field_access<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        function_call: <Self::Types as AbstractTypes>::MethodCall<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::FieldAccess<'ast>> {
        self.borrow()
            .method_call_has_inner_field_access(storage, function_call)
    }
    fn function_call_is_method_call_receiver<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        function_call: <Self::Types as AbstractTypes>::FunctionCall<'ast>,
    ) -> bool {
        self.borrow()
            .function_call_is_method_call_receiver(storage, function_call)
    }
    fn macro_call_is_method_call_receiver<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        macro_call: <Self::Types as AbstractTypes>::MacroCall<'ast>,
    ) -> bool {
        self.borrow()
            .macro_call_is_method_call_receiver(storage, macro_call)
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
        config: &Config,
        test_files: &[&Path],
    ) -> Result<Vec<Span>> {
        Self::check_config(context, config)?;

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

            let generic_visitor = GenericVisitor {
                context,
                config,
                framework: &mut self.0,
                source_file: SourceFile::new(
                    context.root.clone(),
                    Rc::new(test_file.to_path_buf()),
                ),
                test_name: None,
                last_statement_in_test: None,
                n_statement_leaves_visited: 0,
                n_before: Vec::new(),
                spans_visited: Vec::new(),
            };

            let storage = RefCell::new(T::storage_from_file(&file));

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
    ($Types:ty, $storage:expr, $config:expr, $name:expr, $x:ident) => {
        paste! {
            let unsupported = type_name::<<$Types as AbstractTypes>::[< $x:camel Call >]<'_>>()
                == type_name::<std::convert::Infallible>();
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
    fn check_config(context: &LightContext, config: &Config) -> Result<()> {
        let name = T::name();

        check_config!(T::Types, context, config, name, function);
        check_config!(T::Types, context, config, name, macro);
        check_config!(T::Types, context, config, name, method);

        Ok(())
    }
}
