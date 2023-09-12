#![cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]

use super::{Foundry, FunctionCall, GenericVisitor, Storage, Test, WithContents};
use anyhow::Result;
use if_chain::if_chain;
use necessist_core::Span;
use solang_parser::pt::{Expression, FunctionDefinition, Identifier, Loc, SourceUnit, Statement};
use std::{cell::RefCell, convert::Infallible};

mod visit;
use visit::{self as visit_fns, Visitor as _};

// smoelius: Used for ignoring statements that are prefixed by a cheatcode.
static FAUX_CONTINUE: Statement = Statement::Continue(Loc::Builtin);

/// Wraps a reference to a vector of `Statement`s so that uses of `filter_statements` and
/// `is_prefix_cheatcode` can be restricted to this file.
#[derive(Clone, Copy)]
pub struct Statements<'ast>(&'ast Vec<Statement>);

impl<'ast> Statements<'ast> {
    pub fn get(self) -> impl Iterator<Item = &'ast Statement> {
        filter_statements(self.0)
    }
}

pub(super) fn visit<'ast>(
    generic_visitor: GenericVisitor<'_, '_, '_, 'ast, Foundry>,
    storage: &RefCell<Storage<'ast>>,
    source_unit: &'ast SourceUnit,
) -> Result<Vec<Span>> {
    let mut visitor = Visitor::new(generic_visitor, storage);
    visitor.visit_source_unit(source_unit)?;
    Ok(visitor.generic_visitor.spans_visited())
}

struct Visitor<'context, 'config, 'framework, 'ast, 'storage> {
    generic_visitor: GenericVisitor<'context, 'config, 'framework, 'ast, Foundry>,
    storage: &'storage RefCell<Storage<'ast>>,
}

impl<'context, 'config, 'framework, 'ast, 'storage>
    Visitor<'context, 'config, 'framework, 'ast, 'storage>
{
    fn new(
        generic_visitor: GenericVisitor<'context, 'config, 'framework, 'ast, Foundry>,
        storage: &'storage RefCell<Storage<'ast>>,
    ) -> Self {
        Self {
            generic_visitor,
            storage,
        }
    }
}

impl<'context, 'config, 'framework, 'ast, 'storage> visit_fns::Visitor<'ast>
    for Visitor<'context, 'config, 'framework, 'ast, 'storage>
{
    type Error = Infallible;

    fn visit_function_definition(
        &mut self,
        function_definition: &'ast FunctionDefinition,
    ) -> Result<(), Self::Error> {
        if let Some(test) = is_test_function(function_definition) {
            let walk = self.generic_visitor.visit_test(self.storage, test);

            if walk {
                visit_fns::visit_function_definition(self, function_definition)?;
            }

            self.generic_visitor.visit_test_post(self.storage, test);
        }

        Ok(())
    }

    fn visit_statement(&mut self, statement: &'ast Statement) -> Result<(), Self::Error> {
        if let Statement::Block { statements, .. } = statement {
            for statement in filter_statements(statements) {
                self.visit_statement(statement)?;
            }

            return Ok(());
        }

        let statement = WithContents {
            contents: self.storage.borrow().contents,
            value: statement,
        };

        let walk = self
            .generic_visitor
            .visit_statement(self.storage, statement);

        if walk {
            visit_fns::visit_statement(self, statement.value)?;
        }

        self.generic_visitor
            .visit_statement_post(self.storage, statement);

        Ok(())
    }

    fn visit_expression(&mut self, expression: &'ast Expression) -> Result<(), Self::Error> {
        if let Expression::FunctionCall(loc, callee, args) = expression {
            let call = WithContents {
                contents: self.storage.borrow().contents,
                value: FunctionCall {
                    loc: *loc,
                    callee,
                    args,
                },
            };

            let walk = self.generic_visitor.visit_call(self.storage, call);

            if walk {
                visit_fns::visit_expression(self, expression)?;
            } else {
                self.visit_expression(callee)?;
            }

            self.generic_visitor.visit_call_post(self.storage, call);

            return Ok(());
        }

        Ok(())
    }
}

fn is_test_function(function_definition: &FunctionDefinition) -> Option<Test<'_>> {
    if_chain! {
        if let Some(Identifier { name, .. }) = &function_definition.name;
        if name.starts_with("test");
        if let Some(Statement::Block { statements, .. }) = &function_definition.body;
        then {
            Some(Test {
                name,
                statements: Statements(statements),
            })
        } else {
            None
        }
    }
}

fn filter_statements<'ast, I: IntoIterator<Item = &'ast Statement>>(
    statements: I,
) -> impl Iterator<Item = &'ast Statement> {
    let mut prev = None;
    statements.into_iter().map(move |statement| {
        // smoelius: If the previous statement was a "prefix cheatcode," then replace the current
        // statement with a `continue`, so that the statement is ignored.
        let next = if prev.map_or(false, is_prefix_cheatcode) {
            &FAUX_CONTINUE
        } else {
            statement
        };
        prev = Some(statement);
        next
    })
}

fn is_prefix_cheatcode(statement: &Statement) -> bool {
    if_chain! {
        if let Statement::Expression(_, expression) = statement;
        if let Expression::FunctionCall(_loc, callee, _args) = expression;
        if let Expression::MemberAccess(_, base, Identifier { name: method, .. }) = &**callee;
        if let Expression::Variable(variable) = &**base;
        if variable.to_string() == "vm";
        if method == "prank" || method.starts_with("expect");
        then {
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod test {
    use super::Foundry;
    use crate::ParseLow;

    #[cfg_attr(
        dylint_lib = "assert_eq_arg_misordering",
        allow(assert_eq_arg_misordering)
    )]
    #[test]
    fn ignored_functions_are_sorted() {
        assert_eq!(
            sort(Foundry::IGNORED_FUNCTIONS.unwrap()),
            Foundry::IGNORED_FUNCTIONS.unwrap()
        );
    }

    fn sort<'a>(items: &'a [&str]) -> Vec<&'a str> {
        let mut items = items.to_vec();
        items.sort_unstable();
        items
    }
}
