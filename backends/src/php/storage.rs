use tree_sitter::Tree;

pub struct Storage<'ast> {
    pub text: &'ast str,
}

impl<'ast> Storage<'ast> {
    pub fn new(file: &'ast (String, Tree)) -> Self {
        Self { text: &file.0 }
    }
}
