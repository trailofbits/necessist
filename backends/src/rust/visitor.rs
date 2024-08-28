use super::{Call, GenericVisitor, MacroCall, Rust, Storage, Test};
use anyhow::Result;
use necessist_core::{
    framework::{SpanTestMaps, TestSet},
    warn, WarnFlags, Warning,
};
use std::cell::RefCell;
use syn::{
    visit::{
        visit_expr_call, visit_expr_macro, visit_expr_method_call, visit_item_fn, visit_item_mod,
        visit_stmt, visit_stmt_macro, Visit,
    },
    ExprCall, ExprMacro, ExprMethodCall, File, Ident, ItemFn, ItemMod, PathSegment, Stmt,
    StmtMacro,
};

#[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
pub(super) fn visit<'ast>(
    generic_visitor: GenericVisitor<'_, '_, '_, 'ast, Rust>,
    storage: &RefCell<Storage<'ast>>,
    file: &'ast File,
) -> Result<(TestSet, SpanTestMaps)> {
    let mut visitor = Visitor::new(generic_visitor, storage);
    visitor.visit_file(file);
    for (test_name, error) in &storage.borrow().tests_needing_warnings {
        warn(
            visitor.generic_visitor.context,
            Warning::ModulePathUnknown,
            &format!("Failed to determine module path for test `{test_name}`: {error:?}"),
            WarnFlags::empty(),
        )?;
    }
    if let Some(error) = storage.borrow_mut().error.take() {
        return Err(error);
    }
    let _: &Vec<String> = visitor.generic_visitor.backend.cached_source_file_flags(
        &mut storage.borrow_mut().source_file_package_cache,
        &visitor.generic_visitor.source_file,
    )?;
    Ok(visitor.generic_visitor.results())
}

struct Visitor<'context, 'config, 'backend, 'ast, 'storage> {
    generic_visitor: GenericVisitor<'context, 'config, 'backend, 'ast, Rust>,
    storage: &'storage RefCell<Storage<'ast>>,
    test_ident: Option<&'ast Ident>,
}

impl<'context, 'config, 'backend, 'ast, 'storage>
    Visitor<'context, 'config, 'backend, 'ast, 'storage>
{
    fn new(
        generic_visitor: GenericVisitor<'context, 'config, 'backend, 'ast, Rust>,
        storage: &'storage RefCell<Storage<'ast>>,
    ) -> Self {
        Self {
            generic_visitor,
            storage,
            test_ident: None,
        }
    }
}

impl<'context, 'config, 'backend, 'ast, 'storage> Visit<'ast>
    for Visitor<'context, 'config, 'backend, 'ast, 'storage>
{
    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        if self.test_ident.is_none() {
            self.storage.borrow_mut().module_path.push(&item.ident);
        }

        visit_item_mod(self, item);

        if self.test_ident.is_none() {
            assert_eq!(
                self.storage.borrow_mut().module_path.pop(),
                Some(&item.ident)
            );
        }
    }

    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        if let Some(ident) = is_test(item) {
            assert!(self.test_ident.is_none());
            self.test_ident = Some(ident);

            if let Some(test) = Test::new(self.storage, &self.generic_visitor.source_file, item) {
                let walk = self.generic_visitor.visit_test(self.storage, test);

                if walk {
                    visit_item_fn(self, item);
                }

                self.generic_visitor.visit_test_post(self.storage, test);
            }

            assert_eq!(self.test_ident, Some(ident));
            self.test_ident = None;
        }
    }

    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        let walk = self.generic_visitor.visit_statement(self.storage, stmt);

        if walk {
            visit_stmt(self, stmt);
        }

        self.generic_visitor
            .visit_statement_post(self.storage, stmt);
    }

    fn visit_expr_call(&mut self, function_call: &'ast ExprCall) {
        let call = Call::FunctionCall(function_call);

        let walk = self.generic_visitor.visit_call(self.storage, call);

        if walk {
            visit_expr_call(self, function_call);
        } else {
            self.visit_expr(&function_call.func);
        }

        self.generic_visitor.visit_call_post(self.storage, call);
    }

    fn visit_stmt_macro(&mut self, mac: &'ast StmtMacro) {
        let macro_call = MacroCall::Stmt(mac);

        let walk = self
            .generic_visitor
            .visit_macro_call(self.storage, macro_call);

        if walk {
            visit_stmt_macro(self, mac);
        }

        self.generic_visitor
            .visit_macro_call_post(self.storage, macro_call);
    }

    fn visit_expr_macro(&mut self, mac: &'ast ExprMacro) {
        let macro_call = MacroCall::Expr(mac);

        let walk = self
            .generic_visitor
            .visit_macro_call(self.storage, macro_call);

        if walk {
            visit_expr_macro(self, mac);
        }

        self.generic_visitor
            .visit_macro_call_post(self.storage, macro_call);
    }

    fn visit_expr_method_call(&mut self, method_call: &'ast ExprMethodCall) {
        let call = Call::MethodCall(method_call);

        let walk = self.generic_visitor.visit_call(self.storage, call);

        if walk {
            visit_expr_method_call(self, method_call);
        } else {
            self.visit_expr(&method_call.receiver);
        }

        self.generic_visitor.visit_call_post(self.storage, call);
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

#[cfg(test)]
mod test {
    use super::Rust;
    use crate::ParseLow;
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
        assert!(readme_contains_code_bulleted_list(
            Rust::IGNORED_MACROS.unwrap()
        ));
    }

    #[test]
    fn readme_contains_ignored_methods() {
        assert!(readme_contains_code_bulleted_list(
            Rust::IGNORED_METHODS.unwrap()
        ));
    }

    #[test]
    fn readme_contains_ignored_method_additions() {
        assert!(readme_contains_code_bulleted_list(ADDED_METHODS));
    }

    #[cfg_attr(
        dylint_lib = "assert_eq_arg_misordering",
        allow(assert_eq_arg_misordering)
    )]
    #[test]
    fn ignored_macros_are_sorted() {
        assert_eq!(
            sort(Rust::IGNORED_MACROS.unwrap()),
            Rust::IGNORED_MACROS.unwrap()
        );
    }

    #[cfg_attr(
        dylint_lib = "assert_eq_arg_misordering",
        allow(assert_eq_arg_misordering)
    )]
    #[test]
    fn ignored_methods_are_sorted() {
        assert_eq!(
            sort(Rust::IGNORED_METHODS.unwrap()),
            Rust::IGNORED_METHODS.unwrap()
        );
    }

    #[cfg_attr(
        dylint_lib = "assert_eq_arg_misordering",
        allow(assert_eq_arg_misordering)
    )]
    #[test]
    fn added_methods_are_sorted() {
        assert_eq!(sort(ADDED_METHODS), ADDED_METHODS);
    }

    #[cfg_attr(
        dylint_lib = "assert_eq_arg_misordering",
        allow(assert_eq_arg_misordering)
    )]
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
                    if ["WATCHED_TRAITS", "WATCHED_INHERENTS"].contains(&ident.to_string().as_str());
                    if let Expr::Reference(ExprReference { expr, .. }) = *expr;
                    if let Expr::Array(ExprArray { elems, .. }) = *expr;
                    then {
                        elems.iter().cloned().collect::<Vec<_>>()
                    } else {
                        Vec::new()
                    }
                };
                elems.into_iter().filter_map(|expr| {
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
            })
            .chain(ADDED_METHODS.iter().map(ToString::to_string))
            .collect::<Vec<_>>();
        watched_methods.sort_unstable();
        watched_methods.dedup();
        assert_eq!(watched_methods, Rust::IGNORED_METHODS.unwrap());
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
