use necessist_core::{LineColumn, SourceFile, Span};
use tree_sitter::{Node, Point, Range, TreeCursor};

pub trait ToInternalSpan {
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

pub trait ToLineColumn {
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
