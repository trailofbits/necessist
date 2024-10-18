use super::{
    AbstractTypes, GenericVisitor, MaybeNamed, Named, ParseLow, ProcessLines, RunLow, Spanned,
    WalkDirResult,
};
use anyhow::{anyhow, bail, Context, Result};
use necessist_core::{
    framework::{SpanTestMaps, TestSet},
    util, LightContext, LineColumn, SourceFile, Span, __Rewriter as Rewriter,
};
use once_cell::sync::Lazy;
use regex::Regex;
use std::{
    collections::BTreeMap, convert::Infallible, fs::read_to_string, path::Path, process::Command,
};
use streaming_iterator::StreamingIterator;
use tree_sitter::{
    Language, Node, Parser, Point, Query, QueryCapture, QueryCursor, Range, TextProvider, Tree,
};

mod bounded_cursor;

mod storage;
use storage::Storage;

mod visitor;
use visitor::{collect_local_functions, visit};

// smoelius: To future editors of this file: Tree-sitter Playground has been super helpful for
// debugging: https://tree-sitter.github.io/tree-sitter/playground

const BLOCK_STATEMENTS_SOURCE: &str = r#"
(block
    "{"
    (_statement) @statement
    "}"
) @block
"#;

const EXPRESSION_STATEMENT_EXPRESSION_SOURCE: &str = r"
(expression_statement
    (_expression) @expression
) @expression_statement
";

static LANGUAGE: Lazy<Language> = Lazy::new(|| Language::from(tree_sitter_go::LANGUAGE));
static BLOCK_STATEMENTS_QUERY: Lazy<Query> = Lazy::new(|| valid_query(BLOCK_STATEMENTS_SOURCE));
static EXPRESSION_STATEMENT_EXPRESSION_QUERY: Lazy<Query> =
    Lazy::new(|| valid_query(EXPRESSION_STATEMENT_EXPRESSION_SOURCE));

fn valid_query(source: &str) -> Query {
    #[allow(clippy::unwrap_used)]
    Query::new(&LANGUAGE, source).unwrap()
}

static FIELD_FIELD: Lazy<u16> = Lazy::new(|| valid_field_id("field"));
static FUNCTION_FIELD: Lazy<u16> = Lazy::new(|| valid_field_id("function"));
static OPERAND_FIELD: Lazy<u16> = Lazy::new(|| valid_field_id("operand"));

fn valid_field_id(field_name: &str) -> u16 {
    LANGUAGE.field_id_for_name(field_name).unwrap().into()
}

static BLOCK_KIND: Lazy<u16> = Lazy::new(|| non_zero_kind_id("block"));
static BREAK_STATEMENT_KIND: Lazy<u16> = Lazy::new(|| non_zero_kind_id("break_statement"));
static CALL_EXPRESSION_KIND: Lazy<u16> = Lazy::new(|| non_zero_kind_id("call_expression"));
static CONST_DECLARATION_KIND: Lazy<u16> = Lazy::new(|| non_zero_kind_id("const_declaration"));
static CONTINUE_STATEMENT_KIND: Lazy<u16> = Lazy::new(|| non_zero_kind_id("continue_statement"));
static DEFER_STATEMENT_KIND: Lazy<u16> = Lazy::new(|| non_zero_kind_id("defer_statement"));
static IDENTIFIER_KIND: Lazy<u16> = Lazy::new(|| non_zero_kind_id("identifier"));
static RETURN_STATEMENT_KIND: Lazy<u16> = Lazy::new(|| non_zero_kind_id("return_statement"));
static SELECTOR_EXPRESSION_KIND: Lazy<u16> = Lazy::new(|| non_zero_kind_id("selector_expression"));
static SHORT_VAR_DECLARATION_KIND: Lazy<u16> =
    Lazy::new(|| non_zero_kind_id("short_var_declaration"));
static TYPE_DECLARATION_KIND: Lazy<u16> = Lazy::new(|| non_zero_kind_id("type_declaration"));
static VAR_DECLARATION_KIND: Lazy<u16> = Lazy::new(|| non_zero_kind_id("var_declaration"));

fn non_zero_kind_id(kind: &str) -> u16 {
    let kind_id = LANGUAGE.id_for_node_kind(kind, true);
    assert_ne!(0, kind_id);
    kind_id
}

#[derive(Debug)]
pub struct Go {
    os_name_map: std::cell::RefCell<BTreeMap<SourceFile, String>>,
}

impl Go {
    pub fn applicable(context: &LightContext) -> Result<bool> {
        context.root.join("go.mod").try_exists().map_err(Into::into)
    }

    pub fn new() -> Self {
        Self {
            os_name_map: std::cell::RefCell::new(BTreeMap::new()),
        }
    }
}

#[derive(Clone, Copy)]
pub struct Test<'ast> {
    name: &'ast str,
    body: Node<'ast>,
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct LocalFunction<'ast> {
    body: Node<'ast>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct NodeWithText<'ast> {
    text: &'ast str,
    node: Node<'ast>,
}

impl Spanned for NodeWithText<'_> {
    fn span(&self, source_file: &SourceFile) -> Span {
        self.node.range().to_internal_span(source_file)
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct Statement<'ast>(NodeWithText<'ast>);

#[derive(Clone, Copy)]
pub struct Expression<'ast>(NodeWithText<'ast>);

#[derive(Clone, Copy)]
pub struct Field<'ast>(NodeWithText<'ast>);

#[derive(Clone, Copy)]
pub struct Call<'ast>(NodeWithText<'ast>);

pub struct Types;

impl AbstractTypes for Types {
    type Storage<'ast> = Storage<'ast>;
    type File = (String, Tree);
    type Test<'ast> = Test<'ast>;
    type LocalFunction<'ast> = LocalFunction<'ast>;
    type Statement<'ast> = Statement<'ast>;
    type Expression<'ast> = Expression<'ast>;
    type Await<'ast> = Infallible;
    type Field<'ast> = Field<'ast>;
    type Call<'ast> = Call<'ast>;
    type MacroCall<'ast> = Infallible;
}

impl Named for Test<'_> {
    fn name(&self) -> String {
        self.name.to_string()
    }
}

impl Spanned for Statement<'_> {
    fn span(&self, source_file: &SourceFile) -> Span {
        self.0.span(source_file)
    }
}

impl Spanned for Expression<'_> {
    fn span(&self, source_file: &SourceFile) -> Span {
        self.0.span(source_file)
    }
}

impl Spanned for Field<'_> {
    fn span(&self, source_file: &SourceFile) -> Span {
        self.0.span(source_file)
    }
}

impl Spanned for Call<'_> {
    fn span(&self, source_file: &SourceFile) -> Span {
        self.0.span(source_file)
    }
}

impl MaybeNamed for <Types as AbstractTypes>::Expression<'_> {
    fn name(&self) -> Option<String> {
        if self.0.node.kind_id() == *IDENTIFIER_KIND {
            self.0
                .node
                .utf8_text(self.0.text.as_bytes())
                .ok()
                .map(ToOwned::to_owned)
        } else {
            None
        }
    }
}

impl MaybeNamed for <Types as AbstractTypes>::Field<'_> {
    fn name(&self) -> Option<String> {
        assert_eq!(*SELECTOR_EXPRESSION_KIND, self.0.node.kind_id());
        self.0
            .node
            .child_by_field_id(*FIELD_FIELD)
            .unwrap()
            .utf8_text(self.0.text.as_bytes())
            .ok()
            .map(ToOwned::to_owned)
    }
}

impl MaybeNamed for <Types as AbstractTypes>::Call<'_> {
    fn name(&self) -> Option<String> {
        assert_eq!(*CALL_EXPRESSION_KIND, self.0.node.kind_id());
        self.0
            .node
            .child_by_field_id(*FUNCTION_FIELD)
            .unwrap()
            .utf8_text(self.0.text.as_bytes())
            .ok()
            .map(ToOwned::to_owned)
    }
}

impl ParseLow for Go {
    type Types = Types;

    const IGNORED_FUNCTIONS: Option<&'static [&'static str]> =
        Some(&["assert.*", "panic", "require.*"]);

    const IGNORED_MACROS: Option<&'static [&'static str]> = None;

    const IGNORED_METHODS: Option<&'static [&'static str]> = Some(&[
        "Close", "Error", "Errorf", "Fail", "FailNow", "Fatal", "Fatalf", "Helper", "Log", "Logf",
        "Parallel", "Skip", "Skipf", "SkipNow",
    ]);

    fn walk_dir(&self, root: &Path) -> Box<dyn Iterator<Item = WalkDirResult>> {
        Box::new(
            walkdir::WalkDir::new(root)
                .into_iter()
                .filter_entry(|entry| {
                    let path = entry.path();
                    !path.is_file() || path.to_string_lossy().ends_with("_test.go")
                }),
        )
    }

    fn parse_source_file(
        &self,
        source_file: &Path,
    ) -> Result<<Self::Types as AbstractTypes>::File> {
        let text = read_to_string(source_file)?;
        let mut parser = Parser::new();
        parser
            .set_language(&LANGUAGE)
            .with_context(|| "Failed to load Go grammar")?;
        // smoelius: https://github.com/tree-sitter/tree-sitter/issues/255
        parser
            .parse(&text, None)
            .map(|tree| (text, tree))
            .ok_or_else(|| anyhow!("Unspecified error"))
    }

    fn storage_from_file<'ast>(
        &self,
        file: &'ast <Self::Types as AbstractTypes>::File,
    ) -> <Self::Types as AbstractTypes>::Storage<'ast> {
        Storage::new(file)
    }

    fn local_functions<'ast>(
        &self,
        _storage: &std::cell::RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        file: &'ast <Self::Types as AbstractTypes>::File,
    ) -> Result<BTreeMap<String, Vec<<Self::Types as AbstractTypes>::LocalFunction<'ast>>>> {
        collect_local_functions(&file.0, &file.1)
    }

    fn visit_file<'ast>(
        generic_visitor: GenericVisitor<'_, '_, '_, 'ast, Self>,
        storage: &std::cell::RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        file: &'ast <Self::Types as AbstractTypes>::File,
    ) -> Result<(TestSet, SpanTestMaps)> {
        visit(generic_visitor, storage, &file.1)
    }

    fn test_statements<'ast>(
        &self,
        storage: &std::cell::RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        test: <Self::Types as AbstractTypes>::Test<'ast>,
    ) -> Vec<<Self::Types as AbstractTypes>::Statement<'ast>> {
        assert_eq!(*BLOCK_KIND, test.body.kind_id());
        process_self_captures(
            &BLOCK_STATEMENTS_QUERY,
            test.body,
            storage.borrow().text.as_bytes(),
            |captures| {
                let mut statements = Vec::new();
                while let Some(captures) = captures.next() {
                    assert_eq!(2, captures.len());
                    statements.push(Statement(NodeWithText {
                        text: storage.borrow().text,
                        node: captures[0].node,
                    }));
                }
                statements
            },
        )
    }

    fn statement_is_removable(
        &self,
        _statement: <Self::Types as AbstractTypes>::Statement<'_>,
    ) -> bool {
        true
    }

    fn statement_is_expression<'ast>(
        &self,
        _storage: &std::cell::RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        statement: <Self::Types as AbstractTypes>::Statement<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Expression<'ast>> {
        if let Some(captures) = process_self_captures(
            &EXPRESSION_STATEMENT_EXPRESSION_QUERY,
            statement.0.node,
            statement.0.text.as_bytes(),
            |captures| captures.next().cloned(),
        ) {
            assert_eq!(2, captures.len());
            Some(Expression(NodeWithText {
                text: statement.0.text,
                node: captures[0].node,
            }))
        } else {
            None
        }
    }

    fn statement_is_control<'ast>(
        &self,
        _storage: &std::cell::RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        statement: <Self::Types as AbstractTypes>::Statement<'ast>,
    ) -> bool {
        [
            *BREAK_STATEMENT_KIND,
            *CONTINUE_STATEMENT_KIND,
            *DEFER_STATEMENT_KIND,
            *RETURN_STATEMENT_KIND,
        ]
        .contains(&statement.0.node.kind_id())
    }

    fn statement_is_declaration<'ast>(
        &self,
        _storage: &std::cell::RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        statement: <Self::Types as AbstractTypes>::Statement<'ast>,
    ) -> bool {
        statement.0.node.kind_id() == *CONST_DECLARATION_KIND
            || statement.0.node.kind_id() == *SHORT_VAR_DECLARATION_KIND
            || statement.0.node.kind_id() == *TYPE_DECLARATION_KIND
            || statement.0.node.kind_id() == *VAR_DECLARATION_KIND
    }

    fn expression_is_await<'ast>(
        &self,
        _storage: &std::cell::RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        _expression: <Self::Types as AbstractTypes>::Expression<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Await<'ast>> {
        None
    }

    fn expression_is_field<'ast>(
        &self,
        _storage: &std::cell::RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        expression: <Self::Types as AbstractTypes>::Expression<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Field<'ast>> {
        if expression.0.node.kind_id() == *SELECTOR_EXPRESSION_KIND {
            Some(Field(expression.0))
        } else {
            None
        }
    }

    fn expression_is_call<'ast>(
        &self,
        _storage: &std::cell::RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        expression: <Self::Types as AbstractTypes>::Expression<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Call<'ast>> {
        if expression.0.node.kind_id() == *CALL_EXPRESSION_KIND {
            Some(Call(expression.0))
        } else {
            None
        }
    }

    fn expression_is_macro_call<'ast>(
        &self,
        _storage: &std::cell::RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        _expression: <Self::Types as AbstractTypes>::Expression<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::MacroCall<'ast>> {
        None
    }

    fn await_arg<'ast>(
        &self,
        _storage: &std::cell::RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        _await: <Self::Types as AbstractTypes>::Await<'ast>,
    ) -> <Self::Types as AbstractTypes>::Expression<'ast> {
        unreachable!()
    }

    fn field_base<'ast>(
        &self,
        _storage: &std::cell::RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        field: <Self::Types as AbstractTypes>::Field<'ast>,
    ) -> <Self::Types as AbstractTypes>::Expression<'ast> {
        assert_eq!(*SELECTOR_EXPRESSION_KIND, field.0.node.kind_id());
        Expression(NodeWithText {
            text: field.0.text,
            node: field.0.node.child_by_field_id(*OPERAND_FIELD).unwrap(),
        })
    }

    fn call_callee<'ast>(
        &self,
        _storage: &std::cell::RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        call: <Self::Types as AbstractTypes>::Call<'ast>,
    ) -> <Self::Types as AbstractTypes>::Expression<'ast> {
        assert_eq!(*CALL_EXPRESSION_KIND, call.0.node.kind_id());
        Expression(NodeWithText {
            text: call.0.text,
            node: call.0.node.child_by_field_id(*FUNCTION_FIELD).unwrap(),
        })
    }

    fn macro_call_callee<'ast>(
        &self,
        _storage: &std::cell::RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        _macro_call: <Self::Types as AbstractTypes>::MacroCall<'ast>,
    ) -> <Self::Types as AbstractTypes>::Expression<'ast> {
        unreachable!()
    }
}

impl RunLow for Go {
    fn command_to_run_source_file(&self, context: &LightContext, source_file: &Path) -> Command {
        Self::test_command(context, source_file)
    }

    fn instrument_source_file(
        &self,
        _context: &LightContext,
        rewriter: &mut Rewriter,
        source_file: &SourceFile,
        n_instrumentable_statements: usize,
    ) -> Result<()> {
        let mut os_name_map = self.os_name_map.borrow_mut();

        // smoelius: `n_instrumentable_statements == 0` to avoid an "unused imports" error.
        if os_name_map.contains_key(source_file) || n_instrumentable_statements == 0 {
            return Ok(());
        }

        if let Some(os_name) = imports_os(source_file.contents()) {
            os_name_map.insert(source_file.clone(), os_name.to_owned());
            return Ok(());
        }

        let Some(package_line) = package_line(source_file.contents()) else {
            bail!("Failed to find line starting with `package`");
        };
        let line_column = LineColumn {
            line: package_line + 1,
            column: 0,
        };
        source_file.insert(rewriter, line_column, "import \"os\"\n");
        os_name_map.insert(source_file.clone(), "os".to_owned());
        Ok(())
    }

    fn statement_prefix_and_suffix(&self, span: &Span) -> Result<(String, String)> {
        let os_name_map = self.os_name_map.borrow();
        let os_name = os_name_map.get(&span.source_file).unwrap();
        let os_prefix = if os_name == "." {
            String::new()
        } else {
            format!("{os_name}.")
        };
        Ok((
            format!(
                r#"if {os_prefix}Getenv("NECESSIST_REMOVAL") != "{}" {{ "#,
                span.id()
            ),
            " }".to_owned(),
        ))
    }

    fn command_to_build_source_file(&self, context: &LightContext, source_file: &Path) -> Command {
        let mut command = Self::test_command(context, source_file);
        command.arg("-run=^$");
        command
    }

    fn command_to_build_test(
        &self,
        context: &LightContext,
        _test_name: &str,
        span: &Span,
    ) -> Command {
        let mut command = Self::test_command(context, &span.source_file);
        command.arg("-run=^$");
        command
    }

    fn command_to_run_test(
        &self,
        context: &LightContext,
        test_name: &str,
        span: &Span,
    ) -> (Command, Vec<String>, Option<ProcessLines>) {
        let mut command = Self::test_command(context, &span.source_file);
        command.args([format!("-run=^{test_name}$").as_ref(), "-v"]);

        let needle = format!("=== RUN   {test_name}");

        (
            command,
            Vec::new(),
            Some((false, Box::new(move |line| line == needle))),
        )
    }
}

const PREFIX: &str = r"\([^)]*";
const NAME: &str = r"(\.|[A-Za-z_][0-9A-Za-z_]*)( )?";
const SUFFIX: &str = r"[^)]*\)";

static IMPORT_NAMED_OS_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(&import_os_re("", NAME, "")).unwrap());
static IMPORT_UNNAMED_OS_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(&import_os_re("", "", "")).unwrap());
static PARENTHESIZED_IMPORT_NAMED_OS_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(&import_os_re(PREFIX, NAME, SUFFIX)).unwrap());
static PARENTHESIZED_IMPORT_UNNAMED_OS_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(&import_os_re(PREFIX, "", SUFFIX)).unwrap());

fn import_os_re(prefix: &str, name: &str, suffix: &str) -> String {
    format!(r#"\bimport {prefix}{name}"os"{suffix}"#)
}

fn imports_os(contents: &str) -> Option<&str> {
    if let Some(captures) = IMPORT_NAMED_OS_RE.captures(contents) {
        assert_eq!(3, captures.len());
        Some(captures.get(1).unwrap().as_str())
    } else if let Some(captures) = IMPORT_UNNAMED_OS_RE.captures(contents) {
        assert_eq!(1, captures.len());
        Some("os")
    } else if let Some(captures) = PARENTHESIZED_IMPORT_NAMED_OS_RE.captures(contents) {
        assert_eq!(3, captures.len());
        Some(captures.get(1).unwrap().as_str())
    } else if let Some(captures) = PARENTHESIZED_IMPORT_UNNAMED_OS_RE.captures(contents) {
        assert_eq!(1, captures.len());
        Some("os")
    } else {
        None
    }
}

fn package_line(contents: &str) -> Option<usize> {
    // smoelius: `+ 1` because `LineColumn` `line`s are one-based.
    contents
        .lines()
        .position(|line| line.starts_with("package "))
        .map(|i| i + 1)
}

impl Go {
    fn test_command(context: &LightContext, source_file: &Path) -> Command {
        #[allow(clippy::expect_used)]
        let package_path = source_file_package_path(context, source_file)
            .expect("Failed to get source file package path");
        let mut command = Command::new("go");
        command.current_dir(context.root.as_path());
        command.arg("test");
        command.arg(package_path);
        command
    }
}

fn source_file_package_path(context: &LightContext, source_file: &Path) -> Result<String> {
    let dir = source_file
        .parent()
        .ok_or_else(|| anyhow!("Failed to get parent"))?;

    let stripped = util::strip_prefix(dir, context.root)?;

    Ok(Path::new(".").join(stripped).to_string_lossy().to_string())
}

fn process_self_captures<'query, 'source, 'tree, T, U>(
    query: &'query Query,
    node: Node<'tree>,
    text_provider: T,
    f: impl Fn(&mut dyn StreamingIterator<Item = Vec<QueryCapture<'tree>>>) -> U,
) -> U
where
    'source: 'tree,
    T: TextProvider<&'source [u8]> + 'source,
{
    // smoelius: `STATEMENT_QUERY` does not match `node` when `node` is a statement and the query
    // starts at `node`. I don't understand why.
    // smoelius: `STATEMENT_QUERY` is defined in the sibling file, visitor.rs.
    let (max_start_depth, query_node) = if let Some(parent) = node.parent() {
        (1, parent)
    } else {
        (0, node)
    };

    let mut cursor = QueryCursor::new();

    cursor.set_max_start_depth(Some(max_start_depth));

    let query_matches = cursor.matches(query, query_node, text_provider);

    let mut iter = query_matches
        .map(|query_match| query_match.captures)
        .filter(|captures| captures.iter().any(|capture| capture.node == node))
        .map(|captures| sort_captures(captures));

    f(&mut iter)
}

fn sort_captures<'tree>(captures: &[QueryCapture<'tree>]) -> Vec<QueryCapture<'tree>> {
    let mut captures = captures.to_vec();
    captures.sort_by_key(|capture| capture.index);
    captures
}

trait ToInternalSpan {
    fn to_internal_span(&self, source_file: &SourceFile) -> Span;
}

impl ToInternalSpan for Range {
    fn to_internal_span(&self, source_file: &SourceFile) -> Span {
        Span {
            source_file: source_file.clone(),
            start: self.start_point.to_line_column(source_file),
            end: self.end_point.to_line_column(source_file),
        }
    }
}

trait ToLineColumn {
    fn to_line_column(&self, source_file: &SourceFile) -> LineColumn;
}

// smoelius: `Point`'s `column` field counts bytes, not chars. See:
// https://github.com/tree-sitter/tree-sitter/issues/397#issuecomment-515115012
impl ToLineColumn for Point {
    fn to_line_column(&self, source_file: &SourceFile) -> LineColumn {
        let line_column = LineColumn {
            line: self.row + 1,
            column: 0,
        };
        let (line_offset, _) = source_file
            .offset_calculator()
            .borrow_mut()
            .offsets_from_span(&Span {
                source_file: source_file.clone(),
                start: line_column,
                end: line_column,
            });
        let suffix = &source_file.contents()[line_offset..];
        let column = suffix
            .char_indices()
            .position(|(offset, _)| self.column == offset)
            .unwrap();
        LineColumn {
            line: self.row + 1,
            column,
        }
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn imports_os() {
        const TESTS: &[(&str, Option<&str>)] = &[
            ("", None),
            (r#"import "fmt""#, None),
            (r#"import "os""#, Some("os")),
            (r#"import . "os""#, Some(".")),
            (r#"import x "os""#, Some("x")),
            (r#"import ( "os" )"#, Some("os")),
            (r#"import ( . "os" )"#, Some(".")),
            (r#"import ( x "os" )"#, Some("x")),
        ];
        for &(contents, expected) in TESTS {
            assert_eq!(expected, super::imports_os(contents), "{contents:?}");
        }
    }
}
