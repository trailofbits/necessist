use super::{
    AbstractTypes, GenericVisitor, MaybeNamed, Named, ParseLow, ProcessLines, RunLow, Spanned,
    WalkDirResult,
};
use anyhow::Result;
use cargo_metadata::Package;
use necessist_core::{warn, LightContext, SourceFile, Span, ToInternalSpan, WarnFlags, Warning};
use quote::ToTokens;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    ffi::OsStr,
    fs::read_to_string,
    path::{Path, PathBuf, StripPrefixError},
    process::Command,
};

mod storage;
use storage::{cached_test_file_package, Storage};

mod try_insert;
use try_insert::TryInsert;

mod visitor;
use visitor::visit;

#[derive(Debug)]
pub struct Rust {
    test_file_flags_cache: BTreeMap<PathBuf, Vec<String>>,
    span_test_path_map: BTreeMap<Span, Vec<String>>,
}

impl Rust {
    pub fn applicable(context: &LightContext) -> Result<bool> {
        context
            .root
            .join("Cargo.toml")
            .try_exists()
            .map_err(Into::into)
    }

    pub fn new() -> Self {
        Self {
            test_file_flags_cache: BTreeMap::new(),
            span_test_path_map: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Copy)]
pub enum MacroCall<'ast> {
    Stmt(&'ast syn::StmtMacro),
    Expr(&'ast syn::ExprMacro),
}

impl<'ast> MacroCall<'ast> {
    fn path(&self) -> &syn::Path {
        match self {
            MacroCall::Stmt(syn::StmtMacro { mac, .. })
            | MacroCall::Expr(syn::ExprMacro { mac, .. }) => &mac.path,
        }
    }
}

pub struct Types;

impl AbstractTypes for Types {
    type Storage<'ast> = Storage<'ast>;
    type File = syn::File;
    type Test<'ast> = &'ast syn::ItemFn;
    type Statement<'ast> = &'ast syn::Stmt;
    type FieldAccess<'ast> = &'ast syn::ExprField;
    type FunctionCall<'ast> = &'ast syn::ExprCall;
    type MacroCall<'ast> = MacroCall<'ast>;
    type MethodCall<'ast> = &'ast syn::ExprMethodCall;
}

impl<'ast> Named for <Types as AbstractTypes>::Test<'ast> {
    fn name(&self) -> String {
        self.sig.ident.to_string()
    }
}

impl<'ast> MaybeNamed for <Types as AbstractTypes>::FieldAccess<'ast> {
    fn name(&self) -> Option<String> {
        if let syn::Member::Named(ident) = &self.member {
            Some(ident.to_string())
        } else {
            None
        }
    }
}

impl<'ast> MaybeNamed for <Types as AbstractTypes>::FunctionCall<'ast> {
    fn name(&self) -> Option<String> {
        if let syn::Expr::Path(path) = &*self.func {
            Some(path.to_token_stream().to_string().replace(' ', ""))
        } else {
            None
        }
    }
}

impl<'ast> MaybeNamed for <Types as AbstractTypes>::MacroCall<'ast> {
    fn name(&self) -> Option<String> {
        Some(self.path().to_token_stream().to_string().replace(' ', ""))
    }
}

impl<'ast> MaybeNamed for <Types as AbstractTypes>::MethodCall<'ast> {
    fn name(&self) -> Option<String> {
        Some(self.method.to_string())
    }
}

impl<'ast> Spanned for <Types as AbstractTypes>::Statement<'ast> {
    fn span(&self, source_file: &SourceFile) -> Span {
        <_ as syn::spanned::Spanned>::span(self).to_internal_span(source_file)
    }
}

impl<'ast> Spanned for <Types as AbstractTypes>::FunctionCall<'ast> {
    fn span(&self, source_file: &SourceFile) -> Span {
        <_ as syn::spanned::Spanned>::span(self).to_internal_span(source_file)
    }
}

impl<'ast> Spanned for <Types as AbstractTypes>::MacroCall<'ast> {
    fn span(&self, source_file: &SourceFile) -> Span {
        <_ as syn::spanned::Spanned>::span(self.path()).to_internal_span(source_file)
    }
}

impl<'ast> Spanned for <Types as AbstractTypes>::MethodCall<'ast> {
    fn span(&self, source_file: &SourceFile) -> Span {
        let mut span = <_ as syn::spanned::Spanned>::span(self).to_internal_span(source_file);
        span.start = <_ as syn::spanned::Spanned>::span(&self.dot_token).start();
        assert!(span.start <= span.end);
        span
    }
}

impl ParseLow for Rust {
    type Types = Types;

    const IGNORED_FUNCTIONS: &'static [&'static str] = &[];

    const IGNORED_MACROS: &'static [&'static str] = &[
        "assert",
        "assert_eq",
        "assert_matches",
        "assert_ne",
        "eprint",
        "eprintln",
        "panic",
        "print",
        "println",
        "unimplemented",
        "unreachable",
    ];

    const IGNORED_METHODS: &'static [&'static str] = &[
        "as_bytes",
        "as_mut",
        "as_mut_os_str",
        "as_mut_os_string",
        "as_mut_slice",
        "as_mut_str",
        "as_os_str",
        "as_os_str_bytes",
        "as_path",
        "as_ref",
        "as_slice",
        "as_str",
        "borrow",
        "borrow_mut",
        "clone",
        "cloned",
        "copied",
        "deref",
        "deref_mut",
        "expect",
        "expect_err",
        "into_boxed_bytes",
        "into_boxed_os_str",
        "into_boxed_path",
        "into_boxed_slice",
        "into_boxed_str",
        "into_bytes",
        "into_os_string",
        "into_owned",
        "into_path_buf",
        "into_string",
        "into_vec",
        "iter",
        "iter_mut",
        "success",
        "to_os_string",
        "to_owned",
        "to_path_buf",
        "to_string",
        "to_vec",
        "unwrap",
        "unwrap_err",
    ];

    fn walk_dir(root: &Path) -> Box<dyn Iterator<Item = WalkDirResult>> {
        Box::new(
            walkdir::WalkDir::new(root)
                .into_iter()
                .filter_entry(|entry| {
                    let path = entry.path();
                    path.file_name() != Some(OsStr::new("target"))
                        && (!path.is_file() || path.extension() == Some(OsStr::new("rs")))
                }),
        )
    }

    fn parse_file(&self, test_file: &Path) -> Result<<Self::Types as AbstractTypes>::File> {
        let content = read_to_string(test_file)?;
        syn::parse_file(&content).map_err(Into::into)
    }

    fn storage_from_file(
        file: &<Self::Types as AbstractTypes>::File,
    ) -> <Self::Types as AbstractTypes>::Storage<'_> {
        Storage::new(file)
    }

    fn visit_file<'ast>(
        generic_visitor: GenericVisitor<'_, '_, '_, 'ast, Self>,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        file: &'ast <Self::Types as AbstractTypes>::File,
    ) -> Result<Vec<Span>> {
        visit(generic_visitor, storage, file)
    }

    fn on_candidate_found(
        &mut self,
        context: &LightContext,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'_>>,
        test_name: &str,
        span: &Span,
    ) {
        #[cfg_attr(
            dylint_lib = "non_local_effect_before_error_return",
            allow(non_local_effect_before_error_return)
        )]
        let result = (|| {
            let _ = self.cached_test_file_flags(
                &mut storage.borrow_mut().test_file_package_cache,
                &span.source_file,
            )?;
            let test_path = match storage.borrow_mut().test_path(span, test_name) {
                Ok(test_path) => test_path,
                Err(error) => {
                    if error.downcast_ref::<StripPrefixError>().is_some() {
                        warn(
                            context,
                            Warning::ModulePathUnknown,
                            &format!("Failed to determine module path: {error}"),
                            WarnFlags::empty(),
                        )?;
                    }
                    return Ok(());
                }
            };
            self.set_span_test_path(span, test_path);
            Ok(())
        })();
        if let Err(error) = result {
            let mut storage = storage.borrow_mut();
            storage.error = storage.error.take().or(Some(error));
        }
    }

    fn test_statements<'ast>(
        &self,
        _storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        test: <Self::Types as AbstractTypes>::Test<'ast>,
    ) -> Vec<<Self::Types as AbstractTypes>::Statement<'ast>> {
        test.block.stmts.iter().collect::<Vec<_>>()
    }

    fn statement_is_call<'ast>(
        &self,
        _storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        statement: &'ast syn::Stmt,
    ) -> bool {
        matches!(statement, syn::Stmt::Macro(_))
            || match statement {
                syn::Stmt::Expr(expr, _) => Some(expr),
                _ => None,
            }
            .map_or(false, |expr| {
                matches!(
                    expr,
                    syn::Expr::Call(..) | syn::Expr::Macro(..) | syn::Expr::MethodCall(..)
                )
            })
    }

    fn statement_is_control<'ast>(
        &self,
        _storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        statement: &'ast syn::Stmt,
    ) -> bool {
        match statement {
            syn::Stmt::Expr(expr, _) => Some(expr),
            _ => None,
        }
        .map_or(false, |expr| {
            matches!(
                expr,
                syn::Expr::Break(_) | syn::Expr::Continue(_) | syn::Expr::Return(_)
            )
        })
    }

    fn statement_is_declaration<'ast>(
        &self,
        _storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        statement: <Self::Types as AbstractTypes>::Statement<'ast>,
    ) -> bool {
        matches!(statement, syn::Stmt::Item(_) | syn::Stmt::Local(_))
    }

    fn function_call_is_statement<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        function_call: <Self::Types as AbstractTypes>::FunctionCall<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Statement<'ast>> {
        storage
            .borrow()
            .last_statement_visited
            .and_then(|statement| {
                if let syn::Stmt::Expr(syn::Expr::Call(last_function_call), _) = statement {
                    if function_call == last_function_call {
                        Some(statement)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
    }

    fn macro_call_is_statement<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        macro_call: <Self::Types as AbstractTypes>::MacroCall<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Statement<'ast>> {
        storage
            .borrow()
            .last_statement_visited
            .and_then(|statement| {
                if match (macro_call, statement) {
                    (MacroCall::Stmt(stmt_macro), syn::Stmt::Macro(last_macro_call)) => {
                        stmt_macro == last_macro_call
                    }
                    (
                        MacroCall::Expr(expr_macro),
                        syn::Stmt::Expr(syn::Expr::Macro(last_macro_call), _),
                    ) => expr_macro == last_macro_call,
                    (_, _) => false,
                } {
                    Some(statement)
                } else {
                    None
                }
            })
    }

    fn method_call_is_statement<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        method_call: <Self::Types as AbstractTypes>::MethodCall<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Statement<'ast>> {
        storage
            .borrow()
            .last_statement_visited
            .and_then(|statement| {
                if let syn::Stmt::Expr(syn::Expr::MethodCall(last_method_call), _) = statement {
                    if method_call == last_method_call {
                        Some(statement)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
    }

    fn field_access_has_inner_field_access<'ast>(
        &self,
        _storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        field_access: <Self::Types as AbstractTypes>::FieldAccess<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::FieldAccess<'ast>> {
        if let syn::Expr::Field(field_access) = &*field_access.base {
            Some(field_access)
        } else {
            None
        }
    }

    fn function_call_has_inner_field_access<'ast>(
        &self,
        _storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        function_call: <Self::Types as AbstractTypes>::FunctionCall<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::FieldAccess<'ast>> {
        if let syn::Expr::Field(field_access) = &*function_call.func {
            Some(field_access)
        } else {
            None
        }
    }

    fn method_call_has_inner_field_access<'ast>(
        &self,
        _storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        method_call: <Self::Types as AbstractTypes>::MethodCall<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::FieldAccess<'ast>> {
        if let syn::Expr::Field(field_access) = &*method_call.receiver {
            Some(field_access)
        } else {
            None
        }
    }

    fn function_call_is_method_call_receiver<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        function_call: <Self::Types as AbstractTypes>::FunctionCall<'ast>,
    ) -> bool {
        storage
            .borrow()
            .last_method_call_visited
            .map_or(false, |method_call| {
                if let syn::Expr::Call(last_function_call) = &*method_call.receiver {
                    function_call == last_function_call
                } else {
                    false
                }
            })
    }

    fn macro_call_is_method_call_receiver<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        macro_call: <Self::Types as AbstractTypes>::MacroCall<'ast>,
    ) -> bool {
        storage
            .borrow()
            .last_method_call_visited
            .map_or(false, |method_call| {
                match (macro_call, &*method_call.receiver) {
                    (MacroCall::Expr(macro_call), syn::Expr::Macro(ref last_macro_call)) => {
                        macro_call == last_macro_call
                    }
                    (_, _) => false,
                }
            })
    }
}

impl RunLow for Rust {
    fn command_to_run_test_file(&self, context: &LightContext, test_file: &Path) -> Command {
        self.test_command(context, test_file)
    }

    fn command_to_build_test(&self, context: &LightContext, span: &Span) -> Command {
        let mut command = self.test_command(context, &span.source_file);
        command.arg("--no-run");
        command
    }

    fn command_to_run_test(
        &self,
        context: &LightContext,
        span: &Span,
    ) -> (Command, Vec<String>, Option<(ProcessLines, String)>) {
        #[allow(clippy::expect_used)]
        let test_path = self
            .span_test_path_map
            .get(span)
            .expect("Test path is not set");
        let test = test_path.join("::");

        (
            self.test_command(context, &span.source_file),
            vec!["--".to_owned(), "--exact".to_owned(), test.clone()],
            Some(((false, Box::new(|line| line == "running 1 test")), test)),
        )
    }
}

impl Rust {
    fn test_command(&self, _context: &LightContext, test_file: &Path) -> Command {
        #[allow(clippy::expect_used)]
        let flags = self
            .test_file_flags_cache
            .get(test_file)
            .expect("Flags are not cached");
        let mut command = Command::new("cargo");
        command.arg("test");
        command.args(flags);
        command
    }

    #[cfg_attr(
        dylint_lib = "non_local_effect_before_error_return",
        allow(non_local_effect_before_error_return)
    )]
    fn cached_test_file_flags(
        &mut self,
        test_file_package_map: &mut BTreeMap<PathBuf, Package>,
        test_file: &Path,
    ) -> Result<&Vec<String>> {
        self.test_file_flags_cache
            .entry(test_file.to_path_buf())
            .or_try_insert_with(|| {
                let package = cached_test_file_package(test_file_package_map, test_file)?;

                let mut flags = vec![
                    "--manifest-path".to_owned(),
                    package.manifest_path.as_str().to_owned(),
                ];

                if let Some(name) = test_file_test(package, test_file) {
                    flags.extend(["--test".to_owned(), name.clone()]);
                } else {
                    // smoelius: Failed to find a test target with this file name. Assume it is a
                    // unit test.
                    for kind in package.targets.iter().flat_map(|target| &target.kind) {
                        match kind.as_ref() {
                            "bin" => flags.push("--bins".to_owned()),
                            "lib" => flags.push("--lib".to_owned()),
                            _ => {}
                        }
                    }
                }

                Ok(flags)
            })
            .map(|value| value as &_)
    }

    fn set_span_test_path(&mut self, span: &Span, test_path: Vec<String>) {
        self.span_test_path_map.insert(span.clone(), test_path);
    }
}

fn test_file_test<'a>(package: &'a Package, test_file: &Path) -> Option<&'a String> {
    if let &[name] = package
        .targets
        .iter()
        .filter_map(|target| {
            if target.kind == ["test"] && target.src_path == test_file {
                Some(&target.name)
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .as_slice()
    {
        Some(name)
    } else {
        None
    }
}
