use super::{
    AbstractTypes, GenericVisitor, MaybeNamed, Named, ParseLow, ProcessLines, RunLow, Spanned,
    WalkDirResult,
};
use anyhow::{anyhow, Result};
use if_chain::if_chain;
use necessist_core::{util, LightContext, LineColumn, SourceFile, Span};
use solang_parser::pt::{CodeLocation, Expression, Identifier, Loc, SourceUnit, Statement};
use std::{
    cell::RefCell, collections::BTreeMap, convert::Infallible, fs::read_to_string, path::Path,
    process::Command,
};

mod storage;
use storage::Storage;

#[cfg_attr(
    dylint_lib = "non_local_effect_before_error_return",
    allow(non_local_effect_before_error_return)
)]
mod visitor;
use visitor::{visit, Statements};

#[derive(Debug)]
pub struct Foundry {
    span_test_name_map: BTreeMap<Span, String>,
}

impl Foundry {
    pub fn applicable(context: &LightContext) -> Result<bool> {
        context
            .root
            .join("foundry.toml")
            .try_exists()
            .map_err(Into::into)
    }

    pub fn new() -> Self {
        Self {
            span_test_name_map: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Copy)]
pub struct Test<'ast> {
    name: &'ast String,
    statements: Statements<'ast>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct WithContents<'ast, T> {
    contents: &'ast str,
    value: T,
}

impl<'ast, T: CodeLocation> Spanned for WithContents<'ast, T> {
    fn span(&self, source_file: &SourceFile) -> Span {
        // smoelius: Calling `extend_to_semicolon` for things other than statements is hacky
        // but... ¯\_(ツ)_/¯
        self.value
            .loc()
            .extend_to_semicolon(self.contents)
            .to_internal_span(source_file, self.contents)
    }
}

#[derive(Clone, Copy)]
pub struct MemberAccess<'ast> {
    loc: Loc,
    base: &'ast Expression,
    member: &'ast Identifier,
}

impl<'ast> CodeLocation for MemberAccess<'ast> {
    fn loc(&self) -> Loc {
        self.loc
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
pub struct FunctionCall<'ast> {
    loc: Loc,
    callee: &'ast Expression,
    args: &'ast Vec<Expression>,
}

impl<'ast> CodeLocation for FunctionCall<'ast> {
    fn loc(&self) -> Loc {
        self.loc
    }
}

pub struct Types;

impl AbstractTypes for Types {
    type Storage<'ast> = Storage<'ast>;
    type File = (String, SourceUnit);
    type Test<'ast> = Test<'ast>;
    type Statement<'ast> = WithContents<'ast, &'ast Statement>;
    type Expression<'ast> = WithContents<'ast, &'ast Expression>;
    type Await<'ast> = Infallible;
    type Field<'ast> = WithContents<'ast, MemberAccess<'ast>>;
    type Call<'ast> = WithContents<'ast, FunctionCall<'ast>>;
    type MacroCall<'ast> = Infallible;
}

impl<'ast> Named for <Types as AbstractTypes>::Test<'ast> {
    fn name(&self) -> String {
        self.name.clone()
    }
}

impl<'ast> MaybeNamed for <Types as AbstractTypes>::Expression<'ast> {
    fn name(&self) -> Option<String> {
        if let Expression::Variable(identifier) = self.value {
            Some(identifier.to_string())
        } else {
            None
        }
    }
}

impl<'ast> MaybeNamed for <Types as AbstractTypes>::Field<'ast> {
    fn name(&self) -> Option<String> {
        Some(self.value.member.to_string())
    }
}

impl<'ast> MaybeNamed for <Types as AbstractTypes>::Call<'ast> {
    fn name(&self) -> Option<String> {
        if let Expression::Variable(identifier) = self.value.callee {
            Some(identifier.to_string())
        } else {
            None
        }
    }
}

impl ParseLow for Foundry {
    type Types = Types;

    const IGNORED_FUNCTIONS: Option<&'static [&'static str]> = Some(&[
        "assert*",
        "vm.expectEmit",
        "vm.expectRevert",
        "vm.prank",
        "vm.startPrank",
        "vm.stopPrank",
    ]);

    const IGNORED_MACROS: Option<&'static [&'static str]> = None;

    const IGNORED_METHODS: Option<&'static [&'static str]> = Some(&[]);

    fn walk_dir(root: &Path) -> Box<dyn Iterator<Item = WalkDirResult>> {
        Box::new(
            walkdir::WalkDir::new(root.join("test"))
                .into_iter()
                .filter_entry(|entry| {
                    let path = entry.path();
                    !path.is_file() || path.to_string_lossy().ends_with(".t.sol")
                }),
        )
    }

    fn parse_file(&self, test_file: &Path) -> Result<<Self::Types as AbstractTypes>::File> {
        let contents = read_to_string(test_file)?;
        solang_parser::parse(&contents, 0)
            .map(|(source_unit, _)| (contents, source_unit))
            .map_err(|error| anyhow!(format!("{error:?}")))
    }

    fn storage_from_file<'ast>(
        &self,
        file: &'ast <Self::Types as AbstractTypes>::File,
    ) -> <Self::Types as AbstractTypes>::Storage<'ast> {
        Storage::new(file)
    }

    fn visit_file<'ast>(
        generic_visitor: GenericVisitor<'_, '_, '_, 'ast, Self>,
        storage: &std::cell::RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        file: &'ast <Self::Types as AbstractTypes>::File,
    ) -> Result<Vec<Span>> {
        visit(generic_visitor, storage, &file.1)
    }

    fn on_candidate_found(
        &mut self,
        _context: &LightContext,
        _storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'_>>,
        test_name: &str,
        span: &Span,
    ) {
        self.set_span_test_name(span, test_name);
    }

    fn test_statements<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        test: <Self::Types as AbstractTypes>::Test<'ast>,
    ) -> Vec<<Self::Types as AbstractTypes>::Statement<'ast>> {
        test.statements
            .get()
            .map(|statement| WithContents {
                contents: storage.borrow().contents,
                value: statement,
            })
            .collect()
    }

    fn statement_is_expression<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        statement: <Self::Types as AbstractTypes>::Statement<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Expression<'ast>> {
        if let Statement::Expression(_, expression) = statement.value {
            Some(WithContents {
                contents: storage.borrow().contents,
                value: expression,
            })
        } else {
            None
        }
    }

    fn statement_is_control<'ast>(
        &self,
        _storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        statement: <Self::Types as AbstractTypes>::Statement<'ast>,
    ) -> bool {
        // smoelius: An `emit` is not technically a "control" statement, but we need some way to
        // ignore it. This approach is good enough for now.
        matches!(
            statement.value,
            Statement::Break(..)
                | Statement::Continue(..)
                | Statement::Emit(..)
                | Statement::Return(..)
                | Statement::Revert(..)
                | Statement::RevertNamedArgs(..)
        )
    }

    fn statement_is_declaration<'ast>(
        &self,
        _storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        statement: <Self::Types as AbstractTypes>::Statement<'ast>,
    ) -> bool {
        if matches!(statement.value, Statement::VariableDefinition(..)) {
            return true;
        }

        if_chain! {
            if let Statement::Expression(_, expression) = statement.value;
            if let Expression::Assign(_, lhs, _) = expression;
            if let Expression::List(_, params) = &**lhs;
            // smoelius: My current belief is: a multiple assignment (i.e., not a declaration) uses
            // the the `Param`s `ty` fields to hold the variables being assigned to.
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

    fn expression_is_await<'ast>(
        &self,
        _storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        _expression: <Self::Types as AbstractTypes>::Expression<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Await<'ast>> {
        None
    }

    fn expression_is_field<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        expression: <Self::Types as AbstractTypes>::Expression<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Field<'ast>> {
        if let Expression::MemberAccess(loc, base, member) = expression.value {
            Some(WithContents {
                contents: storage.borrow().contents,
                value: MemberAccess {
                    loc: *loc,
                    base,
                    member,
                },
            })
        } else {
            None
        }
    }

    fn expression_is_call<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        expression: <Self::Types as AbstractTypes>::Expression<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Call<'ast>> {
        if let Expression::FunctionCall(loc, callee, args) = expression.value {
            Some(WithContents {
                contents: storage.borrow().contents,
                value: FunctionCall {
                    loc: *loc,
                    callee,
                    args,
                },
            })
        } else {
            None
        }
    }

    fn expression_is_macro_call<'ast>(
        &self,
        _storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        _expression: <Self::Types as AbstractTypes>::Expression<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::MacroCall<'ast>> {
        None
    }

    fn await_arg<'ast>(
        &self,
        _storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        _await: <Self::Types as AbstractTypes>::Await<'ast>,
    ) -> <Self::Types as AbstractTypes>::Expression<'ast> {
        unreachable!()
    }

    fn field_base<'ast>(
        &self,
        _storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        field: <Self::Types as AbstractTypes>::Field<'ast>,
    ) -> <Self::Types as AbstractTypes>::Expression<'ast> {
        WithContents {
            contents: field.contents,
            value: field.value.base,
        }
    }

    fn call_callee<'ast>(
        &self,
        _storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        call: <Self::Types as AbstractTypes>::Call<'ast>,
    ) -> <Self::Types as AbstractTypes>::Expression<'ast> {
        WithContents {
            contents: call.contents,
            value: call.value.callee,
        }
    }

    fn macro_call_callee<'ast>(
        &self,
        _storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        _macro_call: <Self::Types as AbstractTypes>::MacroCall<'ast>,
    ) -> <Self::Types as AbstractTypes>::Expression<'ast> {
        unreachable!()
    }
}

impl RunLow for Foundry {
    const REQUIRES_NODE_MODULES: bool = true;

    fn command_to_run_test_file(&self, context: &LightContext, test_file: &Path) -> Command {
        Self::test_command(context, test_file)
    }

    fn command_to_build_test(&self, context: &LightContext, _span: &Span) -> Command {
        let mut command = Command::new("forge");
        command.current_dir(context.root.as_path());
        command.arg("build");
        command
    }

    fn command_to_run_test(
        &self,
        context: &LightContext,
        span: &Span,
    ) -> (Command, Vec<String>, Option<(ProcessLines, String)>) {
        #[allow(clippy::expect_used)]
        let test_name = self
            .span_test_name_map
            .get(span)
            .cloned()
            .expect("Test ident is not set");

        let mut command = Self::test_command(context, &span.source_file);
        command.args(["--match-test", &test_name]);

        (
            command,
            Vec::new(),
            Some((
                (
                    true,
                    Box::new(|line| !line.starts_with("No tests match the provided pattern")),
                ),
                test_name,
            )),
        )
    }
}

impl Foundry {
    fn test_command(context: &LightContext, test_file: &Path) -> Command {
        let mut command = Command::new("forge");
        command.current_dir(context.root.as_path());
        command.env("FOUNDRY_FUZZ_RUNS", "1");
        command.args([
            "test",
            "--match-path",
            &util::strip_prefix(test_file, context.root)
                .unwrap()
                .to_string_lossy(),
        ]);
        command
    }

    fn set_span_test_name(&mut self, span: &Span, name: &str) {
        self.span_test_name_map
            .insert(span.clone(), name.to_owned());
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

    let line_no = line_starts.partition_point(|&line_start| loc >= line_start);

    let col_no = if line_no > 0 {
        loc - line_starts[line_no - 1]
    } else {
        loc
    };

    (line_no + 1, col_no)
}
