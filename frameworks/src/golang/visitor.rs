use super::Golang;
use anyhow::Result;
use lazy_static::lazy_static;
use necessist_core::{LineColumn, SourceFile, Span};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
};
use tree_sitter::{Node, Point, Query, QueryCursor, QueryMatch, Range, Tree};

// smoelius: To future editors of this file: Tree-sitter Playground has been super helpful for
// debugging: https://tree-sitter.github.io/tree-sitter/playground

const TEST_FUNCTION_DECLARATION_SOURCE: &str = r#"
(function_declaration
    name: (
        (identifier) @name
        (#match? @name "^Test")
    )
    body: (block) @body
)
"#;

// smoelius: There is no "statement" node kind. So consider a node a statement if it is an immediate
// child of a block.
const STATEMENT_SOURCE: &str = r#"
(block
    "{"
    (_) @statement
    "}"
)
"#;

const METHOD_CALL_SOURCE: &str = r#"
(call_expression
    function: (selector_expression
        operand: (_) @receiver
        "." @dot
        field: (_) @method
    )
    arguments: (_) @arguments
)
"#;

lazy_static! {
    static ref TEST_FUNCTION_DECLARATION_QUERY: Query =
        valid_query(TEST_FUNCTION_DECLARATION_SOURCE);
    static ref STATEMENT_QUERY: Query = valid_query(STATEMENT_SOURCE);
    static ref METHOD_CALL_QUERY: Query = valid_query(METHOD_CALL_SOURCE);
}

fn valid_query(source: &str) -> Query {
    #[allow(clippy::unwrap_used)]
    Query::new(tree_sitter_go::language(), source).unwrap()
}

lazy_static! {
    static ref BREAK_STATEMENT_KIND: u16 = non_zero_kind_id("break_statement");
    static ref COMMENT_KIND: u16 = non_zero_kind_id("comment");
    static ref CONTINUE_STATEMENT_KIND: u16 = non_zero_kind_id("continue_statement");
    static ref DEFER_STATEMENT_KIND: u16 = non_zero_kind_id("defer_statement");
    static ref RETURN_STATEMENT_KIND: u16 = non_zero_kind_id("return_statement");
    static ref SHORT_VAR_DECLARATION_KIND: u16 = non_zero_kind_id("short_var_declaration");
    static ref VAR_DECLARATION_KIND: u16 = non_zero_kind_id("var_declaration");
}

fn non_zero_kind_id(kind: &str) -> u16 {
    let kind_id = tree_sitter_go::language().id_for_node_kind(kind, true);
    assert_ne!(0, kind_id);
    kind_id
}

#[cfg_attr(
    dylint_lib = "non_local_effect_before_error_return",
    allow(non_local_effect_before_error_return)
)]
pub(super) fn visit(
    framework: &mut Golang,
    root: Rc<PathBuf>,
    test_file: &Path,
    text: &str,
    tree: &Tree,
) -> Result<Vec<Span>> {
    let mut visitor = Visitor::new(framework, root, test_file, text);
    visitor.visit_tree(tree)?;
    Ok(visitor.spans)
}

struct Visitor<'framework, 'text> {
    framework: &'framework mut Golang,
    source_file: SourceFile,
    text: &'text str,
    test_name: Option<&'text str>,
    ranges_visited: HashMap<Range, usize>,
    spans: Vec<Span>,
}

impl<'framework, 'text> Visitor<'framework, 'text> {
    fn new(
        framework: &'framework mut Golang,
        root: Rc<PathBuf>,
        test_file: &Path,
        text: &'text str,
    ) -> Self {
        Self {
            framework,
            source_file: SourceFile::new(root, Rc::new(test_file.to_path_buf())),
            text,
            test_name: None,
            ranges_visited: HashMap::new(),
            spans: Vec::new(),
        }
    }

    fn elevate_span(&mut self, span: Span) {
        #[allow(clippy::expect_used)]
        self.framework.span_test_name_map.insert(
            span.clone(),
            self.test_name.expect("Test name is not set").to_owned(),
        );
        self.spans.push(span);
    }

    fn visit_tree(&mut self, tree: &Tree) -> Result<()> {
        for QueryMatch { captures, .. } in QueryCursor::new().matches(
            &TEST_FUNCTION_DECLARATION_QUERY,
            tree.root_node(),
            self.text.as_bytes(),
        ) {
            assert_eq!(2, captures.len());
            self.visit_test_function_declaration(captures[0].node, captures[1].node)?;
        }

        Ok(())
    }

    #[cfg_attr(
        dylint_lib = "non_local_effect_before_error_return",
        allow(non_local_effect_before_error_return)
    )]
    fn visit_test_function_declaration(&mut self, name: Node, body: Node) -> Result<()> {
        let name = name.utf8_text(self.text.as_bytes())?;

        assert!(self.test_name.is_none());
        self.test_name = Some(name);

        let result = self
            .visit_statements(body)
            .map(|_| self.visit_method_calls(body));

        assert!(self.test_name == Some(name));
        self.test_name = None;

        result
    }

    fn visit_statements(&mut self, node: Node) -> Result<usize> {
        let mut n_stmt_leaves_visited = 0;

        for QueryMatch { captures, .. } in
            QueryCursor::new().matches(&STATEMENT_QUERY, node, self.text.as_bytes())
        {
            assert_eq!(1, captures.len());
            n_stmt_leaves_visited += self.visit_statement(captures[0].node)?;
        }

        Ok(n_stmt_leaves_visited)
    }

    fn visit_statement(&mut self, node: Node) -> Result<usize> {
        if node.kind_id() == *COMMENT_KIND {
            return Ok(0);
        }

        let range = node.range();
        if let Some(&n_stmt_leaves_visited) = self.ranges_visited.get(&range) {
            return Ok(n_stmt_leaves_visited);
        }

        let mut n_stmt_leaves_visited = self.visit_statements(node)?;

        // smoelius: Consider this a "leaf" if-and-only-if no "leaves" were added during the
        // recursive call.
        if n_stmt_leaves_visited != 0 {
            self.ranges_visited.insert(range, n_stmt_leaves_visited);
            return Ok(n_stmt_leaves_visited);
        }

        // smoelius: For now, we remove method call statements, provided the method is not ignored.
        // Ideally, we would remove a statement like `x.foo()` when `x` is a package, but not when
        // `x` is an object. There is no obvious way to distinguish the two cases, so we remove the
        // statement either way.
        #[allow(clippy::nonminimal_bool)]
        if !self.is_ignored_method_call_statement(node)
            && !(node.kind_id() == *SHORT_VAR_DECLARATION_KIND
                || node.kind_id() == *VAR_DECLARATION_KIND)
            && !Self::is_control(node)
            && node.kind_id() != *DEFER_STATEMENT_KIND
        {
            let span = range.to_internal_span(&self.source_file);
            self.elevate_span(span);
        }

        n_stmt_leaves_visited += 1;
        self.ranges_visited.insert(range, n_stmt_leaves_visited);
        Ok(n_stmt_leaves_visited)
    }

    fn visit_method_calls(&mut self, node: Node) {
        for QueryMatch { captures, .. } in
            QueryCursor::new().matches(&METHOD_CALL_QUERY, node, self.text.as_bytes())
        {
            assert_eq!(4, captures.len());
            self.visit_method_call(
                captures[0].node,
                captures[1].node,
                captures[2].node,
                captures[3].node,
            );
        }
    }

    fn visit_method_call(&mut self, receiver: Node, dot: Node, method: Node, arguments: Node) {
        if self.is_ignored_method(receiver, method) {
            return;
        }

        let range = Range {
            start_byte: dot.start_byte(),
            end_byte: arguments.end_byte(),
            start_point: dot.start_position(),
            end_point: arguments.end_position(),
        };
        let span = range.to_internal_span(&self.source_file);
        self.elevate_span(span);
    }

    fn is_ignored_method_call_statement(&self, node: Node) -> bool {
        self.is_method_call_statement(node)
            .map_or(false, |[receiver, _, method, _]| {
                self.is_ignored_method(receiver, method)
            })
    }

    fn is_method_call_statement<'tree>(&self, node: Node<'tree>) -> Option<[Node<'tree>; 4]> {
        // smoelius: This is a bit of a hack. We search `node`'s descendants for method calls. A
        // match is a `call_expression` with an immediate child bound to `arguments`. So, for each
        // match, we ask whether `arguments`'s parent is `node`. If the answer to any of these
        // questions is "yes," then `node` is a method call. An alternative approach would be to
        // deconstruct `node`, but that would effectively mean reimplementing the method call query.
        for QueryMatch { captures, .. } in
            QueryCursor::new().matches(&METHOD_CALL_QUERY, node, self.text.as_bytes())
        {
            assert_eq!(4, captures.len());
            if captures[3].node.parent() == Some(node) {
                return Some([
                    captures[0].node,
                    captures[1].node,
                    captures[2].node,
                    captures[3].node,
                ]);
            }
        }
        None
    }

    fn is_control(node: Node) -> bool {
        [
            *BREAK_STATEMENT_KIND,
            *CONTINUE_STATEMENT_KIND,
            *RETURN_STATEMENT_KIND,
        ]
        .contains(&node.kind_id())
    }

    fn is_ignored_method(&self, receiver: Node, method: Node) -> bool {
        const IGNORED_METHODS: &[&str] = &[
            "Close", "Error", "Errorf", "Fail", "FailNow", "Fatal", "Fatalf", "Log", "Logf",
            "Parallel",
        ];

        receiver
            .utf8_text(self.text.as_bytes())
            .map_or(false, |s| s == "assert" || s == "require")
            || method
                .utf8_text(self.text.as_bytes())
                .map_or(false, |s| IGNORED_METHODS.binary_search(&s).is_ok())
    }
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
