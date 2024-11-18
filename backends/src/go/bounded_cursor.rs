use tree_sitter::{Node, TreeCursor};

pub struct BoundedCursor<'tree> {
    cursor: TreeCursor<'tree>,
    bounds: Vec<Node<'tree>>,
    exhausted: bool,
}

impl<'tree> BoundedCursor<'tree> {
    pub fn new(node: Node<'tree>) -> Self {
        Self {
            cursor: node.walk(),
            bounds: Vec::new(),
            exhausted: false,
        }
    }

    pub fn current_node(&self) -> Option<Node<'tree>> {
        if self.exhausted {
            None
        } else {
            Some(self.cursor.node())
        }
    }

    pub fn push(&mut self) {
        assert!(!self.exhausted);

        self.bounds.push(self.cursor.node());
    }

    pub fn pop(&mut self) -> bool {
        let bound = self.bounds.pop().unwrap();

        while self.cursor.node() != bound {
            assert!(self.cursor.goto_parent());
        }

        self.skip()
    }

    pub fn goto_next_node(&mut self) -> bool {
        if self.cursor.goto_first_child() {
            return true;
        }

        self.skip()
    }

    pub fn skip(&mut self) -> bool {
        loop {
            if self.cursor.goto_next_sibling() {
                self.exhausted = false;
                return true;
            }

            if !self.cursor.goto_parent() {
                assert!(self.bounds.is_empty(), "{:#?}", self.bounds);
                self.exhausted = true;
                return false;
            }

            if self.at_current_bound() {
                self.exhausted = true;
                return false;
            }
        }
    }

    fn at_current_bound(&self) -> bool {
        self.bounds
            .last()
            .is_some_and(|&bound| bound == self.cursor.node())
    }
}
