use super::{
    AbstractTypes, DirectiveSyntax, GenericVisitor, MaybeNamed, Named, ParseLow, ProcessLines,
    RunLow, Spanned, WalkDirResult,
};
use anyhow::{Context, Result, anyhow};
use elaborate::std::{
    ffi::OsStrContext, fs::read_to_string_wc, path::PathContext, process::CommandContext,
};
use necessist_core::{
    LightContext, LineColumn, SourceFile, Span,
    framework::{SpanTestMaps, TestSet},
    util,
};
use ruff_python_ast::{
    AnyNodeRef, Expr, ExprAttribute, ExprAwait, ExprCall, ExprRef, ModModule, Stmt,
    StmtFunctionDef,
    token::{Tokens, parenthesized_range},
};
use ruff_python_parser::{Parsed, parse_module};
use ruff_source_file::LineIndex;
use ruff_text_size::{Ranged, TextRange};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    convert::Infallible,
    hash::{Hash, Hasher},
    path::Path,
    process::{Command, Stdio},
    sync::RwLock,
};

mod visitor;
use visitor::{collect_local_functions, visit};

#[derive(Debug)]
pub struct Python {
    executable: &'static str,
}

impl Python {
    pub fn applicable(context: &LightContext) -> Result<bool> {
        if context.root.join("pytest.ini").try_exists_wc()? {
            return Ok(true);
        }
        Ok(Self::walk(context.root.as_path())
            .any(|entry| entry.is_ok_and(|entry| entry.path().is_file())))
    }

    pub fn new() -> Result<Self> {
        let executable = python_executable(command_is_available).ok_or_else(|| {
            anyhow!("could not find a Python interpreter (`python` or `python3`)")
        })?;
        Ok(Self { executable })
    }

    fn walk(root: &Path) -> impl Iterator<Item = WalkDirResult> + use<> {
        ignore::WalkBuilder::new(root)
            .filter_entry(|entry| {
                let path = entry.path();
                !path.is_file() || is_test_file(path)
            })
            .build()
    }
}

fn is_test_file(path: &Path) -> bool {
    let Ok(stem) = path.file_stem_wc().and_then(OsStrContext::to_str_wc) else {
        return false;
    };
    (stem.starts_with("test_") || stem.ends_with("_test"))
        && path
            .extension_wc()
            .is_ok_and(|extension| extension.eq_ignore_ascii_case("py"))
}

pub struct Storage<'ast> {
    tokens: &'ast Tokens,
    line_index: &'ast LineIndex,
    classes: Vec<String>,
}

pub struct File {
    parsed: Parsed<ModModule>,
    line_index: LineIndex,
}

#[derive(Clone, Copy)]
pub struct Test<'ast> {
    test_path_id: usize,
    function: &'ast StmtFunctionDef,
}

impl<'ast> Test<'ast> {
    fn new(classes: &[String], function: &'ast StmtFunctionDef) -> Self {
        let test_path = classes
            .iter()
            .map(String::as_str)
            .chain(std::iter::once(function.name.as_str()))
            .map(str::to_owned)
            .collect();
        let test_path_id = reserve_test_path_id(test_path);
        Self {
            test_path_id,
            function,
        }
    }
}

static TEST_PATHS: RwLock<Vec<Vec<String>>> = RwLock::new(Vec::new());

fn reserve_test_path_id(test_path: Vec<String>) -> usize {
    let mut test_paths = TEST_PATHS.write().unwrap();
    test_paths.push(test_path);
    test_paths.len() - 1
}

impl Named for Test<'_> {
    fn name(&self) -> String {
        let test_paths = TEST_PATHS.read().unwrap();
        test_paths[self.test_path_id].join("::")
    }
}

#[derive(Clone, Copy)]
pub struct LocalFunction<'ast>(&'ast StmtFunctionDef);

impl PartialEq for LocalFunction<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}

impl Eq for LocalFunction<'_> {}

impl Hash for LocalFunction<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::ptr::from_ref(self.0).hash(state);
    }
}

#[derive(Clone, Copy)]
pub struct Statement<'ast> {
    tokens: &'ast Tokens,
    line_index: &'ast LineIndex,
    statement: &'ast Stmt,
}

impl PartialEq for Statement<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.statement, other.statement)
    }
}

impl Eq for Statement<'_> {}

#[derive(Clone, Copy)]
pub struct Expression<'ast> {
    tokens: &'ast Tokens,
    line_index: &'ast LineIndex,
    expression: &'ast Expr,
    range: Option<TextRange>,
}

#[derive(Clone, Copy)]
pub struct Field<'ast> {
    tokens: &'ast Tokens,
    line_index: &'ast LineIndex,
    field: &'ast ExprAttribute,
}

#[derive(Clone, Copy)]
pub struct Call<'ast> {
    tokens: &'ast Tokens,
    line_index: &'ast LineIndex,
    call: &'ast ExprCall,
}

pub struct Types;

impl AbstractTypes for Types {
    type Storage<'ast> = Storage<'ast>;
    type File = File;
    type Test<'ast> = Test<'ast>;
    type LocalFunction<'ast> = LocalFunction<'ast>;
    type Statement<'ast> = Statement<'ast>;
    type Expression<'ast> = Expression<'ast>;
    type Await<'ast> = &'ast ExprAwait;
    type Field<'ast> = Field<'ast>;
    type Call<'ast> = Call<'ast>;
    type MacroCall<'ast> = Infallible;
}

fn span(range: TextRange, source_file: &SourceFile, line_index: &LineIndex) -> Span {
    let contents = source_file.contents();
    let start = line_index.line_column(range.start(), contents);
    let end = line_index.line_column(range.end(), contents);
    Span {
        source_file: source_file.clone(),
        start: LineColumn {
            line: start.line.get(),
            column: start.column.get() - 1,
        },
        end: LineColumn {
            line: end.line.get(),
            column: end.column.get() - 1,
        },
    }
}

impl Spanned for Statement<'_> {
    fn span(&self, source_file: &SourceFile) -> Span {
        span(self.statement.range(), source_file, self.line_index)
    }
}

impl Spanned for Expression<'_> {
    fn span(&self, source_file: &SourceFile) -> Span {
        span(
            self.range.unwrap_or_else(|| self.expression.range()),
            source_file,
            self.line_index,
        )
    }
}

impl MaybeNamed for Expression<'_> {
    fn name(&self) -> Option<String> {
        match self.expression {
            Expr::Name(name) => Some(name.id.to_string()),
            _ => None,
        }
    }
}

impl Spanned for Field<'_> {
    fn span(&self, source_file: &SourceFile) -> Span {
        span(self.field.range, source_file, self.line_index)
    }
}

impl MaybeNamed for Field<'_> {
    fn name(&self) -> Option<String> {
        Some(self.field.attr.to_string())
    }
}

impl Spanned for Call<'_> {
    fn span(&self, source_file: &SourceFile) -> Span {
        span(self.call.range, source_file, self.line_index)
    }
}

impl MaybeNamed for Call<'_> {
    fn name(&self) -> Option<String> {
        Expression {
            tokens: self.tokens,
            line_index: self.line_index,
            expression: &self.call.func,
            range: None,
        }
        .name()
    }
}

impl ParseLow for Python {
    type Types = Types;

    const DIRECTIVE_SYNTAX: DirectiveSyntax = DirectiveSyntax::Python;
    const IGNORED_FUNCTIONS: Option<&'static [&'static str]> = Some(&["print"]);
    const IGNORED_MACROS: Option<&'static [&'static str]> = None;
    const IGNORED_METHODS: Option<&'static [&'static str]> = Some(&["assert*", "fail"]);

    fn walk_dir(&self, root: &Path) -> Box<dyn Iterator<Item = WalkDirResult>> {
        Box::new(Self::walk(root))
    }

    fn parse_source_file(&self, source_file: &Path) -> Result<File> {
        let text = read_to_string_wc(source_file)?;
        let parsed = parse_module(&text)
            .with_context(|| format!("failed to parse `{}`", source_file.display()))?;
        let line_index = LineIndex::from_source_text(&text);
        Ok(File { parsed, line_index })
    }

    fn storage_from_file<'ast>(&self, file: &'ast File) -> Storage<'ast> {
        Storage {
            tokens: file.parsed.tokens(),
            line_index: &file.line_index,
            classes: Vec::new(),
        }
    }

    fn local_functions<'ast>(
        &self,
        _storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        file: &'ast File,
    ) -> Result<BTreeMap<String, Vec<LocalFunction<'ast>>>> {
        Ok(collect_local_functions(file.parsed.syntax()))
    }

    fn visit_file<'ast>(
        generic_visitor: GenericVisitor<'_, '_, '_, 'ast, Self>,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        file: &'ast File,
    ) -> Result<(TestSet, SpanTestMaps)> {
        visit(generic_visitor, storage, file.parsed.syntax())
    }

    fn test_statements<'ast>(
        &self,
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        test: <Self::Types as AbstractTypes>::Test<'ast>,
    ) -> Vec<<Self::Types as AbstractTypes>::Statement<'ast>> {
        let storage = storage.borrow();
        test.function
            .body
            .iter()
            .map(|statement| Statement {
                tokens: storage.tokens,
                line_index: storage.line_index,
                statement,
            })
            .collect()
    }

    fn statement_is_removable(&self, statement: Statement<'_>) -> bool {
        !matches!(
            statement.statement,
            Stmt::Pass(_) | Stmt::IpyEscapeCommand(_)
        )
    }

    fn statement_is_expression<'ast>(
        &self,
        _storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        statement: <Self::Types as AbstractTypes>::Statement<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Expression<'ast>> {
        match statement.statement {
            Stmt::Expr(expr) => Some(Expression {
                tokens: statement.tokens,
                line_index: statement.line_index,
                expression: &expr.value,
                range: None,
            }),
            _ => None,
        }
    }

    fn statement_is_control<'ast>(
        &self,
        _storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        statement: Statement<'ast>,
    ) -> bool {
        matches!(
            statement.statement,
            Stmt::Return(_) | Stmt::Raise(_) | Stmt::Assert(_) | Stmt::Break(_) | Stmt::Continue(_)
        )
    }

    fn statement_is_declaration<'ast>(
        &self,
        _storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        statement: Statement<'ast>,
    ) -> bool {
        matches!(
            statement.statement,
            Stmt::FunctionDef(_)
                | Stmt::ClassDef(_)
                | Stmt::TypeAlias(_)
                | Stmt::Assign(_)
                | Stmt::AnnAssign(_)
                | Stmt::Import(_)
                | Stmt::ImportFrom(_)
                | Stmt::Global(_)
                | Stmt::Nonlocal(_)
        )
    }

    fn expression_is_await<'ast>(
        &self,
        _storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        expression: <Self::Types as AbstractTypes>::Expression<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Await<'ast>> {
        match expression.expression {
            Expr::Await(await_) => Some(await_),
            _ => None,
        }
    }

    fn expression_is_field<'ast>(
        &self,
        _storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        expression: <Self::Types as AbstractTypes>::Expression<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Field<'ast>> {
        match expression.expression {
            Expr::Attribute(attribute) => Some(Field {
                tokens: expression.tokens,
                line_index: expression.line_index,
                field: attribute,
            }),
            _ => None,
        }
    }

    fn expression_is_call<'ast>(
        &self,
        _storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        expression: <Self::Types as AbstractTypes>::Expression<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Call<'ast>> {
        match expression.expression {
            Expr::Call(call) => Some(Call {
                tokens: expression.tokens,
                line_index: expression.line_index,
                call,
            }),
            _ => None,
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
        storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        await_: <Self::Types as AbstractTypes>::Await<'ast>,
    ) -> <Self::Types as AbstractTypes>::Expression<'ast> {
        let storage = storage.borrow();
        Expression {
            tokens: storage.tokens,
            line_index: storage.line_index,
            expression: &await_.value,
            range: None,
        }
    }

    fn field_base<'ast>(
        &self,
        _storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        field: <Self::Types as AbstractTypes>::Field<'ast>,
    ) -> <Self::Types as AbstractTypes>::Expression<'ast> {
        let range = field_base_range(field.field, field.tokens);
        Expression {
            tokens: field.tokens,
            line_index: field.line_index,
            expression: &field.field.value,
            range,
        }
    }

    fn call_callee<'ast>(
        &self,
        _storage: &RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        call: <Self::Types as AbstractTypes>::Call<'ast>,
    ) -> <Self::Types as AbstractTypes>::Expression<'ast> {
        Expression {
            tokens: call.tokens,
            line_index: call.line_index,
            expression: &call.call.func,
            range: None,
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

fn field_base_range(field: &ExprAttribute, tokens: &Tokens) -> Option<TextRange> {
    parenthesized_range(
        ExprRef::from(field.value.as_ref()),
        AnyNodeRef::from(field),
        tokens,
    )
}

impl RunLow for Python {
    fn command_to_run_source_file(&self, context: &LightContext, source_file: &Path) -> Command {
        self.pytest(
            context,
            util::strip_prefix(source_file, context.root).unwrap(),
        )
    }

    fn instrument_source_file(
        &self,
        _context: &LightContext,
        _rewriter: &mut super::Rewriter,
        _source_file: &SourceFile,
        _n_instrumentable_statements: usize,
    ) -> Result<()> {
        Ok(())
    }

    fn statement_prefix_and_suffix(&self, span: &Span) -> Result<(String, String)> {
        Ok((
            format!(
                r#"if __import__("os").environ.get("NECESSIST_REMOVAL") != "{}": "#,
                span.id()
            ),
            String::new(),
        ))
    }

    fn command_to_build_source_file(&self, context: &LightContext, source_file: &Path) -> Command {
        self.py_compile(context, source_file)
    }

    fn command_to_build_test(
        &self,
        context: &LightContext,
        _test_name: &str,
        span: &Span,
    ) -> Command {
        self.py_compile(context, &span.source_file)
    }

    fn command_to_run_test(
        &self,
        context: &LightContext,
        test_name: &str,
        span: &Span,
    ) -> (Command, Vec<String>, Option<ProcessLines>) {
        let path = util::strip_prefix(&span.source_file, context.root).unwrap();
        let node_id = format!("{}::{test_name}", path.to_string_lossy());
        (self.pytest(context, Path::new(&node_id)), Vec::new(), None)
    }
}

impl Python {
    fn pytest(&self, context: &LightContext, target: &Path) -> Command {
        let mut command = Command::new(self.executable);
        command.current_dir(context.root.as_path());
        command.args(["-m", "pytest"]);
        command.arg(target);
        command
    }

    fn py_compile(&self, context: &LightContext, source_file: &Path) -> Command {
        let mut command = Command::new(self.executable);
        command.current_dir(context.root.as_path());
        command.args(["-m", "py_compile"]);
        command.arg(source_file);
        command
    }
}

fn python_executable(mut is_available: impl FnMut(&str) -> bool) -> Option<&'static str> {
    ["python", "python3"]
        .into_iter()
        .find(|&name| is_available(name))
}

fn command_is_available(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .stdin(Stdio::null())
        .output_wc()
        .is_ok_and(|output| {
            output.status.success()
                && (is_python_3_version(&output.stdout) || is_python_3_version(&output.stderr))
        })
}

fn is_python_3_version(output: &[u8]) -> bool {
    output.starts_with(b"Python 3.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_python_ast::{Expr, Stmt};
    use ruff_python_parser::parse_module;

    fn parenthesized_receiver(source: &str) -> &str {
        let parsed = parse_module(source).unwrap();
        let Stmt::Expr(statement) = &parsed.syntax().body[0] else {
            panic!("expected expression statement");
        };
        let Expr::Call(call) = statement.value.as_ref() else {
            panic!("expected call");
        };
        let Expr::Attribute(field) = call.func.as_ref() else {
            panic!("expected attribute");
        };
        &source[field_base_range(field, parsed.tokens()).unwrap()]
    }

    #[test]
    fn parenthesized_method_receiver_range() {
        assert_eq!("(value)", parenthesized_receiver("(value).strip()"));
    }

    #[test]
    fn nested_parenthesized_method_receiver_range() {
        assert_eq!("((value))", parenthesized_receiver("((value)).strip()"));
    }

    #[test]
    fn multiline_parenthesized_method_receiver_range() {
        assert_eq!(
            "(\n    value\n)",
            parenthesized_receiver("(\n    value\n).strip()")
        );
    }

    #[test]
    fn parenthesized_call_receiver_range() {
        assert_eq!(
            "(factory())",
            parenthesized_receiver("(factory()).method()")
        );
    }

    #[test]
    fn python_executable_prefers_python() {
        assert_eq!(Some("python"), python_executable(|_| true));
    }

    #[test]
    fn python_executable_falls_back_to_python3() {
        assert_eq!(Some("python3"), python_executable(|name| name == "python3"));
    }

    #[test]
    fn python_executable_can_be_absent() {
        assert_eq!(None, python_executable(|_| false));
    }

    #[test]
    fn python_3_version_is_accepted() {
        assert!(is_python_3_version(b"Python 3.12.11\n"));
    }

    #[test]
    fn python_2_version_is_rejected() {
        assert!(!is_python_3_version(b"Python 2.7.18\n"));
    }

    #[test]
    fn unrelated_output_is_rejected() {
        assert!(!is_python_3_version(b"command not found\n"));
    }
}
