use super::{cached_test_file_fs_module_path, Parsing, Rust};
use crate::{SourceFile, Span, ToInternalSpan};
use anyhow::{Error, Result};
use std::path::Path;
use syn::{
    punctuated::Punctuated,
    spanned::Spanned,
    visit::{visit_item_fn, visit_item_mod, visit_stmt, Visit},
    Expr, ExprMacro, ExprMethodCall, Ident, ItemFn, ItemMod, Macro, PathSegment, Stmt, Token,
};

pub(super) struct Visitor<'ast, 'framework, 'parsing> {
    owner: &'framework mut Rust,
    parsing: &'parsing mut Parsing,
    source_file: SourceFile,
    module_path: Vec<&'ast Ident>,
    test_ident: Option<&'ast Ident>,
    spans: Vec<Span>,
    n_method_call_spans: usize,
    error: Option<Error>,
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

        let n_before = self.n_stmt_spans();
        visit_stmt(self, stmt);
        let n_after = self.n_stmt_spans();

        // smoelius: Consider this a "leaf" if-and-only-if no "leaves" were added during the
        // recursive call.
        if n_before != n_after {
            return;
        }

        if let Some(ident) = self.test_ident {
            if !matches!(stmt, Stmt::Item(_) | Stmt::Local(_))
                && !is_control(stmt)
                && !is_ignored_macro(stmt)
            {
                let span = stmt.span().to_internal_span(&self.source_file);
                self.elevate_span(span, ident, false);
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
        let n_before = self.n_method_call_spans();

        self.visit_ident(method);
        if let Some(it) = turbofish {
            self.visit_method_turbofish(it);
        }
        for el in Punctuated::pairs(args) {
            let (it, _) = el.into_tuple();
            self.visit_expr(it);
        }

        let n_after = self.n_method_call_spans();

        // smoelius: See the comment in `visit_stmt` above regarding "leaves."
        if n_before != n_after {
            return;
        }

        if let Some(ident) = self.test_ident {
            if !is_ignored_method(method, args) {
                let mut span = method_call.span().to_internal_span(&self.source_file);
                span.start = dot_token.span().start();
                assert!(span.start <= span.end);
                self.elevate_span(span, ident, true);
            }
        }
    }
}

impl<'ast, 'framework, 'parsing> Visitor<'ast, 'framework, 'parsing>
where
    'ast: 'parsing,
    'framework: 'parsing,
{
    pub fn new(
        owner: &'framework mut Rust,
        parsing: &'parsing mut Parsing,
        test_file: &Path,
    ) -> Self {
        Self {
            owner,
            parsing,
            source_file: SourceFile::new(test_file),
            module_path: Vec::new(),
            test_ident: None,
            spans: Vec::new(),
            n_method_call_spans: 0,
            error: None,
        }
    }

    pub fn spans(self) -> Result<Vec<Span>> {
        if let Some(error) = self.error {
            Err(error)
        } else {
            Ok(self.spans)
        }
    }

    fn elevate_span(&mut self, span: Span, ident: &Ident, is_method_call: bool) {
        let result = (|| {
            let _ = self.owner.cached_test_file_flags(
                &mut self.parsing.test_file_package_cache,
                &span.source_file,
            )?;
            let test_path = self.test_path(&span, ident)?;
            self.owner.set_span_test_path(&span, test_path);
            self.spans.push(span);
            if is_method_call {
                self.n_method_call_spans += 1;
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.error = self.error.take().or(Some(error));
        }
    }

    fn n_stmt_spans(&self) -> usize {
        assert!(self.spans.len() >= self.n_method_call_spans);
        self.spans.len() - self.n_method_call_spans
    }

    fn n_method_call_spans(&self) -> usize {
        self.n_method_call_spans
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

fn is_control(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Expr(expr) | Stmt::Semi(expr, ..) => Some(expr),
        _ => None,
    }
    .map_or(false, |expr| {
        matches!(expr, Expr::Break(_) | Expr::Continue(_))
    })
}

const IGNORED_MACROS: &[&str] = &[
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

fn is_ignored_macro(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Expr(expr) | Stmt::Semi(expr, ..) => Some(expr),
        _ => None,
    }
    .map_or(false, |expr| match expr {
        Expr::Macro(ExprMacro {
            mac: Macro { path, .. },
            ..
        }) => path.get_ident().map_or(false, |ident| {
            IGNORED_MACROS.contains(&ident.to_string().as_str())
        }),
        _ => false,
    })
}

const IGNORED_METHODS: &[&str] = &[
    "as_bytes",
    "as_bytes_mut",
    "as_mut",
    "as_mut_ptr",
    "as_os_str",
    "as_path",
    "as_ptr",
    "as_ref",
    "as_slice",
    "as_str",
    "borrow",
    "borrow_mut",
    "clone",
    "cloned",
    "copied",
    "deref",
    "into",
    "into_os_string",
    "into_owned",
    "into_path_buf",
    "into_string",
    "into_vec",
    "success",
    "to_os_string",
    "to_owned",
    "to_path_buf",
    "to_str",
    "to_string",
    "to_string_lossy",
    "to_vec",
    "try_into",
    "unwrap",
    "unwrap_err",
];

fn is_ignored_method(method: &Ident, args: &Punctuated<Expr, Token![,]>) -> bool {
    IGNORED_METHODS.contains(&method.to_string().as_ref()) && args.is_empty()
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

#[cfg(test)]
mod test {
    use super::{IGNORED_MACROS, IGNORED_METHODS};
    use std::fs::read_to_string;

    #[test]
    fn readme_contains_ignored_macros() {
        assert!(readme_contains_code_unordered_list(IGNORED_MACROS));
    }

    #[test]
    fn readme_contains_ignored_methods() {
        assert!(readme_contains_code_unordered_list(IGNORED_METHODS));
    }

    #[test]
    fn ignored_macros_are_sorted() {
        assert_eq!(sort(IGNORED_MACROS), IGNORED_MACROS);
    }

    #[test]
    fn ignored_methods_are_sorted() {
        assert_eq!(sort(IGNORED_METHODS), IGNORED_METHODS);
    }

    #[allow(clippy::unwrap_used)]
    fn readme_contains_code_unordered_list(items: &[&str]) -> bool {
        let n = items.len();
        let readme = read_to_string("README.md").unwrap();
        readme.lines().collect::<Vec<_>>().windows(n).any(|window| {
            window
                .iter()
                .zip(items)
                .all(|(line, item)| line.starts_with(&format!("- `{}`", item)))
        })
    }

    fn sort<'a>(items: &'a [&str]) -> Vec<&'a str> {
        let mut items = items.to_vec();
        items.sort_unstable();
        items
    }
}
