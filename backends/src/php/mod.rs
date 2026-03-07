use super::tree_sitter_utils::{BoundedCursor, ToInternalSpan};
use super::{
    AbstractTypes, GenericVisitor, MaybeNamed, Named, ParseLow, ProcessLines, RunLow, Spanned,
    WalkDirResult,
};
use anyhow::{Context, Result, anyhow};
use necessist_core::{
    LightContext, SourceFile, Span,
    framework::{SpanTestMaps, TestSet},
    util,
};
use std::{
    collections::BTreeMap, convert::Infallible, fs::read_to_string, path::Path, process::Command,
    sync::LazyLock,
};
use tree_sitter::{Language, Node, Parser, Query, Tree};

mod storage;
use storage::Storage;

mod visitor;
use visitor::visit;

static LANGUAGE: LazyLock<Language> =
    LazyLock::new(|| Language::from(tree_sitter_php::LANGUAGE_PHP));

fn valid_query(source: &str) -> Query {
    #[allow(clippy::unwrap_used)]
    Query::new(&LANGUAGE, source).unwrap()
}

// Tree-sitter field IDs
static NAME_FIELD: LazyLock<u16> = LazyLock::new(|| valid_field_id("name"));
static OBJECT_FIELD: LazyLock<u16> = LazyLock::new(|| valid_field_id("object"));
static FUNCTION_FIELD: LazyLock<u16> = LazyLock::new(|| valid_field_id("function"));
static SCOPE_FIELD: LazyLock<u16> = LazyLock::new(|| valid_field_id("scope"));

fn valid_field_id(field_name: &str) -> u16 {
    LANGUAGE.field_id_for_name(field_name).unwrap().into()
}

// Tree-sitter node kind IDs
static COMPOUND_STATEMENT_KIND: LazyLock<u16> =
    LazyLock::new(|| non_zero_kind_id("compound_statement"));
static EXPRESSION_STATEMENT_KIND: LazyLock<u16> =
    LazyLock::new(|| non_zero_kind_id("expression_statement"));
static RETURN_STATEMENT_KIND: LazyLock<u16> =
    LazyLock::new(|| non_zero_kind_id("return_statement"));
static BREAK_STATEMENT_KIND: LazyLock<u16> = LazyLock::new(|| non_zero_kind_id("break_statement"));
static CONTINUE_STATEMENT_KIND: LazyLock<u16> =
    LazyLock::new(|| non_zero_kind_id("continue_statement"));
static THROW_EXPRESSION_KIND: LazyLock<u16> =
    LazyLock::new(|| non_zero_kind_id("throw_expression"));
static FUNCTION_CALL_EXPRESSION_KIND: LazyLock<u16> =
    LazyLock::new(|| non_zero_kind_id("function_call_expression"));
static MEMBER_CALL_EXPRESSION_KIND: LazyLock<u16> =
    LazyLock::new(|| non_zero_kind_id("member_call_expression"));
static NULLSAFE_MEMBER_CALL_EXPRESSION_KIND: LazyLock<u16> =
    LazyLock::new(|| non_zero_kind_id("nullsafe_member_call_expression"));
static SCOPED_CALL_EXPRESSION_KIND: LazyLock<u16> =
    LazyLock::new(|| non_zero_kind_id("scoped_call_expression"));
static MEMBER_ACCESS_EXPRESSION_KIND: LazyLock<u16> =
    LazyLock::new(|| non_zero_kind_id("member_access_expression"));
static NULLSAFE_MEMBER_ACCESS_EXPRESSION_KIND: LazyLock<u16> =
    LazyLock::new(|| non_zero_kind_id("nullsafe_member_access_expression"));
static SCOPED_PROPERTY_ACCESS_EXPRESSION_KIND: LazyLock<u16> =
    LazyLock::new(|| non_zero_kind_id("scoped_property_access_expression"));
static NAME_KIND: LazyLock<u16> = LazyLock::new(|| non_zero_kind_id("name"));
static QUALIFIED_NAME_KIND: LazyLock<u16> = LazyLock::new(|| non_zero_kind_id("qualified_name"));
static FUNCTION_DEFINITION_KIND: LazyLock<u16> =
    LazyLock::new(|| non_zero_kind_id("function_definition"));
static CLASS_DECLARATION_KIND: LazyLock<u16> =
    LazyLock::new(|| non_zero_kind_id("class_declaration"));
static FUNCTION_STATIC_DECLARATION_KIND: LazyLock<u16> =
    LazyLock::new(|| non_zero_kind_id("function_static_declaration"));
static GLOBAL_DECLARATION_KIND: LazyLock<u16> =
    LazyLock::new(|| non_zero_kind_id("global_declaration"));

fn non_zero_kind_id(kind: &str) -> u16 {
    let kind_id = LANGUAGE.id_for_node_kind(kind, true);
    assert_ne!(0, kind_id, "unknown node kind: {kind}");
    kind_id
}

fn is_call_kind(kind: u16) -> bool {
    kind == *FUNCTION_CALL_EXPRESSION_KIND
        || kind == *MEMBER_CALL_EXPRESSION_KIND
        || kind == *NULLSAFE_MEMBER_CALL_EXPRESSION_KIND
        || kind == *SCOPED_CALL_EXPRESSION_KIND
}

// Tree-sitter query for test method declarations
const TEST_METHOD_DECLARATION_SOURCE: &str = r#"
(method_declaration
    name: (name) @name
    (#match? @name "^test")
    body: (compound_statement) @body
)
"#;

static TEST_METHOD_DECLARATION_QUERY: LazyLock<Query> =
    LazyLock::new(|| valid_query(TEST_METHOD_DECLARATION_SOURCE));

pub(super) fn test_method_query() -> &'static Query {
    &TEST_METHOD_DECLARATION_QUERY
}

#[derive(Debug)]
pub struct Php;

impl Php {
    pub fn applicable(context: &LightContext) -> Result<bool> {
        let xml = context.root.join("php.xml").try_exists()?;
        let xml_dist = context.root.join("php.xml.dist").try_exists()?;
        Ok(xml || xml_dist)
    }

    pub fn new() -> Self {
        Self
    }
}

#[derive(Clone, Copy)]
pub struct Test<'ast> {
    name: &'ast str,
    body: Node<'ast>,
}

/// Thin wrapper around a tree-sitter node with the source text.
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

/// An expression node, optionally flagged as a "virtual field" when it
/// represents the callee portion of a member/scoped call expression.
#[derive(Clone, Copy)]
pub struct Expression<'ast> {
    inner: NodeWithText<'ast>,
    virtual_field: bool,
}

#[derive(Clone, Copy)]
pub struct Field<'ast>(NodeWithText<'ast>);

#[derive(Clone, Copy)]
pub struct Call<'ast>(NodeWithText<'ast>);

pub struct Types;

impl AbstractTypes for Types {
    type Storage<'ast> = Storage<'ast>;
    type File = (String, Tree);
    type Test<'ast> = Test<'ast>;
    type LocalFunction<'ast> = Infallible;
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
        self.inner.span(source_file)
    }
}

impl MaybeNamed for Expression<'_> {
    fn name(&self) -> Option<String> {
        let kind = self.inner.node.kind_id();
        if kind == *NAME_KIND || kind == *QUALIFIED_NAME_KIND {
            self.inner
                .node
                .utf8_text(self.inner.text.as_bytes())
                .ok()
                .map(ToOwned::to_owned)
        } else {
            None
        }
    }
}

impl Spanned for Field<'_> {
    fn span(&self, source_file: &SourceFile) -> Span {
        self.0.span(source_file)
    }
}

impl MaybeNamed for Field<'_> {
    fn name(&self) -> Option<String> {
        self.0
            .node
            .child_by_field_id(*NAME_FIELD)
            .and_then(|child| {
                child
                    .utf8_text(self.0.text.as_bytes())
                    .ok()
                    .map(ToOwned::to_owned)
            })
    }
}

impl Spanned for Call<'_> {
    fn span(&self, source_file: &SourceFile) -> Span {
        self.0.span(source_file)
    }
}

impl MaybeNamed for Call<'_> {
    fn name(&self) -> Option<String> {
        let kind = self.0.node.kind_id();
        if kind == *FUNCTION_CALL_EXPRESSION_KIND {
            self.0
                .node
                .child_by_field_id(*FUNCTION_FIELD)
                .and_then(|child| {
                    child
                        .utf8_text(self.0.text.as_bytes())
                        .ok()
                        .map(ToOwned::to_owned)
                })
        } else {
            // member_call_expression and scoped_call_expression are
            // handled via the callee_is_named_field path
            None
        }
    }
}

impl ParseLow for Php {
    type Types = Types;

    const IGNORED_FUNCTIONS: Option<&'static [&'static str]> =
        Some(&["var_dump", "print_r", "var_export"]);

    const IGNORED_MACROS: Option<&'static [&'static str]> = None;

    const IGNORED_METHODS: Option<&'static [&'static str]> = Some(&[
        "assert*",
        "expect*",
        "markTestIncomplete",
        "markTestSkipped",
    ]);

    fn walk_dir(&self, root: &Path) -> Box<dyn Iterator<Item = WalkDirResult>> {
        Box::new(
            walkdir::WalkDir::new(root)
                .into_iter()
                .filter_entry(|entry| {
                    // Skip vendor/ directories
                    if entry.path().is_dir() {
                        return entry.file_name() != "vendor";
                    }
                    entry.path().to_string_lossy().ends_with("Test.php")
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
            .with_context(|| "Failed to load PHP grammar")?;
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
        _file: &'ast <Self::Types as AbstractTypes>::File,
    ) -> Result<BTreeMap<String, Vec<<Self::Types as AbstractTypes>::LocalFunction<'ast>>>> {
        // PHP doesn't use the local function walking feature
        Ok(BTreeMap::new())
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
        assert_eq!(*COMPOUND_STATEMENT_KIND, test.body.kind_id());
        let source_text = storage.borrow().text;
        let mut cursor = test.body.walk();
        test.body
            .named_children(&mut cursor)
            .filter(|node| !node.is_extra())
            .map(|node| {
                Statement(NodeWithText {
                    text: source_text,
                    node,
                })
            })
            .collect()
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
        if statement.0.node.kind_id() == *EXPRESSION_STATEMENT_KIND {
            statement.0.node.named_child(0).map(|node| Expression {
                inner: NodeWithText {
                    text: statement.0.text,
                    node,
                },
                virtual_field: false,
            })
        } else {
            None
        }
    }

    fn statement_is_control<'ast>(
        &self,
        _storage: &std::cell::RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        statement: <Self::Types as AbstractTypes>::Statement<'ast>,
    ) -> bool {
        let kind = statement.0.node.kind_id();
        if [
            *RETURN_STATEMENT_KIND,
            *BREAK_STATEMENT_KIND,
            *CONTINUE_STATEMENT_KIND,
        ]
        .contains(&kind)
        {
            return true;
        }
        // A throw expression used as a statement
        if kind == *EXPRESSION_STATEMENT_KIND
            && let Some(child) = statement.0.node.named_child(0)
        {
            return child.kind_id() == *THROW_EXPRESSION_KIND;
        }
        false
    }

    fn statement_is_declaration<'ast>(
        &self,
        _storage: &std::cell::RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        statement: <Self::Types as AbstractTypes>::Statement<'ast>,
    ) -> bool {
        let kind = statement.0.node.kind_id();
        kind == *FUNCTION_DEFINITION_KIND
            || kind == *CLASS_DECLARATION_KIND
            || kind == *FUNCTION_STATIC_DECLARATION_KIND
            || kind == *GLOBAL_DECLARATION_KIND
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
        let kind = expression.inner.node.kind_id();
        // Standard field access: $obj->prop, $obj?->prop, Class::$prop
        if kind == *MEMBER_ACCESS_EXPRESSION_KIND
            || kind == *NULLSAFE_MEMBER_ACCESS_EXPRESSION_KIND
            || kind == *SCOPED_PROPERTY_ACCESS_EXPRESSION_KIND
        {
            return Some(Field(expression.inner));
        }
        // Virtual field: a member/scoped call expression returned by
        // call_callee, treated as a field for method name extraction
        if expression.virtual_field
            && (kind == *MEMBER_CALL_EXPRESSION_KIND
                || kind == *NULLSAFE_MEMBER_CALL_EXPRESSION_KIND
                || kind == *SCOPED_CALL_EXPRESSION_KIND)
        {
            return Some(Field(expression.inner));
        }
        None
    }

    fn expression_is_call<'ast>(
        &self,
        _storage: &std::cell::RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        expression: <Self::Types as AbstractTypes>::Expression<'ast>,
    ) -> Option<<Self::Types as AbstractTypes>::Call<'ast>> {
        if is_call_kind(expression.inner.node.kind_id()) {
            Some(Call(expression.inner))
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
        let kind = field.0.node.kind_id();
        // For scoped expressions, the base is in the `scope` field
        let field_id = if kind == *SCOPED_CALL_EXPRESSION_KIND
            || kind == *SCOPED_PROPERTY_ACCESS_EXPRESSION_KIND
        {
            *SCOPE_FIELD
        } else {
            *OBJECT_FIELD
        };
        Expression {
            inner: NodeWithText {
                text: field.0.text,
                node: field.0.node.child_by_field_id(field_id).unwrap(),
            },
            virtual_field: false,
        }
    }

    fn call_callee<'ast>(
        &self,
        _storage: &std::cell::RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        call: <Self::Types as AbstractTypes>::Call<'ast>,
    ) -> <Self::Types as AbstractTypes>::Expression<'ast> {
        let kind = call.0.node.kind_id();
        if kind == *FUNCTION_CALL_EXPRESSION_KIND {
            // For function calls, the callee is the `function` field
            Expression {
                inner: NodeWithText {
                    text: call.0.text,
                    node: call.0.node.child_by_field_id(*FUNCTION_FIELD).unwrap(),
                },
                virtual_field: false,
            }
        } else {
            // For member/scoped calls, return the call node itself
            // flagged as a virtual field so expression_is_field
            // can extract the method name
            Expression {
                inner: call.0,
                virtual_field: true,
            }
        }
    }

    fn macro_call_callee<'ast>(
        &self,
        _storage: &std::cell::RefCell<<Self::Types as AbstractTypes>::Storage<'ast>>,
        _macro_call: <Self::Types as AbstractTypes>::MacroCall<'ast>,
    ) -> <Self::Types as AbstractTypes>::Expression<'ast> {
        unreachable!()
    }
}

impl RunLow for Php {
    fn command_to_run_source_file(&self, context: &LightContext, source_file: &Path) -> Command {
        let mut command = Command::new("vendor/bin/phpunit");
        command.current_dir(context.root.as_path());
        command.arg(
            util::strip_prefix(source_file, context.root)
                .unwrap()
                .to_string_lossy()
                .to_string(),
        );
        command
    }

    fn instrument_source_file(
        &self,
        _context: &LightContext,
        _rewriter: &mut super::Rewriter,
        _source_file: &SourceFile,
        _n_instrumentable_statements: usize,
    ) -> Result<()> {
        // PHP doesn't need import injection — getenv() is built-in
        Ok(())
    }

    fn statement_prefix_and_suffix(&self, span: &Span) -> Result<(String, String)> {
        Ok((
            format!(r"if (getenv('NECESSIST_REMOVAL') !== '{}') {{ ", span.id()),
            " }".to_owned(),
        ))
    }

    fn command_to_build_source_file(&self, _context: &LightContext, source_file: &Path) -> Command {
        let mut command = Command::new("phpunit");
        command.arg("-l");
        command.arg(source_file);
        command
    }

    fn command_to_build_test(
        &self,
        _context: &LightContext,
        _test_name: &str,
        span: &Span,
    ) -> Command {
        let mut command = Command::new("phpunit");
        command.arg("-l");
        command.arg(&*span.source_file);
        command
    }

    fn command_to_run_test(
        &self,
        context: &LightContext,
        test_name: &str,
        span: &Span,
    ) -> (Command, Vec<String>, Option<ProcessLines>) {
        let mut command = Command::new("vendor/bin/phpunit");
        command.current_dir(context.root.as_path());
        command.args(["--filter", &format!("^.*::{test_name}$")]);
        command.arg(
            util::strip_prefix(&span.source_file, context.root)
                .unwrap()
                .to_string_lossy()
                .to_string(),
        );
        (command, Vec::new(), None)
    }
}
