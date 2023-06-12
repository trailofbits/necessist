use super::{
    bounded_cursor, cursor_matches, process_self_captures, valid_query, Call, GenericVisitor,
    Golang, Statement, Storage, Test, CALL_EXPRESSION_KIND,
};
use anyhow::Result;
use necessist_core::Span;
use once_cell::sync::Lazy;
use std::{cell::RefCell, iter::Peekable};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use tree_sitter::{Query, QueryCursor, QueryMatch, Tree};

macro_rules! trace {
    () => {
        log::trace!("{}:{}", file!(), line!())
    };
    ($expr:expr) => {
        log::trace!("{}:{}: {:?}", file!(), line!(), $expr)
    };
}

#[derive(Clone, Copy, Debug, EnumIter, PartialEq, PartialOrd)]
enum Possibility {
    Statement,
    CallExpression,
}

type PossibleIter = Peekable<PossibilityIter>;

const TEST_FUNCTION_DECLARATION_SOURCE: &str = r#"
(function_declaration
    name: (
        (identifier) @name
        (#match? @name "^Test")
    )
    body: (block) @body
)
"#;

const STATEMENT_SOURCE: &str = r#"
(_statement) @statement
"#;

static TEST_FUNCTION_DECLARATION_QUERY: Lazy<Query> =
    Lazy::new(|| valid_query(TEST_FUNCTION_DECLARATION_SOURCE));
static STATEMENT_QUERY: Lazy<Query> = Lazy::new(|| valid_query(STATEMENT_SOURCE));

#[cfg_attr(
    dylint_lib = "non_local_effect_before_error_return",
    allow(non_local_effect_before_error_return)
)]
pub(super) fn visit<'ast>(
    generic_visitor: GenericVisitor<'_, '_, '_, 'ast, Golang>,
    storage: &RefCell<Storage<'ast>>,
    tree: &'ast Tree,
) -> Result<Vec<Span>> {
    let mut visitor = Visitor::new(generic_visitor, storage);
    visitor.visit_tree(tree)?;
    Ok(visitor.generic_visitor.spans_visited())
}

struct Visitor<'context, 'config, 'framework, 'ast, 'storage> {
    generic_visitor: GenericVisitor<'context, 'config, 'framework, 'ast, Golang>,
    storage: &'storage RefCell<Storage<'ast>>,
}

impl<'context, 'config, 'framework, 'ast, 'storage>
    Visitor<'context, 'config, 'framework, 'ast, 'storage>
{
    fn new(
        generic_visitor: GenericVisitor<'context, 'config, 'framework, 'ast, Golang>,
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

    /// Visits `cursor`'s current node, which [`Self::visit_current_node_and_possibility`] has
    /// already determined to be a statement. Calls [`Self::walk_or_skip`] unconditionally, with
    /// `walk` set to the value [`GenericVisitor::visit_statement`] returns.
    fn visit_statement(
        &mut self,
        cursor: &mut bounded_cursor::BoundedCursor<'ast>,
        possible_iter: &mut PossibleIter,
    ) -> Result<()> {
        let node = cursor.current_node().unwrap();

        trace!(node);

        let statement = Statement(super::NodeWithText {
            text: self.storage.borrow().text,
            node,
        });

        let walk = self
            .generic_visitor
            .visit_statement(self.storage, statement);

        self.walk_or_skip(cursor, possible_iter, walk)?;

        self.generic_visitor
            .visit_statement_post(self.storage, statement);

        Ok(())
    }

    /// Visits `cursor`'s current node, which [`Self::visit_current_node_and_possibility`] has
    /// already determined to be a call. Calls [`Self::walk_or_skip`] unconditionally, with `walk`
    /// set to the value [`GenericVisitor::visit_call`] returns.
    fn visit_call(
        &mut self,
        cursor: &mut bounded_cursor::BoundedCursor<'ast>,
        possible_iter: &mut PossibleIter,
    ) -> Result<()> {
        let node = cursor.current_node().unwrap();

        trace!(node);

        let call = Call(super::NodeWithText {
            text: self.storage.borrow().text,
            node,
        });

        let walk = self.generic_visitor.visit_call(self.storage, call);

        self.walk_or_skip(cursor, possible_iter, walk)?;

        self.generic_visitor.visit_call_post(self.storage, call);

        Ok(())
    }

    /// If `walk` is true, calls [`Self::next_possibility`]; otherwise, skips `cursor`s current node
    /// and returns.
    fn walk_or_skip(
        &mut self,
        cursor: &mut bounded_cursor::BoundedCursor<'ast>,
        possible_iter: &mut PossibleIter,
        walk: bool,
    ) -> Result<bool> {
        trace!(walk);

        if !walk {
            cursor.skip();
            return Ok(true);
        }

        self.next_possibility(cursor, possible_iter, true)
    }

    /// Visits each descendant node in the subtree rooted at `cursor`s current node (unless a
    /// descendant node is an a subtree that is explicitly skipped by [`GenericVisitor`]). Calls
    /// [`Self::walk_possibilities`] on each such node.
    fn walk_nodes(&mut self, cursor: &mut bounded_cursor::BoundedCursor<'ast>) -> Result<()> {
        trace!();

        cursor.push();

        cursor.goto_next_node();

        while let Some(node) = cursor.current_node() {
            let matched = self.walk_possibilities(cursor)?;

            if !matched {
                cursor.goto_next_node();
            }

            assert_ne!(Some(node), cursor.current_node());
        }

        cursor.pop();

        Ok(())
    }

    /// Calls [`Self::visit_current_node_and_possibility`] with `cursor`'s current node and each
    /// [`Possibility`].
    fn walk_possibilities(
        &mut self,
        cursor: &mut bounded_cursor::BoundedCursor<'ast>,
    ) -> Result<bool> {
        trace!();

        let mut possible_iter = Possibility::iter().peekable();

        while let Some(possibility) = possible_iter.peek().copied() {
            let matched =
                self.visit_current_node_and_possibility(cursor, &mut possible_iter, false)?;

            if matched {
                return Ok(true);
            }

            assert_ne!(Some(possibility), possible_iter.peek().copied());
        }

        Ok(false)
    }

    /// Moves `possible_iter` to the next [`Possibility`] (if any) and does one of the following:
    /// - If `recurse` is true and `possible_iter` is not exhausted, calls
    ///   [`Self::visit_current_node_and_possibility`].
    /// - If `recurse` is true and `possible_iter` is exhausted, calls [`Self::walk_nodes`].
    /// - If `recurse` is false, returns `Ok(false)` (indicating the node and its subtree were not
    ///   explored).
    ///
    /// **This function is tricky because it may recurse or not, depending on `recurse`.**
    fn next_possibility(
        &mut self,
        cursor: &mut bounded_cursor::BoundedCursor<'ast>,
        possible_iter: &mut PossibleIter,
        recurse: bool,
    ) -> Result<bool> {
        trace!(recurse);

        let _ = possible_iter.next().unwrap();

        if recurse {
            if possible_iter.peek().is_some() {
                let _ = self.visit_current_node_and_possibility(cursor, possible_iter, true)?;
            } else {
                self.walk_nodes(cursor)?;
            }

            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Visits `cursor`'s current node and `possible_iter`'s current possibility. Returns a `bool`
    /// wrapped in a `Result`. That `bool` indicates whether `cursor`'s current node matched the
    /// current possibility, and so the node's subtree need not be considered further by
    /// [`Self::visit_current_node_and_possibility`]'s caller (which happens to be
    /// [`Self::walk_possibilities`]).
    fn visit_current_node_and_possibility(
        &mut self,
        cursor: &mut bounded_cursor::BoundedCursor<'ast>,
        possible_iter: &mut PossibleIter,
        recurse: bool,
    ) -> Result<bool> {
        let node = cursor.current_node().unwrap();

        let possibility = *possible_iter.peek().unwrap();

        trace!((recurse, node, possibility));

        match possibility {
            Possibility::Statement
                if process_self_captures(
                    &STATEMENT_QUERY,
                    node,
                    self.storage.borrow().text.as_bytes(),
                    |captures| captures.next().is_some(),
                ) =>
            {
                self.visit_statement(cursor, possible_iter)?;
                Ok(true)
            }

            Possibility::CallExpression if node.kind_id() == *CALL_EXPRESSION_KIND => {
                self.visit_call(cursor, possible_iter)?;
                Ok(true)
            }

            _ => self.next_possibility(cursor, possible_iter, recurse),
        }
    }
}
