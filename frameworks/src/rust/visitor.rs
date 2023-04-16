use super::{cached_test_file_fs_module_path, Parsing, Rust};
use anyhow::{Error, Result};
use necessist_core::{
    warn, Config, LightContext, SourceFile, Span, ToInternalSpan, WarnFlags, Warning,
};
use std::{
    path::{Path, StripPrefixError},
    rc::Rc,
};
use syn::{
    punctuated::Punctuated,
    spanned::Spanned,
    visit::{visit_expr_method_call, visit_item_fn, visit_item_mod, visit_stmt, Visit},
    Expr, ExprMacro, ExprMethodCall, File, Ident, ItemFn, ItemMod, Macro, PathSegment, Stmt,
    StmtMacro, Token,
};

#[cfg_attr(
    dylint_lib = "non_local_effect_before_error_return",
    allow(non_local_effect_before_error_return)
)]
pub(super) fn visit(
    context: &LightContext,
    config: &Config,
    framework: &mut Rust,
    parsing: &mut Parsing,
    test_file: &Path,
    file: &File,
) -> Result<Vec<Span>> {
    let mut visitor = Visitor::new(context, config, framework, parsing, test_file);
    visitor.visit_file(file);
    if let Some(error) = visitor.error {
        Err(error)
    } else {
        Ok(visitor.spans)
    }
}

struct Visitor<'ast, 'context, 'config, 'framework, 'parsing> {
    context: &'context LightContext<'context>,
    config: &'config Config,
    framework: &'framework mut Rust,
    parsing: &'parsing mut Parsing,
    source_file: SourceFile,
    module_path: Vec<&'ast Ident>,
    test_ident: Option<&'ast Ident>,
    n_stmt_leaves_visited: usize,
    spans: Vec<Span>,
    error: Option<Error>,
}

impl<'ast, 'context, 'config, 'framework, 'parsing> Visit<'ast>
    for Visitor<'ast, 'context, 'config, 'framework, 'parsing>
where
    'ast: 'parsing,
    'framework: 'parsing,
{
    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        if self.error.is_some() {
            return;
        }

        if self.test_ident.is_none() {
            self.module_path.push(&item.ident);
        }

        visit_item_mod(self, item);

        if self.test_ident.is_none() {
            assert_eq!(self.module_path.pop(), Some(&item.ident));
        }
    }

    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        if self.error.is_some() {
            return;
        }

        if let Some(ident) = is_test(item) {
            assert!(self.test_ident.is_none());
            self.test_ident = Some(ident);

            visit_item_fn(self, item);

            assert!(self.test_ident == Some(ident));
            self.test_ident = None;
        }
    }

    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        if self.error.is_some() {
            return;
        }

        let n_before = self.n_stmt_leaves_visited;
        visit_stmt(self, stmt);
        let n_after = self.n_stmt_leaves_visited;

        // smoelius: Consider this a "leaf" if-and-only-if no "leaves" were added during the
        // recursive call.
        if n_before != n_after {
            return;
        }
        self.n_stmt_leaves_visited += 1;

        if let Some(ident) = self.test_ident {
            if !is_method_call_statement(stmt)
                && !matches!(stmt, Stmt::Item(_) | Stmt::Local(_))
                && !is_control(stmt)
                && !is_ignored_macro(self.config, stmt)
            {
                let span = stmt.span().to_internal_span(&self.source_file);
                self.elevate_span(span, ident);
            }
        }
    }

    fn visit_expr_method_call(&mut self, method_call: &'ast ExprMethodCall) {
        if self.error.is_some() {
            return;
        }

        visit_expr_method_call(self, method_call);

        let ExprMethodCall {
            dot_token,
            method,
            args,
            ..
        } = method_call;

        if let Some(ident) = self.test_ident {
            if !is_ignored_method(method, args) {
                let mut span = method_call.span().to_internal_span(&self.source_file);
                span.start = dot_token.span().start();
                assert!(span.start <= span.end);
                self.elevate_span(span, ident);
            }
        }
    }
}

impl<'ast, 'context, 'config, 'framework, 'parsing>
    Visitor<'ast, 'context, 'config, 'framework, 'parsing>
where
    'ast: 'parsing,
    'framework: 'parsing,
{
    fn new(
        context: &'context LightContext,
        config: &'config Config,
        framework: &'framework mut Rust,
        parsing: &'parsing mut Parsing,
        test_file: &Path,
    ) -> Self {
        Self {
            context,
            config,
            framework,
            parsing,
            source_file: SourceFile::new(context.root.clone(), Rc::new(test_file.to_path_buf())),
            module_path: Vec::new(),
            test_ident: None,
            n_stmt_leaves_visited: 0,
            spans: Vec::new(),
            error: None,
        }
    }

    #[cfg_attr(
        dylint_lib = "non_local_effect_before_error_return",
        allow(non_local_effect_before_error_return)
    )]
    fn elevate_span(&mut self, span: Span, ident: &Ident) {
        let result = (|| {
            let _ = self.framework.cached_test_file_flags(
                &mut self.parsing.test_file_package_cache,
                &span.source_file,
            )?;
            let test_path = match self.test_path(&span, ident) {
                Ok(test_path) => test_path,
                Err(error) => {
                    if error.downcast_ref::<StripPrefixError>().is_some() {
                        warn(
                            self.context,
                            Warning::ModulePathUnknown,
                            &format!("Failed to determine module path: {error}"),
                            WarnFlags::empty(),
                        )?;
                    }
                    return Ok(());
                }
            };
            self.framework.set_span_test_path(&span, test_path);
            self.spans.push(span);
            Ok(())
        })();
        if let Err(error) = result {
            self.error = self.error.take().or(Some(error));
        }
    }

    fn test_path(&mut self, span: &Span, ident: &Ident) -> Result<Vec<String>> {
        let mut test_path = cached_test_file_fs_module_path(
            &mut self.parsing.test_file_fs_module_path_cache,
            &mut self.parsing.test_file_package_cache,
            &span.source_file,
        )
        .cloned()?;
        test_path.extend(self.module_path.iter().map(ToString::to_string));
        test_path.push(ident.to_string());
        Ok(test_path)
    }
}

fn is_test(item: &ItemFn) -> Option<&Ident> {
    if item.attrs.iter().any(|attr| {
        let path = attr
            .path()
            .segments
            .iter()
            .map(|PathSegment { ident, arguments }| {
                if arguments.is_empty() {
                    Some(ident.to_string())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        matches!(
            path.iter()
                .map(Option::as_deref)
                .collect::<Vec<_>>()
                .as_slice(),
            &[Some("test")] | &[Some("tokio"), Some("test")]
        )
    }) {
        Some(&item.sig.ident)
    } else {
        None
    }
}

fn is_method_call_statement(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Expr(expr, _) => Some(expr),
        _ => None,
    }
    .map_or(false, |expr| matches!(expr, Expr::MethodCall(..)))
}

fn is_control(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Expr(expr, _) => Some(expr),
        _ => None,
    }
    .map_or(false, |expr| {
        matches!(expr, Expr::Break(_) | Expr::Continue(_) | Expr::Return(_))
    })
}

const IGNORED_MACROS: &[&str] = &[
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

fn is_ignored_macro(config: &Config, stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Macro(StmtMacro {
            mac: Macro { path, .. },
            ..
        })
        | Stmt::Expr(
            Expr::Macro(ExprMacro {
                mac: Macro { path, .. },
                ..
            }),
            _,
        ) => path.get_ident().map_or(false, |ident| {
            let s = ident.to_string();
            IGNORED_MACROS.binary_search(&s.as_ref()).is_ok() || config.ignored_macros.contains(&s)
        }),
        _ => false,
    }
}

const IGNORED_METHODS: &[&str] = &[
    "as_bytes",
    "as_mut",
    "as_mut_os_str",
    "as_mut_os_string",
    "as_mut_slice",
    "as_mut_str",
    "as_os_str",
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

fn is_ignored_method(method: &Ident, args: &Punctuated<Expr, Token![,]>) -> bool {
    IGNORED_METHODS
        .binary_search(&method.to_string().as_ref())
        .is_ok()
        && args.is_empty()
}

#[cfg(test)]
mod test {
    use super::{IGNORED_MACROS, IGNORED_METHODS};
    use if_chain::if_chain;
    use std::fs::read_to_string;
    use syn::{parse_file, Expr, ExprArray, ExprLit, ExprReference, Item, ItemConst, Lit};

    const UNNECESSARY_CONVERSION_FOR_TRAIT_URL: &str = "https://raw.githubusercontent.com/trailofbits/dylint/master/examples/supplementary/unnecessary_conversion_for_trait/src/lib.rs";

    const REMOVED_METHODS: &[&str] = &["path", "new"];

    const ADDED_METHODS: &[&str] = &[
        "clone",
        "cloned",
        "copied",
        "expect",
        "expect_err",
        "into_owned",
        "success",
        "unwrap",
        "unwrap_err",
    ];

    #[test]
    fn readme_contains_ignored_macros() {
        assert!(readme_contains_code_bulleted_list(IGNORED_MACROS));
    }

    #[test]
    fn readme_contains_ignored_methods() {
        assert!(readme_contains_code_bulleted_list(IGNORED_METHODS));
    }

    #[test]
    fn readme_contains_ignored_method_additions() {
        assert!(readme_contains_code_bulleted_list(ADDED_METHODS));
    }

    #[test]
    fn ignored_macros_are_sorted() {
        assert_eq!(sort(IGNORED_MACROS), IGNORED_MACROS);
    }

    #[test]
    fn ignored_methods_are_sorted() {
        assert_eq!(sort(IGNORED_METHODS), IGNORED_METHODS);
    }

    #[test]
    fn added_methods_are_sorted() {
        assert_eq!(sort(ADDED_METHODS), ADDED_METHODS);
    }

    #[test]
    fn ignored_methods_match_unnecessary_conversion_for_trait_watched_methods() {
        let data = get(UNNECESSARY_CONVERSION_FOR_TRAIT_URL).unwrap();
        let contents = std::str::from_utf8(&data).unwrap();
        #[allow(clippy::panic)]
        let file = parse_file(contents).unwrap_or_else(|_| panic!("Failed to parse: {contents:?}"));
        let mut watched_methods = file
            .items
            .into_iter()
            .flat_map(|item| {
                let elems = if_chain! {
                    if let Item::Const(ItemConst { ident, expr, .. }) = item;
                    if ["WATCHED_TRAITS", "WATCHED_INHERENTS"]
                        .contains(&ident.to_string().as_str());
                    if let Expr::Reference(ExprReference { expr, .. }) = *expr;
                    if let Expr::Array(ExprArray { elems, .. }) = *expr;
                    then {
                        elems.iter().cloned().collect::<Vec<_>>()
                    } else {
                        Vec::new()
                    }
                };
                elems
                    .into_iter()
                    .filter_map(|expr| {
                        if_chain! {
                            if let Expr::Reference(ExprReference { expr, .. }) = expr;
                            if let Expr::Array(ExprArray { elems, .. }) = *expr;
                            if let Some(Expr::Lit(ExprLit { lit, .. })) = elems.last();
                            if let Lit::Str(lit_str) = lit;
                            let s = lit_str.value();
                            if !REMOVED_METHODS.contains(&s.as_str());
                            then {
                                Some(s)
                            } else {
                                None
                            }
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .chain(ADDED_METHODS.iter().map(ToString::to_string))
            .collect::<Vec<_>>();
        watched_methods.sort_unstable();
        watched_methods.dedup();
        assert_eq!(sort(IGNORED_METHODS), watched_methods);
    }

    fn readme_contains_code_bulleted_list(items: &[&str]) -> bool {
        let n = items.len();
        #[allow(clippy::unwrap_used)]
        let readme = read_to_string("../README.md").unwrap();
        readme.lines().collect::<Vec<_>>().windows(n).any(|window| {
            window
                .iter()
                .zip(items)
                .all(|(line, item)| line.starts_with(&format!("- `{item}`")))
        })
    }

    fn sort<'a>(items: &'a [&str]) -> Vec<&'a str> {
        let mut items = items.to_vec();
        items.sort_unstable();
        items
    }

    fn get(url: &str) -> Result<Vec<u8>, curl::Error> {
        let mut data = Vec::new();
        let mut handle = curl::easy::Easy::new();
        handle.url(url)?;
        {
            let mut transfer = handle.transfer();
            transfer.write_function(|new_data| {
                data.extend_from_slice(new_data);
                Ok(new_data.len())
            })?;
            transfer.perform()?;
        }
        Ok(data)
    }
}
