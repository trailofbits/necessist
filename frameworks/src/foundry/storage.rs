use solang_parser::pt::SourceUnit;

pub struct Storage<'ast> {
    pub contents: &'ast str,
}

impl<'ast> Storage<'ast> {
    pub fn new(file: &'ast (String, SourceUnit)) -> Self {
        Self { contents: &file.0 }
    }
}
