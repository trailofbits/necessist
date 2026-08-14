use super::{LocalFunction, Python, Statement, Test};
use crate::GenericVisitor;
use anyhow::Result;
use necessist_core::framework::{SpanTestMaps, TestSet};
use ruff_python_ast::{
    Expr, ExprCall, ModModule, Stmt, StmtClassDef,
    visitor::{Visitor as AstVisitor, walk_expr, walk_stmt},
};
use std::{cell::RefCell, collections::BTreeMap};

pub(super) fn collect_local_functions(
    module: &ModModule,
) -> BTreeMap<String, Vec<LocalFunction<'_>>> {
    let mut functions = BTreeMap::<_, Vec<_>>::new();
    collect_functions(&module.body, &mut functions);
    functions
}

fn collect_functions<'ast>(
    body: &'ast [Stmt],
    functions: &mut BTreeMap<String, Vec<LocalFunction<'ast>>>,
) {
    for statement in body {
        match statement {
            Stmt::FunctionDef(function) => {
                if !function.name.as_str().starts_with("test_") {
                    functions
                        .entry(function.name.to_string())
                        .or_default()
                        .push(LocalFunction(function));
                }
            }
            Stmt::ClassDef(class) => collect_functions(&class.body, functions),
            _ => {}
        }
    }
}

pub(super) fn visit<'ast>(
    generic_visitor: GenericVisitor<'_, '_, '_, 'ast, Python>,
    storage: &RefCell<()>,
    module: &'ast ModModule,
) -> Result<(TestSet, SpanTestMaps)> {
    let mut visitor = Visitor {
        generic_visitor,
        storage,
    };
    visitor.visit_tests(&module.body, None);
    while let Some(local_function) = visitor.generic_visitor.next_local_function() {
        visitor.visit_body(&local_function.0.body);
    }
    visitor.generic_visitor.results()
}

struct Visitor<'context, 'config, 'backend, 'ast, 'storage> {
    generic_visitor: GenericVisitor<'context, 'config, 'backend, 'ast, Python>,
    storage: &'storage RefCell<()>,
}

impl<'ast> Visitor<'_, '_, '_, 'ast, '_> {
    fn visit_tests(&mut self, body: &'ast [Stmt], class: Option<&'ast StmtClassDef>) {
        for statement in body {
            match statement {
                Stmt::FunctionDef(function) if function.name.as_str().starts_with("test_") => {
                    let test = Test {
                        class: class.map(|class| class.name.as_str()),
                        function,
                    };
                    let walk = self.generic_visitor.visit_test(self.storage, test);
                    if walk {
                        self.visit_body(&function.body);
                    }
                    self.generic_visitor.visit_test_post(self.storage, test);
                }
                Stmt::ClassDef(class) if class.name.as_str().starts_with("Test") => {
                    self.visit_tests(&class.body, Some(class));
                }
                _ => {}
            }
        }
    }
}

impl<'ast> AstVisitor<'ast> for Visitor<'_, '_, '_, 'ast, '_> {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        let statement = Statement(statement);
        let walk = self
            .generic_visitor
            .visit_statement(self.storage, statement);
        if walk && !matches!(statement.0, Stmt::FunctionDef(_) | Stmt::ClassDef(_)) {
            walk_stmt(self, statement.0);
        }
        self.generic_visitor
            .visit_statement_post(self.storage, statement);
    }

    fn visit_expr(&mut self, expression: &'ast Expr) {
        if let Expr::Call(call) = expression {
            self.visit_call(call);
        } else {
            walk_expr(self, expression);
        }
    }
}

impl<'ast> Visitor<'_, '_, '_, 'ast, '_> {
    fn visit_call(&mut self, call: &'ast ExprCall) {
        let call = super::Call(call);
        let walk = self.generic_visitor.visit_call(self.storage, call);
        if walk {
            self.visit_expr(&call.0.func);
            self.visit_arguments(&call.0.arguments);
        } else if let Expr::Attribute(attribute) = call.0.func.as_ref() {
            self.visit_expr(&attribute.value);
        }
        self.generic_visitor.visit_call_post(self.storage, call);
    }
}
