use super::{
    BoundedCursor, COMPOUND_STATEMENT_KIND, Call, GenericVisitor, NodeWithText, PhpUnit, Statement,
    Storage, Test, is_call_kind, test_method_query,
};
use anyhow::Result;
use necessist_core::framework::{SpanTestMaps, TestSet};
use std::cell::RefCell;
use streaming_iterator::StreamingIterator;
use tree_sitter::{QueryCursor, QueryMatch, Tree};

pub(super) fn visit<'ast>(
    generic_visitor: GenericVisitor<'_, '_, '_, 'ast, PhpUnit>,
    storage: &RefCell<Storage<'ast>>,
    tree: &'ast Tree,
) -> Result<(TestSet, SpanTestMaps)> {
    let mut visitor = Visitor::new(generic_visitor, storage);
    visitor.visit_tree(tree)?;
    // No local function support for PHPUnit
    visitor.generic_visitor.results()
}

struct Visitor<'context, 'config, 'backend, 'ast, 'storage> {
    generic_visitor: GenericVisitor<'context, 'config, 'backend, 'ast, PhpUnit>,
    storage: &'storage RefCell<Storage<'ast>>,
}

impl<'context, 'config, 'backend, 'ast, 'storage>
    Visitor<'context, 'config, 'backend, 'ast, 'storage>
{
    fn new(
        generic_visitor: GenericVisitor<'context, 'config, 'backend, 'ast, PhpUnit>,
        storage: &'storage RefCell<Storage<'ast>>,
    ) -> Self {
        Self {
            generic_visitor,
            storage,
        }
    }

    fn visit_tree(&mut self, tree: &'ast Tree) -> Result<()> {
        let query = test_method_query();
        let mut cursor = QueryCursor::new();
        let mut query_matches = cursor.matches(
            query,
            tree.root_node(),
            self.storage.borrow().text.as_bytes(),
        );
        while let Some(query_match) = query_matches.next() {
            self.visit_test_method_declaration(query_match)?;
        }
        Ok(())
    }

    fn visit_test_method_declaration(&mut self, query_match: &QueryMatch<'_, 'ast>) -> Result<()> {
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
            self.walk_nodes(&mut BoundedCursor::new(body))?;
        }

        self.generic_visitor.visit_test_post(self.storage, test);

        Ok(())
    }

    fn visit_statement(&mut self, cursor: &mut BoundedCursor<'ast>) -> Result<()> {
        let node = cursor.current_node().unwrap();

        let statement = Statement(NodeWithText {
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

    fn visit_call(&mut self, cursor: &mut BoundedCursor<'ast>) -> Result<()> {
        let node = cursor.current_node().unwrap();

        let call = Call(NodeWithText {
            text: self.storage.borrow().text,
            node,
        });

        let walk = self.generic_visitor.visit_call(self.storage, call);

        self.walk_or_skip(cursor, walk)?;

        self.generic_visitor.visit_call_post(self.storage, call);

        Ok(())
    }

    fn walk_or_skip(&mut self, cursor: &mut BoundedCursor<'ast>, walk: bool) -> Result<()> {
        if walk {
            self.walk_nodes(cursor)?;
        } else {
            cursor.skip();
        }
        Ok(())
    }

    fn walk_nodes(&mut self, cursor: &mut BoundedCursor<'ast>) -> Result<()> {
        cursor.push();
        cursor.goto_next_node();

        while let Some(node) = cursor.current_node() {
            let matched = self.visit_current_node(cursor)?;

            if !matched {
                cursor.goto_next_node();
            }

            assert_ne!(Some(node), cursor.current_node());
        }

        cursor.pop();
        Ok(())
    }

    /// Returns true if the node's subtree was fully handled.
    fn visit_current_node(&mut self, cursor: &mut BoundedCursor<'ast>) -> Result<bool> {
        let node = cursor.current_node().unwrap();

        // Check if this is a PHP statement. Named children of
        // compound_statement are statements, but skip "extra" nodes
        // like comments (leaf nodes that would break BoundedCursor).
        if node.is_named()
            && !node.is_extra()
            && node
                .parent()
                .is_some_and(|p| p.kind_id() == *COMPOUND_STATEMENT_KIND)
        {
            self.visit_statement(cursor)?;
            return Ok(true);
        }

        // Check if this is a call expression
        if is_call_kind(node.kind_id()) {
            self.visit_call(cursor)?;
            return Ok(true);
        }

        Ok(false)
    }
}
