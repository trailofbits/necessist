#![cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]

use super::{
    bounded_cursor, cursor_matches, process_self_captures, valid_query, Call, GenericVisitor, Go,
    Statement, Storage, Test, CALL_EXPRESSION_KIND,
};
use anyhow::Result;
use necessist_core::Span;
use once_cell::sync::Lazy;
use std::cell::RefCell;
use tree_sitter::{Query, QueryCursor, QueryMatch, Tree};

macro_rules! trace {
    () => {
        log::trace!("{}:{}", file!(), line!())
    };
    ($expr:expr) => {
        log::trace!("{}:{}: {:?}", file!(), line!(), $expr)
    };
}

const TEST_FUNCTION_DECLARATION_SOURCE: &str = r#"
(function_declaration
    name: (
        (identifier) @name
        (#match? @name "^Test")
    )
    body: (block) @body
)
"#;

const STATEMENT_SOURCE: &str = r"
(_statement) @statement
";

static TEST_FUNCTION_DECLARATION_QUERY: Lazy<Query> =
    Lazy::new(|| valid_query(TEST_FUNCTION_DECLARATION_SOURCE));
static STATEMENT_QUERY: Lazy<Query> = Lazy::new(|| valid_query(STATEMENT_SOURCE));

pub(super) fn visit<'ast>(
    generic_visitor: GenericVisitor<'_, '_, '_, 'ast, Go>,
    storage: &RefCell<Storage<'ast>>,
    tree: &'ast Tree,
) -> Result<Vec<Span>> {
    let mut visitor = Visitor::new(generic_visitor, storage);
    visitor.visit_tree(tree)?;
    Ok(visitor.generic_visitor.spans_visited())
}

struct Visitor<'context, 'config, 'framework, 'ast, 'storage> {
    generic_visitor: GenericVisitor<'context, 'config, 'framework, 'ast, Go>,
    storage: &'storage RefCell<Storage<'ast>>,
}

impl<'context, 'config, 'framework, 'ast, 'storage>
    Visitor<'context, 'config, 'framework, 'ast, 'storage>
{
    fn new(
        generic_visitor: GenericVisitor<'context, 'config, 'framework, 'ast, Go>,
        storage: &'storage RefCell<Storage<'ast>>,
    ) -> Self {
        Self {
            generic_visitor,
            storage,
        }
    }

    fn visit_tree(&mut self, tree: &'ast Tree) -> Result<()> {
        let mut cursor = QueryCursor::new();
        for query_match in cursor_matches(
            &mut cursor,
            &TEST_FUNCTION_DECLARATION_QUERY,
            tree.root_node(),
            self.storage.borrow().text.as_bytes(),
        ) {
            self.visit_test_function_declaration(&query_match)?;
        }

        Ok(())
    }

    fn visit_test_function_declaration(
        &mut self,
        query_match: &QueryMatch<'ast, '_>,
    ) -> Result<()> {
        assert_eq!(2, query_match.captures.len());

        let name = query_match
            .nodes_for_capture_index(0)
            .next()
            .unwrap()
            .utf8_text(self.storage.borrow().text.as_bytes())?;

        let body = query_match.nodes_for_capture_index(1).next().unwrap();

        let test = Test { name, body };

        let walk = self.generic_visitor.visit_test(self.storage, test);

        if walk {
            self.walk_nodes(&mut bounded_cursor::BoundedCursor::new(body))?;
        }

        self.generic_visitor.visit_test_post(self.storage, test);

        Ok(())
    }

    /// Visits `cursor`'s current node, which [`Self::visit_current_node`] has already determined to
    /// be a statement. Calls [`Self::walk_or_skip`] unconditionally, with `walk` set to the value
    /// [`GenericVisitor::visit_statement`] returns.
    fn visit_statement(&mut self, cursor: &mut bounded_cursor::BoundedCursor<'ast>) -> Result<()> {
        let node = cursor.current_node().unwrap();

        trace!(node);

        let statement = Statement(super::NodeWithText {
            text: self.storage.borrow().text,
            node,
        });

        let walk = self
            .generic_visitor
            .visit_statement(self.storage, statement);

        self.walk_or_skip(cursor, walk)?;

        self.generic_visitor
            .visit_statement_post(self.storage, statement);

        Ok(())
    }

    /// Visits `cursor`'s current node, which [`Self::visit_current_node`] has already determined to
    /// be a call. Calls [`Self::walk_or_skip`] unconditionally, with `walk` set to the value
    /// [`GenericVisitor::visit_call`] returns.
    fn visit_call(&mut self, cursor: &mut bounded_cursor::BoundedCursor<'ast>) -> Result<()> {
        let node = cursor.current_node().unwrap();

        trace!(node);

        let call = Call(super::NodeWithText {
            text: self.storage.borrow().text,
            node,
        });

        let walk = self.generic_visitor.visit_call(self.storage, call);

        self.walk_or_skip(cursor, walk)?;

        self.generic_visitor.visit_call_post(self.storage, call);

        Ok(())
    }

    /// If `walk` is true, calls [`Self::walk_nodes`]; otherwise, skips `cursor`s current node and
    /// returns.
    fn walk_or_skip(
        &mut self,
        cursor: &mut bounded_cursor::BoundedCursor<'ast>,
        walk: bool,
    ) -> Result<()> {
        trace!(walk);

        if walk {
            self.walk_nodes(cursor)?;
        } else {
            cursor.skip();
        }

        Ok(())
    }

    /// Visits each descendant node in the subtree rooted at `cursor`s current node (unless a
    /// descendant node is a subtree that is explicitly skipped by [`GenericVisitor`]). Calls
    /// [`Self::visit_current_node`] on each such node.
    fn walk_nodes(&mut self, cursor: &mut bounded_cursor::BoundedCursor<'ast>) -> Result<()> {
        trace!();

        cursor.push();

        cursor.goto_next_node();

        while let Some(node) = cursor.current_node() {
            let matched = self.visit_current_node(cursor, false)?;

            if !matched {
                cursor.goto_next_node();
            }

            assert_ne!(Some(node), cursor.current_node());
        }

        cursor.pop();

        Ok(())
    }

    /// Visits `cursor`'s current node. Returns a `bool` wrapped in a `Result`. That `bool`
    /// indicates whether `cursor`'s current node's subtree need not be considered further by
    /// [`Self::visit_current_node`]'s caller (which happens to be [`Self::walk_nodes`]).
    fn visit_current_node(
        &mut self,
        cursor: &mut bounded_cursor::BoundedCursor<'ast>,
        recurse: bool,
    ) -> Result<bool> {
        let node = cursor.current_node().unwrap();

        trace!((recurse, node));

        if process_self_captures(
            &STATEMENT_QUERY,
            node,
            self.storage.borrow().text.as_bytes(),
            |captures| captures.next().is_some(),
        ) {
            self.visit_statement(cursor)?;
            Ok(true)
        } else if node.kind_id() == *CALL_EXPRESSION_KIND {
            self.visit_call(cursor)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
