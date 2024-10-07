use super::{is_it_call_expr, is_it_call_stmt, GenericVisitor, Mocha, SourceMapped, Storage};
use anyhow::Result;
use necessist_core::framework::{SpanTestMaps, TestSet};
use std::{cell::RefCell, collections::BTreeMap};
use swc_core::ecma::{
    ast::{Expr, FnDecl, Module, Stmt},
    visit::{Visit, VisitWith},
};

pub(super) fn collect_local_functions(module: &Module) -> BTreeMap<String, Vec<&FnDecl>> {
    let mut collector = FnDeclCollector::default();
    collector.visit_module(module);
    collector.fn_decls.split_off(&String::new())
}

#[derive(Default)]
struct FnDeclCollector<'ast> {
    fn_decls: BTreeMap<String, Vec<&'ast FnDecl>>,
}

impl<'ast> Visit for FnDeclCollector<'ast> {
    fn visit_fn_decl(&mut self, fn_decl: &FnDecl) {
        // smoelius: Unsafe hack to work around: https://github.com/swc-project/swc/issues/6032
        let fn_decl = unsafe { std::mem::transmute::<&FnDecl, &'ast FnDecl>(fn_decl) };

        self.fn_decls
            .entry(fn_decl.ident.to_string())
            .or_default()
            .push(fn_decl);
    }
}

#[allow(clippy::unnecessary_wraps)]
pub(super) fn visit<'ast>(
    generic_visitor: GenericVisitor<'_, '_, '_, 'ast, Mocha>,
    storage: &RefCell<Storage<'ast>>,
    module: &Module,
) -> Result<(TestSet, SpanTestMaps)> {
    let mut visitor = Visitor::new(generic_visitor, storage);
    visitor.visit_module(module);
    while let Some(local_function) = visitor.generic_visitor.next_local_function() {
        visitor.visit_local_function(local_function);
    }
    visitor.generic_visitor.results()
}

struct Visitor<'context, 'config, 'backend, 'ast, 'storage> {
    generic_visitor: GenericVisitor<'context, 'config, 'backend, 'ast, Mocha>,
    storage: &'storage RefCell<Storage<'ast>>,
}

impl<'context, 'config, 'backend, 'ast, 'storage>
    Visitor<'context, 'config, 'backend, 'ast, 'storage>
{
    fn new(
        generic_visitor: GenericVisitor<'context, 'config, 'backend, 'ast, Mocha>,
        storage: &'storage RefCell<Storage<'ast>>,
    ) -> Self {
        Self {
            generic_visitor,
            storage,
        }
    }

    fn visit_local_function(&mut self, local_function: &'ast FnDecl) {
        local_function.visit_children_with(self);
    }
}

impl<'ast> Visit for Visitor<'_, '_, '_, 'ast, '_> {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        // smoelius: Unsafe hack to work around: https://github.com/swc-project/swc/issues/6032
        let stmt = unsafe { std::mem::transmute::<&Stmt, &'ast Stmt>(stmt) };

        if let Some(test) = is_it_call_stmt(stmt) {
            let walk = self.generic_visitor.visit_test(self.storage, test);

            if walk {
                stmt.visit_children_with(self);
            }

            self.generic_visitor.visit_test_post(self.storage, test);

            return;
        }

        let statement = SourceMapped {
            source_map: self.storage.borrow().source_map,
            node: stmt,
        };

        let walk = self
            .generic_visitor
            .visit_statement(self.storage, statement);

        if walk {
            stmt.visit_children_with(self);
        }

        self.generic_visitor
            .visit_statement_post(self.storage, statement);
    }

    fn visit_expr(&mut self, expr: &Expr) {
        // smoelius: Unsafe hack to work around: https://github.com/swc-project/swc/issues/6032
        let expr = unsafe { std::mem::transmute::<&Expr, &'ast Expr>(expr) };

        if is_it_call_expr(expr).is_some() {
            expr.visit_children_with(self);

            return;
        }

        if let Expr::Call(call) = expr {
            let call = SourceMapped {
                source_map: self.storage.borrow().source_map,
                node: call,
            };

            let walk = self.generic_visitor.visit_call(self.storage, call);

            if walk {
                expr.visit_children_with(self);
            } else {
                self.visit_callee(&call.node.callee);
            }

            self.generic_visitor.visit_call_post(self.storage, call);

            return;
        }

        expr.visit_children_with(self);
    }
}
