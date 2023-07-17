use super::{is_it_call_expr, is_it_call_stmt, GenericVisitor, Mocha, SourceMapped, Storage};
use anyhow::Result;
use necessist_core::Span;
use std::cell::RefCell;
use swc_core::ecma::{
    ast::{Expr, Module, Stmt},
    visit::{visit_expr, visit_stmt, Visit},
};

#[allow(clippy::unnecessary_wraps)]
pub(super) fn visit<'ast>(
    generic_visitor: GenericVisitor<'_, '_, '_, 'ast, Mocha>,
    storage: &RefCell<Storage<'ast>>,
    module: &Module,
) -> Result<Vec<Span>> {
    let mut visitor = Visitor::new(generic_visitor, storage);
    visitor.visit_module(module);
    Ok(visitor.generic_visitor.spans_visited())
}

struct Visitor<'context, 'config, 'framework, 'ast, 'storage> {
    generic_visitor: GenericVisitor<'context, 'config, 'framework, 'ast, Mocha>,
    storage: &'storage RefCell<Storage<'ast>>,
}

impl<'context, 'config, 'framework, 'ast, 'storage>
    Visitor<'context, 'config, 'framework, 'ast, 'storage>
{
    fn new(
        generic_visitor: GenericVisitor<'context, 'config, 'framework, 'ast, Mocha>,
        storage: &'storage RefCell<Storage<'ast>>,
    ) -> Self {
        Self {
            generic_visitor,
            storage,
        }
    }
}

impl<'context, 'config, 'framework, 'ast, 'storage> Visit
    for Visitor<'context, 'config, 'framework, 'ast, 'storage>
{
    fn visit_stmt(&mut self, stmt: &Stmt) {
        // smoelius: Unsafe hack to work around: https://github.com/swc-project/swc/issues/6032
        let stmt = unsafe { std::mem::transmute::<&Stmt, &'ast Stmt>(stmt) };

        if let Some(test) = is_it_call_stmt(stmt) {
            let walk = self.generic_visitor.visit_test(self.storage, test);

            if walk {
                visit_stmt(self, stmt);
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
            visit_stmt(self, stmt);
        }

        self.generic_visitor
            .visit_statement_post(self.storage, statement);
    }

    fn visit_expr(&mut self, expr: &Expr) {
        // smoelius: Unsafe hack to work around: https://github.com/swc-project/swc/issues/6032
        let expr = unsafe { std::mem::transmute::<&Expr, &'ast Expr>(expr) };

        if is_it_call_expr(expr).is_some() {
            visit_expr(self, expr);

            return;
        }

        if let Expr::Call(call) = expr {
            let call = SourceMapped {
                source_map: self.storage.borrow().source_map,
                node: call,
            };

            let walk = self.generic_visitor.visit_call(self.storage, call);

            if walk {
                visit_expr(self, expr);
            } else {
                self.visit_callee(&call.node.callee);
            }

            self.generic_visitor.visit_call_post(self.storage, call);

            return;
        }

        visit_expr(self, expr);
    }
}
