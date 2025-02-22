use super::{
    AbstractTypes, GenericVisitor, MaybeNamed, Named, ParseLow, ProcessLines, RunLow, Spanned,
    WalkDirResult,
};
use anyhow::{Result, anyhow};
use if_chain::if_chain;
use necessist_core::{
    __Rewriter as Rewriter, LightContext, LineColumn, SourceFile, Span,
    framework::{SpanTestMaps, TestSet},
    util,
};
use solang_parser::pt::{
    CodeLocation, Expression, FunctionDefinition, Identifier, Loc, SourceUnit, Statement,
};
use std::{
    cell::RefCell, collections::BTreeMap, convert::Infallible, fs::read_to_string, hash::Hash,
    path::Path, process::Command,
};

mod storage;
use storage::Storage;

mod visitor;
use visitor::{Statements, collect_local_functions, visit};

#[derive(Debug)]
pub struct Foundry;

impl Foundry {
    pub fn applicable(context: &LightContext) -> Result<bool> {
        context
            .root
            .join("foundry.toml")
            .try_exists()
            .map_err(Into::into)
    }

    pub fn new() -> Self {
        Self
    }
}

#[derive(Clone, Copy)]
pub struct Test<'ast> {
    name: &'ast String,
    statements: Statements<'ast>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct LocalFunction<'ast> {
    function_definition: &'ast FunctionDefinition,
}

impl Hash for LocalFunction<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // smoelius: Hack.
        let bytes = serde_json::to_vec(self.function_definition)
            .expect("failed to serialize function definition");
        bytes.hash(state);
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct WithContents<'ast, T> {
    contents: &'ast str,
    value: T,
}

impl<T: LocWithOptionalSemicolon> Spanned for WithContents<'_, T> {
    fn span(&self, source_file: &SourceFile) -> Span {
        self.value
            .loc_with_optional_semicolon(self.contents)
            .to_internal_span(source_file, self.contents)
    }
}

#[derive(Clone, Copy)]
pub struct MemberAccess<'ast> {
    loc: Loc,
    base: &'ast Expression,
    member: &'ast Identifier,
}

impl CodeLocation for MemberAccess<'_> {
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

impl CodeLocation for FunctionCall<'_> {
    fn loc(&self) -> Loc {
        self.loc
    }
}

pub struct Types;

impl AbstractTypes for Types {
    type Storage<'ast> = Storage<'ast>;
    type File = (String, SourceUnit);
    type Test<'ast> = Test<'ast>;
    type LocalFunction<'ast> = LocalFunction<'ast>;
    type Statement<'ast> = WithContents<'ast, &'ast Statement>;
    type Expression<'ast> = WithContents<'ast, &'ast Expression>;
    type Await<'ast> = Infallible;
    type Field<'ast> = WithContents<'ast, MemberAccess<'ast>>;
    type Call<'ast> = WithContents<'ast, FunctionCall<'ast>>;
    type MacroCall<'ast> = Infallible;
}

impl Named for <Types as AbstractTypes>::Test<'_> {
    fn name(&self) -> String {
        self.name.clone()
    }
}

impl MaybeNamed for <Types as AbstractTypes>::Expression<'_> {
    fn name(&self) -> Option<String> {
        if let Expression::Variable(identifier) = self.value {
            Some(identifier.to_string())
        } else {
            None
        }
    }
}

impl MaybeNamed for <Types as AbstractTypes>::Field<'_> {
    fn name(&self) -> Option<String> {
        Some(self.value.member.to_string())
    }
}

impl MaybeNamed for <Types as AbstractTypes>::Call<'_> {
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
        "console.log*",
        "console2.log*",
        "vm.expect*",
        "vm.getLabel",
        "vm.label",
        // tarunbhm: prank is generally used to call contracts with a privileged account so we want
        // to find tests passing after removing them. However, in case prank is used to
        // call contracts with less privileged account then it results in false positives.
        // Revisit it later.
        // "vm.prank",
        // "vm.startPrank",
        // "vm.stopPrank",
    ]);

    const IGNORED_MACROS: Option<&'static [&'static str]> = None;

    const IGNORED_METHODS: Option<&'static [&'static str]> = Some(&[]);

    fn walk_dir(&self, root: &Path) -> Box<dyn Iterator<Item = WalkDirResult>> {
        Box::new(
            walkdir::WalkDir::new(root.join("test"))
                .into_iter()
                .filter_entry(|entry| {
                    let path = entry.path();
                    !path.is_file() || path.to_string_lossy().ends_with(".t.sol")
                }),
        )
    }

    fn parse_source_file(
        &self,
        source_file: &Path,
    ) -> Result<<Self::Types as AbstractTypes>::File> {
        let contents = read_to_string(source_file)?;
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

    fn local_functions<'ast>(
        &self,
        _storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        file: &'ast <Self::Types as AbstractTypes>::File,
    ) -> Result<BTreeMap<String, Vec<<Self::Types as AbstractTypes>::LocalFunction<'ast>>>> {
        Ok(collect_local_functions(&file.1))
    }

    fn visit_file<'ast>(
        generic_visitor: GenericVisitor<'_, '_, '_, 'ast, Self>,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        file: &'ast <Self::Types as AbstractTypes>::File,
    ) -> Result<(TestSet, SpanTestMaps)> {
        visit(generic_visitor, storage, &file.1)
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

    fn statement_is_removable(
        &self,
        _statement: <Self::Types as AbstractTypes>::Statement<'_>,
    ) -> bool {
        true
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
                .any(|(_, param)| param.as_ref().is_some_and(|param| param.name.is_some()));
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

    fn command_to_run_source_file(&self, context: &LightContext, source_file: &Path) -> Command {
        Self::test_command(context, source_file)
    }

    fn instrument_source_file(
        &self,
        _context: &LightContext,
        _rewriter: &mut Rewriter,
        _source_file: &SourceFile,
        _n_instrumentable_statements: usize,
    ) -> Result<()> {
        Ok(())
    }

    fn statement_prefix_and_suffix(&self, span: &Span) -> Result<(String, String)> {
        Ok((
            format!(
                r#"if (bytes8(vm.envBytes("NECESSIST_REMOVAL")) != bytes8(hex"{}")) {{ "#,
                span.id()
            ),
            " }".to_owned(),
        ))
    }

    fn command_to_build_source_file(&self, context: &LightContext, _source_file: &Path) -> Command {
        let mut command = Command::new("forge");
        command.current_dir(context.root.as_path());
        command.arg("build");
        command
    }

    // smoelius: If the user specifies additional arguments to pass to the test command, Necessist
    // passes them to the build command as well. This causes problems when the test command accepts
    // arguments that the build command doesn't. A workaround is to use, for the "build" command, a
    // test command that runs exactly zero tests.
    // smoelius: Recent versions of Foundry output `Error: No tests to run` and exit with a non-zero
    // status. So just return `forge build` like `command_to_build_source_file` above.
    fn command_to_build_test(
        &self,
        context: &LightContext,
        _test_name: &str,
        _span: &Span,
    ) -> Command {
        let mut command = Command::new("forge");
        command.current_dir(context.root.as_path());
        command.arg("build");
        command
    }

    fn command_to_run_test(
        &self,
        context: &LightContext,
        test_name: &str,
        span: &Span,
    ) -> (Command, Vec<String>, Option<ProcessLines>) {
        let mut command = Self::test_command(context, &span.source_file);
        command.args(["--match-test", test_name]);

        let pat = format!(" {test_name}(");

        (
            command,
            Vec::new(),
            Some((false, Box::new(move |line| line.contains(&pat)))),
        )
    }
}

impl Foundry {
    fn test_command(context: &LightContext, source_file: &Path) -> Command {
        let mut command = Command::new("forge");
        command.current_dir(context.root.as_path());
        command.env("FOUNDRY_FUZZ_RUNS", "1");
        command.args([
            "test",
            "--match-path",
            &util::strip_prefix(source_file, context.root)
                .unwrap()
                .to_string_lossy(),
        ]);
        command
    }
}

trait LocWithOptionalSemicolon: CodeLocation {
    fn loc_with_optional_semicolon(&self, _contents: &str) -> Loc {
        self.loc()
    }
}

impl<T: LocWithOptionalSemicolon> LocWithOptionalSemicolon for &T {
    fn loc_with_optional_semicolon(&self, contents: &str) -> Loc {
        (*self).loc_with_optional_semicolon(contents)
    }
}

impl LocWithOptionalSemicolon for Expression {}

impl LocWithOptionalSemicolon for MemberAccess<'_> {}

impl LocWithOptionalSemicolon for FunctionCall<'_> {}

impl LocWithOptionalSemicolon for Statement {
    fn loc_with_optional_semicolon(&self, contents: &str) -> Loc {
        let loc = self.loc();
        match loc {
            Loc::File(file_no, start, mut end) => {
                let mut chars = contents.chars().skip(end).peekable();
                while chars.peek().copied().is_some_and(char::is_whitespace) {
                    end += 1;
                    let _: Option<char> = chars.next();
                }
                if chars.next() == Some(';') {
                    Loc::File(file_no, start, end + 1)
                } else {
                    loc
                }
            }
            _ => loc,
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

#[cfg(test)]
mod test {
    use cargo_metadata::{MetadataCommand, Package};
    use std::{
        fs::read_to_string,
        io::{Error, Write},
        path::{Path, PathBuf},
        process::{Command, Stdio},
    };
    use tempfile::tempdir;

    const PT_RS: &str = "src/pt.rs";

    #[test]
    fn check_pt_rs() {
        let tempdir = tempdir().unwrap();

        let metadata = MetadataCommand::new().exec().unwrap();
        let package = metadata
            .packages
            .iter()
            .find(|package| package.name == "solang-parser")
            .unwrap();
        let solang_dir = download_package(tempdir.path(), package).unwrap();
        let pt_rs = solang_dir.join(PT_RS);
        let expected = read_to_string(pt_rs).unwrap();
        let actual = read_to_string("assets/solang_parser_pt.rs").unwrap();
        assert_eq!(expected, actual);
    }

    fn download_package(out_dir: &Path, package: &Package) -> Result<PathBuf, Error> {
        let download = get(&format!(
            "https://crates.io/api/v1/crates/{}/{}/download",
            package.name, package.version
        ))?;

        let mut child = Command::new("tar")
            .current_dir(out_dir)
            .args(["xzf", "-"])
            .stdin(Stdio::piped())
            .spawn()?;
        {
            let child_stdin = child.stdin.as_mut().unwrap();
            child_stdin.write_all(&download)?;
        }
        let output = child.wait_with_output()?;
        assert!(output.status.success(), "{output:#?}");

        Ok(out_dir.join(format!("{}-{}", package.name, package.version)))
    }

    fn get(url: &str) -> Result<Vec<u8>, curl::Error> {
        let mut data = Vec::new();
        let mut handle = curl::easy::Easy::new();
        handle.follow_location(true)?;
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
