use super::{cached_test_file_fs_module_path, Parsing, Rust};
use crate::{SourceFile, Span, ToInternalSpan};
use anyhow::{Error, Result};
use syn::{
    punctuated::Punctuated,
    spanned::Spanned,
    visit::{visit_item_fn, visit_item_mod, visit_stmt, Visit},
    Expr, ExprMacro, ExprMethodCall, Ident, ItemFn, ItemMod, Macro, PathSegment, Stmt,
};

pub(super) struct Visitor<'ast, 'framework, 'parsing> {
    pub owner: &'framework mut Rust,
    pub parsing: &'parsing mut Parsing,
    pub source_file: SourceFile,
    pub module_path: Vec<&'ast Ident>,
    pub test_ident: Option<&'ast Ident>,
    pub spans: Vec<Span>,
    pub error: Option<Error>,
}

impl<'ast, 'framework, 'parsing> Visit<'ast> for Visitor<'ast, 'framework, 'parsing>
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

        let before = self.spans.len();
        visit_stmt(self, stmt);
        let after = self.spans.len();

        // smoelius: Consider this a "leaf" if-and-only-if no "leaves" were added during the
        // recursive call.
        if before != after {
            return;
        }

        if let Some(ident) = self.test_ident {
            if !matches!(stmt, Stmt::Local(_)) && !is_control(stmt) && !is_whitelisted_macro(stmt) {
                let span = stmt.span().to_internal_span(&self.source_file);
                self.elevate_span(span, ident);
            }
        }
    }

    fn visit_expr_method_call(&mut self, method_call: &'ast ExprMethodCall) {
        if self.error.is_some() {
            return;
        }

        let ExprMethodCall {
            attrs,
            receiver,
            dot_token,
            method,
            turbofish,
            paren_token: _,
            args,
        } = method_call;

        for it in attrs {
            self.visit_attribute(it);
        }
        self.visit_expr(receiver);

        // smoelius: Start tracking leaves added after `receiver` has been traversed.
        let before = self.spans.len();

        self.visit_ident(method);
        if let Some(it) = turbofish {
            self.visit_method_turbofish(it);
        }
        for el in Punctuated::pairs(args) {
            let (it, _) = el.into_tuple();
            self.visit_expr(it);
        }

        let after = self.spans.len();

        // smoelius: See the comment in `visit_stmt` above regarding "leaves."
        if n_before != n_after {
            return;
        }

        if let Some(ident) = self.test_ident {
            if !is_whitelisted_method(method) {
                let mut span = method_call.span().to_internal_span(&self.source_file);
                span.start = dot_token.span().start();
                assert!(span.start <= span.end);
                self.elevate_span(span, ident);
            }
        }
    }
}

impl<'ast, 'framework, 'parsing> Visitor<'ast, 'framework, 'parsing>
where
    'ast: 'parsing,
    'framework: 'parsing,
{
    fn elevate_span(&mut self, span: Span, ident: &Ident) {
        let result = (|| {
            let _ = self.owner.cached_test_file_flags(
                &mut self.parsing.test_file_package_cache,
                &span.source_file,
            )?;
            let test_path = self.test_path(&span, ident)?;
            self.owner.set_span_test_path(&span, test_path);
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
            .path
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

const MACRO_WHITELIST: &[&str] = &[
    "assert",
    "assert_eq",
    "assert_ne",
    "eprint",
    "eprintln",
    "panic",
    "print",
    "println",
    "unimplemented",
    "unreachable",
];

fn is_control(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Expr(expr) | Stmt::Semi(expr, ..) => Some(expr),
        _ => None,
    }
    .map_or(false, |expr| {
        matches!(expr, Expr::Break(_) | Expr::Continue(_))
    })
}

fn is_whitelisted_macro(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Expr(expr) | Stmt::Semi(expr, ..) => Some(expr),
        _ => None,
    }
    .map_or(false, |expr| match expr {
        Expr::Macro(ExprMacro {
            mac: Macro { path, .. },
            ..
        }) => path.get_ident().map_or(false, |ident| {
            MACRO_WHITELIST.contains(&ident.to_string().as_str())
        }),
        _ => false,
    })
}

const METHOD_WHITELIST: &[&str] = &["success", "unwrap", "unwrap_err"];

fn is_whitelisted_method(method: &Ident) -> bool {
    METHOD_WHITELIST.contains(&method.to_string().as_ref())
}

impl ToInternalSpan for proc_macro2::Span {
    fn to_internal_span(&self, source_file: &SourceFile) -> Span {
        Span {
            source_file: source_file.clone(),
            start: self.start(),
            end: self.end(),
        }
    }
}
