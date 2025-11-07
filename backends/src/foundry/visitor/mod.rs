#![cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]

use super::{Foundry, FunctionCall, GenericVisitor, LocalFunction, Storage, Test, WithContents};
use anyhow::Result;
use necessist_core::framework::{SpanTestMaps, TestSet};
use solang_parser::pt::{
    Expression, FunctionAttribute, FunctionDefinition, Identifier, Loc, Mutability, SourceUnit,
    Statement,
};
use std::{cell::RefCell, collections::BTreeMap, convert::Infallible};

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

pub(super) fn collect_local_functions(
    source_unit: &SourceUnit,
) -> BTreeMap<String, Vec<LocalFunction<'_>>> {
    let mut collector = FunctionDefinitionCollector::default();
    collector.visit_source_unit(source_unit).unwrap();
    collector.function_definitions.split_off(&String::new())
}

#[derive(Default)]
struct FunctionDefinitionCollector<'ast> {
    function_definitions: BTreeMap<String, Vec<LocalFunction<'ast>>>,
}

impl<'ast> visit_fns::Visitor<'ast> for FunctionDefinitionCollector<'ast> {
    type Error = Infallible;

    fn visit_function_definition(
        &mut self,
        function_definition: &'ast FunctionDefinition,
    ) -> Result<(), Self::Error> {
        if let Some(name) = &function_definition.name
            && !is_pure(function_definition)
        {
            self.function_definitions
                .entry(name.to_string())
                .or_default()
                .push(LocalFunction {
                    function_definition,
                });
        }
        Ok(())
    }
}

pub(super) fn visit<'ast>(
    generic_visitor: GenericVisitor<'_, '_, '_, 'ast, Foundry>,
    storage: &RefCell<Storage<'ast>>,
    source_unit: &'ast SourceUnit,
) -> Result<(TestSet, SpanTestMaps)> {
    let mut visitor = Visitor::new(generic_visitor, storage);
    visitor.visit_source_unit(source_unit)?;
    while let Some(local_function) = visitor.generic_visitor.next_local_function() {
        visitor.visit_local_function(local_function)?;
    }
    visitor.generic_visitor.results()
}

struct Visitor<'context, 'config, 'backend, 'ast, 'storage> {
    generic_visitor: GenericVisitor<'context, 'config, 'backend, 'ast, Foundry>,
    storage: &'storage RefCell<Storage<'ast>>,
}

impl<'context, 'config, 'backend, 'ast, 'storage>
    Visitor<'context, 'config, 'backend, 'ast, 'storage>
{
    fn new(
        generic_visitor: GenericVisitor<'context, 'config, 'backend, 'ast, Foundry>,
        storage: &'storage RefCell<Storage<'ast>>,
    ) -> Self {
        Self {
            generic_visitor,
            storage,
        }
    }

    fn visit_local_function(&mut self, local_function: LocalFunction<'ast>) -> Result<()> {
        visit_fns::visit_function_definition(self, local_function.function_definition)?;

        Ok(())
    }
}

impl<'ast> visit_fns::Visitor<'ast> for Visitor<'_, '_, '_, 'ast, '_> {
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
    if let Some(Identifier { name, .. }) = &function_definition.name
        && name.starts_with("test")
        && !is_pure(function_definition)
        && let Some(Statement::Block { statements, .. }) = &function_definition.body
    {
        Some(Test {
            name,
            statements: Statements(statements),
        })
    } else {
        None
    }
}

// smoelius: Skip `pure` functions. @smonicas noticed that instrumenting them is a bug because
// `vm.envBytes` is a `view` function. See: https://github.com/trailofbits/necessist/issues/1728
fn is_pure(function_definition: &FunctionDefinition) -> bool {
    function_definition.attributes.iter().any(|attribute| {
        matches!(
            attribute,
            FunctionAttribute::Mutability(Mutability::Pure(_))
        )
    })
}

fn filter_statements<'ast, I: IntoIterator<Item = &'ast Statement>>(
    statements: I,
) -> impl Iterator<Item = &'ast Statement> {
    let mut prev = None;
    statements.into_iter().map(move |statement| {
        // smoelius: If the previous statement was a "prefix cheatcode," then replace the current
        // statement with a `continue`, so that the statement is ignored.
        let next = if prev.is_some_and(is_prefix_cheatcode) {
            &FAUX_CONTINUE
        } else {
            statement
        };
        prev = Some(statement);
        next
    })
}

fn is_prefix_cheatcode(statement: &Statement) -> bool {
    if let Statement::Expression(_, expression) = statement
        && let Expression::FunctionCall(_loc, callee, _args) = expression
        && let Expression::MemberAccess(_, base, Identifier { name: method, .. }) = &**callee
        && let Expression::Variable(variable) = &**base
        && variable.to_string() == "vm"
        && (method == "prank" || method.starts_with("expect"))
    {
        true
    } else {
        false
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
