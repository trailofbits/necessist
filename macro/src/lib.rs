#![feature(proc_macro_span)]

use necessist_common::{self as necessist, removed_message};
use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use std::{cell::Cell, env};
use syn::{
    parse_macro_input,
    spanned::Spanned,
    visit_mut::{visit_stmt_mut, VisitMut},
    Item, ItemFn, Stmt,
};

#[proc_macro_attribute]
pub fn necessist(_: TokenStream, item: TokenStream) -> TokenStream {
    let mut item = parse_macro_input!(item as ItemFn);
    if enabled("") {
        let target = necessist::Span::from_env().unwrap();
        let current = item.span();
        if enabled("DEBUG") {
            eprintln!("{:?}: `{}`", current, item.to_token_stream());
        }
        if target.source_file
            == current
                .unwrap()
                .source_file()
                .path()
                .canonicalize()
                .unwrap()
        {
            StmtVisitor {
                target,
                leaves_visited: Cell::new(0),
            }
            .visit_item_fn_mut(&mut item);
        }
    }
    item.to_token_stream().into()
}

struct StmtVisitor {
    target: necessist::Span,
    leaves_visited: Cell<usize>,
}

impl VisitMut for StmtVisitor {
    fn visit_stmt_mut(&mut self, stmt: &mut Stmt) {
        let before = self.leaves_visited.get();
        visit_stmt_mut(self, stmt);
        let after = self.leaves_visited.get();

        // smoelius: This is a leaf if-and-only-if no leaves were visited during the recursive call.
        if before != after {
            return;
        }
        self.leaves_visited.set(after + 1);

        let current = necessist::Span::from_span(&self.target.source_file, stmt.span());
        if self.target == current {
            *stmt = Stmt::Item(Item::Verbatim(quote! {}));
            println!("{}", removed_message(&self.target));
        }
    }
}

fn enabled(suffix: &str) -> bool {
    let key = "NECESSIST".to_owned() + if suffix.is_empty() { "" } else { "_" } + suffix;
    env::var(key).map_or(false, |value| value != "0")
}
