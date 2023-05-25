use super::{
    AbstractTypes, GenericVisitor, MaybeNamed, Named, ParseLow, ProcessLines, RunLow, Spanned,
    WalkDirResult,
};
use anyhow::{anyhow, Context, Result};
use necessist_core::{util, LightContext, LineColumn, SourceFile, Span};
use once_cell::sync::Lazy;
use std::{
    collections::BTreeMap, convert::Infallible, fs::read_to_string, path::Path, process::Command,
};
use tree_sitter::{
    Node, Parser, Point, Query, QueryCapture, QueryCursor, Range, TextProvider, Tree,
};

mod bounded_cursor;

mod storage;
use storage::Storage;

#[cfg_attr(
    dylint_lib = "non_local_effect_before_error_return",
    allow(non_local_effect_before_error_return)
)]
mod visitor;
use visitor::visit;

// smoelius: To future editors of this file: Tree-sitter Playground has been super helpful for
// debugging: https://tree-sitter.github.io/tree-sitter/playground

const BLOCK_STATEMENTS_SOURCE: &str = r#"
(block
    "{"
    (_statement) @statement
    "}"
) @block
"#;

const EXPRESSION_SOURCE: &str = r#"
(_expression) @expression
"#;

static BLOCK_STATEMENTS_QUERY: Lazy<Query> = Lazy::new(|| valid_query(BLOCK_STATEMENTS_SOURCE));
static EXPRESSION_QUERY: Lazy<Query> = Lazy::new(|| valid_query(EXPRESSION_SOURCE));

fn valid_query(source: &str) -> Query {
    #[allow(clippy::unwrap_used)]
    Query::new(tree_sitter_go::language(), source).unwrap()
}

static FIELD_FIELD: Lazy<u16> = Lazy::new(|| valid_field_id("field"));
static FUNCTION_FIELD: Lazy<u16> = Lazy::new(|| valid_field_id("function"));
static OPERAND_FIELD: Lazy<u16> = Lazy::new(|| valid_field_id("operand"));

fn valid_field_id(field_name: &str) -> u16 {
    tree_sitter_go::language()
        .field_id_for_name(field_name)
        .unwrap()
}

static BLOCK_KIND: Lazy<u16> = Lazy::new(|| non_zero_kind_id("block"));
static BREAK_STATEMENT_KIND: Lazy<u16> = Lazy::new(|| non_zero_kind_id("break_statement"));
static CALL_EXPRESSION_KIND: Lazy<u16> = Lazy::new(|| non_zero_kind_id("call_expression"));
static CONTINUE_STATEMENT_KIND: Lazy<u16> = Lazy::new(|| non_zero_kind_id("continue_statement"));
static DEFER_STATEMENT_KIND: Lazy<u16> = Lazy::new(|| non_zero_kind_id("defer_statement"));
static IDENTIFIER_KIND: Lazy<u16> = Lazy::new(|| non_zero_kind_id("identifier"));
static RETURN_STATEMENT_KIND: Lazy<u16> = Lazy::new(|| non_zero_kind_id("return_statement"));
static SELECTOR_EXPRESSION_KIND: Lazy<u16> = Lazy::new(|| non_zero_kind_id("selector_expression"));
static SHORT_VAR_DECLARATION_KIND: Lazy<u16> =
    Lazy::new(|| non_zero_kind_id("short_var_declaration"));
static VAR_DECLARATION_KIND: Lazy<u16> = Lazy::new(|| non_zero_kind_id("var_declaration"));

fn non_zero_kind_id(kind: &str) -> u16 {
    let kind_id = tree_sitter_go::language().id_for_node_kind(kind, true);
    assert_ne!(0, kind_id);
    kind_id
}

#[derive(Debug)]
pub struct Golang {
    span_test_name_map: BTreeMap<Span, String>,
}

impl Golang {
    pub fn applicable(context: &LightContext) -> Result<bool> {
        context.root.join("go.mod").try_exists().map_err(Into::into)
    }

    pub fn new() -> Self {
        Self {
            span_test_name_map: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Copy)]
pub struct Test<'ast> {
    name: &'ast str,
    body: Node<'ast>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct NodeWithText<'ast> {
    text: &'ast str,
    node: Node<'ast>,
}

impl<'ast> Spanned for NodeWithText<'ast> {
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
    type Statement<'ast> = Statement<'ast>;
    type Expression<'ast> = Expression<'ast>;
    type Await<'ast> = Infallible;
    type Field<'ast> = Field<'ast>;
    type Call<'ast> = Call<'ast>;
    type MacroCall<'ast> = Infallible;
}

impl<'ast> Named for Test<'ast> {
    fn name(&self) -> String {
        self.name.to_string()
    }
}

impl<'ast> Spanned for Statement<'ast> {
    fn span(&self, source_file: &SourceFile) -> Span {
        self.0.span(source_file)
    }
}

impl<'ast> Spanned for Expression<'ast> {
    fn span(&self, source_file: &SourceFile) -> Span {
        self.0.span(source_file)
    }
}

impl<'ast> Spanned for Field<'ast> {
    fn span(&self, source_file: &SourceFile) -> Span {
        self.0.span(source_file)
    }
}

impl<'ast> Spanned for Call<'ast> {
    fn span(&self, source_file: &SourceFile) -> Span {
        self.0.span(source_file)
    }
}

impl<'ast> MaybeNamed for <Types as AbstractTypes>::Expression<'ast> {
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

impl<'ast> MaybeNamed for <Types as AbstractTypes>::Field<'ast> {
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

impl<'ast> MaybeNamed for <Types as AbstractTypes>::Call<'ast> {
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

impl ParseLow for Golang {
    type Types = Types;

    const IGNORED_FUNCTIONS: Option<&'static [&'static str]> = Some(&["assert.*", "require.*"]);

    const IGNORED_MACROS: Option<&'static [&'static str]> = None;

    const IGNORED_METHODS: Option<&'static [&'static str]> = Some(&[
        "Close", "Error", "Errorf", "Fail", "FailNow", "Fatal", "Fatalf", "Log", "Logf", "Parallel",
    ]);

    fn walk_dir(root: &Path) -> Box<dyn Iterator<Item = WalkDirResult>> {
        Box::new(
            walkdir::WalkDir::new(root)
                .into_iter()
                .filter_entry(|entry| {
                    let path = entry.path();
                    !path.is_file() || path.to_string_lossy().ends_with("_test.go")
                }),
        )
    }

    fn parse_file(&self, test_file: &Path) -> Result<<Self::Types as AbstractTypes>::File> {
        let text = read_to_string(test_file)?;
        let mut parser = Parser::new();
        parser
            .set_language(tree_sitter_go::language())
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

    fn visit_file<'ast>(
        generic_visitor: crate::generic_visitor::GenericVisitor<'_, '_, '_, 'ast, Self>,
        storage: &std::cell::RefCell<<Self::Types as crate::parsing::AbstractTypes>::Storage<'ast>>,
        file: &'ast <Self::Types as crate::parsing::AbstractTypes>::File,
    ) -> Result<Vec<Span>> {
        visit(generic_visitor, storage, &file.1)
    }

    fn on_candidate_found(
        &mut self,
        _context: &LightContext,
        _storage: &std::cell::RefCell<<Self::Types as crate::parsing::AbstractTypes>::Storage<'_>>,
        test_name: &str,
        span: &Span,
    ) {
        self.span_test_name_map
            .insert(span.clone(), test_name.to_owned());
    }

    fn test_statements<'ast>(
        &self,
        storage: &std::cell::RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        test: <Self::Types as AbstractTypes>::Test<'ast>,
    ) -> Vec<<Self::Types as AbstractTypes>::Statement<'ast>> {
        assert_eq!(*BLOCK_KIND, test.body.kind_id());
        collect_matches(
            &BLOCK_STATEMENTS_QUERY,
            test.body,
            storage.borrow().text.as_bytes(),
        )
        .into_iter()
        .map(|captures| {
            assert_eq!(2, captures.len());
            Statement(NodeWithText {
                text: storage.borrow().text,
                node: captures[0].node,
            })
        })
        .collect()
    }

    fn statement_is_expression<'ast>(
        &self,
        _storage: &std::cell::RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        statement: <Self::Types as AbstractTypes>::Statement<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Expression<'ast>> {
        if let Some(captures) = is_match(
            &EXPRESSION_QUERY,
            statement.0.node,
            statement.0.text.as_bytes(),
        ) {
            assert_eq!(1, captures.len());
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
        statement.0.node.kind_id() == *SHORT_VAR_DECLARATION_KIND
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

impl RunLow for Golang {
    fn command_to_run_test_file(&self, context: &LightContext, test_file: &Path) -> Command {
        Self::test_command(context, test_file)
    }

    fn command_to_build_test(&self, context: &LightContext, span: &Span) -> Command {
        let mut command = Self::test_command(context, &span.source_file);
        command.arg("-run=^$");
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
            .expect("Test name is not set");

        let mut command = Self::test_command(context, &span.source_file);
        command.args([format!("-run=^{test_name}$").as_ref(), "-v"]);

        let needle = format!("=== RUN   {test_name}");

        (
            command,
            Vec::new(),
            Some((
                (false, Box::new(move |line| line == needle)),
                test_name.clone(),
            )),
        )
    }
}

impl Golang {
    fn test_command(context: &LightContext, test_file: &Path) -> Command {
        #[allow(clippy::expect_used)]
        let package_path = test_file_package_path(context, test_file)
            .expect("Failed to get test file package path");
        let mut command = Command::new("go");
        command.current_dir(context.root.as_path());
        command.arg("test");
        command.arg(package_path);
        command
    }
}

fn test_file_package_path(context: &LightContext, test_file: &Path) -> Result<String> {
    let dir = test_file
        .parent()
        .ok_or_else(|| anyhow!("Failed to get parent"))?;

    let stripped = util::strip_prefix(dir, context.root)?;

    Ok(Path::new(".").join(stripped).to_string_lossy().to_string())
}

fn is_match<'query, 'source, 'tree, T>(
    query: &'query Query,
    node: Node<'tree>,
    text_provider: T,
) -> Option<Vec<QueryCapture<'tree>>>
where
    T: TextProvider<'source> + 'source,
{
    // smoelius: `STATEMENT_QUERY` does not match `node` when `node` is a statement and the query
    // starts at `node`. I don't understand why.
    let (max_start_depth, query_node) = if let Some(parent) = node.parent() {
        (1, parent)
    } else {
        (0, node)
    };

    let mut cursor = QueryCursor::new();

    cursor.set_max_start_depth(max_start_depth);

    let query_matches = cursor.matches(query, query_node, text_provider);

    query_matches
        .map(|query_match| query_match.captures)
        .find(|captures| captures.iter().any(|capture| capture.node == node))
        .map(sort_captures)
}

fn collect_matches<'query, 'source, 'tree, T>(
    query: &'query Query,
    node: Node<'tree>,
    text_provider: T,
) -> Vec<Vec<QueryCapture<'tree>>>
where
    T: TextProvider<'source> + 'source,
{
    // smoelius: See comment in `is_match` just above.
    let (max_start_depth, query_node) = if let Some(parent) = node.parent() {
        (1, parent)
    } else {
        (0, node)
    };

    let mut cursor = QueryCursor::new();

    cursor.set_max_start_depth(max_start_depth);

    let query_matches = cursor.matches(query, query_node, text_provider);

    query_matches
        .map(|query_match| query_match.captures)
        .filter(|captures| captures.iter().any(|capture| capture.node == node))
        .map(sort_captures)
        .collect()
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
            start: self.start_point.to_line_column(),
            end: self.end_point.to_line_column(),
        }
    }
}

trait ToLineColumn {
    fn to_line_column(&self) -> LineColumn;
}

impl ToLineColumn for Point {
    fn to_line_column(&self) -> LineColumn {
        LineColumn {
            line: self.row + 1,
            column: self.column,
        }
    }
}
