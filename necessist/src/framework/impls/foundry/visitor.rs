use super::{
    visit::{self as visit_fns, Visitor as _},
    Foundry,
};
use if_chain::if_chain;
use necessist_core::{LineColumn, SourceFile, Span};
use solang_parser::pt::{
    CodeLocation, Expression, FunctionDefinition, Identifier, Loc, SourceUnit, Statement,
};
use std::{
    path::{Path, PathBuf},
    rc::Rc,
};
use thiserror::Error;

#[derive(Error, Debug)]
#[error(transparent)]
struct Error(anyhow::Error);

pub(super) fn visit<'framework>(
    framework: &'framework mut Foundry,
    root: Rc<PathBuf>,
    test_file: &Path,
    contents: &str,
    source_unit: &mut SourceUnit,
) -> Vec<Span> {
    let mut visitor = Visitor::new(framework, root, test_file, contents);
    visitor.visit_source_unit(source_unit).unwrap();
    visitor.spans
}

struct Visitor<'ast, 'contents, 'framework> {
    framework: &'framework mut Foundry,
    source_file: SourceFile,
    contents: &'contents str,
    test_name: Option<&'ast str>,
    n_stmt_leaves_visited: usize,
    spans: Vec<Span>,
}

#[allow(dead_code)]
struct MethodCall<'a> {
    pub loc: Loc,
    pub obj: &'a Expression,
    pub ident: &'a Identifier,
    pub args: &'a Vec<Expression>,
}

impl<'ast, 'contents, 'framework> visit_fns::Visitor<'ast>
    for Visitor<'ast, 'contents, 'framework>
{
    type Error = Error;

    fn visit_function_definition(
        &mut self,
        func: &'ast FunctionDefinition,
    ) -> Result<(), Self::Error> {
        if let Some(name) = is_test_function(func) {
            assert!(self.test_name.is_none());
            self.test_name = Some(name);

            if let Some(Statement::Block { statements, .. }) = &func.body {
                // smoelius: Skip the last statement.
                self.visit_statements(
                    statements
                        .split_last()
                        .map_or(&[] as &[Statement], |(_, stmts)| stmts),
                )?;
            }

            assert!(self.test_name == Some(name));
            self.test_name = None;
        }

        Ok(())
    }

    fn visit_statement(&mut self, stmt: &'ast Statement) -> Result<(), Error> {
        let n_before = self.n_stmt_leaves_visited;
        if let Statement::Block { statements, .. } = stmt {
            self.visit_statements(statements)?;
        } else {
            visit_fns::visit_statement(self, stmt)?;
        }
        let n_after = self.n_stmt_leaves_visited;

        // smoelius: Consider this a "leaf" if-and-only-if no "leaves" were added during the
        // recursive call.
        if n_before != n_after {
            return Ok(());
        }
        self.n_stmt_leaves_visited += 1;

        if let Some(ident) = self.test_name {
            if !is_variable_definition(stmt)
                && !is_control(stmt)
                && !matches!(stmt, Statement::Emit(..))
                && !is_ignored_function_call_statement(stmt)
                && !is_method_call_statement(stmt)
            {
                let span = stmt
                    .loc()
                    .extend_to_semicolon(self.contents)
                    .to_internal_span(&self.source_file, self.contents);
                self.elevate_span(span, ident);
            }
        }

        Ok(())
    }

    fn visit_expression(&mut self, expr: &'ast Expression) -> Result<(), Self::Error> {
        visit_fns::visit_expression(self, expr)?;

        if_chain! {
            if let Some(name) = self.test_name;
            if let Some(MethodCall {
                loc, obj, ident, ..
            }) = is_method_call(expr);
            if !is_ignored_method(ident);
            then {
                let mut span = loc.to_internal_span(&self.source_file, self.contents);
                span.start = obj
                    .loc()
                    .to_internal_span(&self.source_file, self.contents)
                    .end;
                assert!(span.start <= span.end);
                self.elevate_span(span, name);
            }
        }

        Ok(())
    }
}

impl<'ast, 'contents, 'framework> Visitor<'ast, 'contents, 'framework> {
    fn new(
        framework: &'framework mut Foundry,
        root: Rc<PathBuf>,
        test_file: &Path,
        contents: &'contents str,
    ) -> Self {
        Self {
            framework,
            source_file: SourceFile::new(root, Rc::new(test_file.to_path_buf())),
            contents,
            test_name: None,
            n_stmt_leaves_visited: 0,
            spans: Vec::new(),
        }
    }

    fn visit_statements<I: IntoIterator<Item = &'ast Statement>>(
        &mut self,
        stmts: I,
    ) -> Result<(), Error> {
        let mut prev = None;
        for stmt in stmts {
            if prev.map_or(false, is_prefix_cheatcode) {
                continue;
            }
            self.visit_statement(stmt)?;
            prev = Some(stmt);
        }
        Ok(())
    }

    fn elevate_span(&mut self, span: Span, name: &str) {
        self.framework.set_span_test_name(&span, name);
        self.spans.push(span);
    }
}

fn is_test_function(func: &FunctionDefinition) -> Option<&str> {
    if_chain! {
        if let Some(Identifier { name, .. }) = &func.name;
        if name.starts_with("test");
        then {
            Some(name)
        } else {
            None
        }
    }
}

fn is_variable_definition(stmt: &Statement) -> bool {
    if matches!(stmt, Statement::VariableDefinition(..)) {
        return true;
    }

    if_chain! {
        if let Statement::Expression(_, expr) = stmt;
        if let Expression::Assign(_, lhs, _) = expr;
        if let Expression::List(_, params) = &**lhs;
        // smoelius: My current belief is: a multiple assignment (i.e., not a declaration) uses the
        // the `Param`s `ty` fields to hold the variables being assigned to.
        if params
            .iter()
            .any(|(_, param)| param.as_ref().map_or(false, |param| param.name.is_some()));
        then {
            true
        } else {
            false
        }
    }
}

fn is_control(stmt: &Statement) -> bool {
    matches!(
        stmt,
        Statement::Break(..)
            | Statement::Continue(..)
            | Statement::Return(..)
            | Statement::Revert(..)
            | Statement::RevertNamedArgs(..)
    )
}

fn is_ignored_function_call_statement(stmt: &Statement) -> bool {
    if_chain! {
        if let Statement::Expression(_, expr) = stmt;
        if let Expression::FunctionCall(_, func, _) = expr;
        if let Expression::Variable(ident) = &**func;
        if is_ignored_function(ident);
        then {
            true
        } else {
            false
        }
    }
}

fn is_ignored_function(ident: &Identifier) -> bool {
    ident.to_string().starts_with("assert")
}

fn is_method_call_statement(stmt: &Statement) -> bool {
    if_chain! {
        if let Statement::Expression(_, expr) = stmt;
        if is_method_call(expr).is_some();
        then {
            true
        } else {
            false
        }
    }
}

fn is_method_call(expr: &Expression) -> Option<MethodCall> {
    if_chain! {
        if let Expression::FunctionCall(loc, callee, args) = expr;
        if let Expression::MemberAccess(_, obj, ident) = &**callee;
        then {
            Some(MethodCall {
                loc: *loc,
                obj,
                ident,
                args,
            })
        } else {
            None
        }
    }
}

const IGNORED_METHODS: &[&str] = &[
    "expectEmit",
    "expectRevert",
    "prank",
    "startPrank",
    "stopPrank",
];

fn is_ignored_method(ident: &Identifier) -> bool {
    IGNORED_METHODS
        .binary_search(&ident.to_string().as_ref())
        .is_ok()
}

fn is_prefix_cheatcode(stmt: &Statement) -> bool {
    if_chain! {
        if let Statement::Expression(_, expr) = stmt;
        if let Some(MethodCall {
            obj,
            ident: Identifier {
                name: method, ..
            },
            ..
        }) = is_method_call(expr);
        if let Expression::Variable(var) = obj;
        if var.to_string() == "vm";
        if method == "prank" || method.starts_with("expect");
        then {
            true
        } else {
            false
        }
    }
}

trait ExtendToSemicolon {
    fn extend_to_semicolon(&self, contents: &str) -> Self;
}

impl ExtendToSemicolon for Loc {
    fn extend_to_semicolon(&self, contents: &str) -> Self {
        match *self {
            Self::File(file_no, start, mut end) => {
                let mut chars = contents.chars().skip(end).peekable();
                while chars.peek().map_or(false, |c| c.is_whitespace()) {
                    end += 1;
                    let _ = chars.next();
                }
                if chars.next() == Some(';') {
                    Self::File(file_no, start, end + 1)
                } else {
                    *self
                }
            }
            _ => *self,
        }
    }
}

trait ToInternalSpan {
    fn to_internal_span(&self, source_file: &SourceFile, contents: &str) -> Span;
}

impl ToInternalSpan for Loc {
    fn to_internal_span(&self, source_file: &SourceFile, contents: &str) -> Span {
        Span {
            source_file: source_file.clone(),
            start: self.start().to_line_column(contents),
            end: self.end().to_line_column(contents),
        }
    }
}

trait ToLineColumn {
    fn to_line_column(&self, contents: &str) -> LineColumn;
}

impl ToLineColumn for usize {
    fn to_line_column(&self, contents: &str) -> LineColumn {
        let (line, column) = offset_to_line_column(contents, *self);
        LineColumn { line, column }
    }
}

// smoelius: `offset_to_line_column` is based on code from:
// https://github.com/hyperledger/solang/blob/be2e03043232ca84fe05375e22fda97139cb1619/src/sema/file.rs#L8-L64

/// Convert an offset to line (one based) and column number (zero based)
pub fn offset_to_line_column(contents: &str, loc: usize) -> (usize, usize) {
    let mut line_starts = Vec::new();

    for (ind, c) in contents.char_indices() {
        if c == '\n' {
            line_starts.push(ind + 1);
        }
    }

    let line_no = line_starts.partition_point(|line_start| loc >= *line_start);

    let col_no = if line_no > 0 {
        loc - line_starts[line_no - 1]
    } else {
        loc
    };

    (line_no + 1, col_no)
}

#[cfg(test)]
mod test {
    use super::IGNORED_METHODS;

    #[test]
    fn ignored_methods_are_sorted() {
        assert_eq!(sort(IGNORED_METHODS), IGNORED_METHODS);
    }

    fn sort<'a>(items: &'a [&str]) -> Vec<&'a str> {
        let mut items = items.to_vec();
        items.sort_unstable();
        items
    }
}
