use super::{AbstractTypes, MaybeNamed, Named, ParseLow, Spanned};
use if_chain::if_chain;
use necessist_core::{
    config,
    framework::{SpanKind, TestSpanMaps},
    LightContext, SourceFile, Span,
};
use paste::paste;
use std::cell::RefCell;

pub struct GenericVisitor<'context, 'config, 'backend, 'ast, T: ParseLow> {
    pub context: &'context LightContext<'context>,
    pub config: &'config config::Compiled,
    pub backend: &'backend mut T,
    pub source_file: SourceFile,
    pub test_name: Option<String>,
    pub last_statement_in_test: Option<<T::Types as AbstractTypes>::Statement<'ast>>,
    pub n_statement_leaves_visited: usize,
    pub n_before: Vec<usize>,
    pub call_statement: Option<<T::Types as AbstractTypes>::Statement<'ast>>,
    pub test_span_maps: TestSpanMaps,
}

/// `call_info` return values. See that method for details.
struct CallInfo {
    span: Span,
    is_method: bool,
    is_ignored: bool,
    is_nested: bool,
}

struct VisitMaybeMacroCallArgs<'ast, 'storage, 'span, T: ParseLow> {
    // smoelius: Maybe remove this `storage` field?
    _storage: &'storage RefCell<<T::Types as AbstractTypes>::Storage<'ast>>,
    span: &'span Span,
    is_ignored_as_call: bool,
    is_method_call: bool,
    is_ignored_as_method_call: bool,
}

// smoelius: The things we want to remove are only:
// - entire statements (function, macro, or method calls with the receiver)
// - method calls (without the receiver)
// So, for example, a function, macro, or method call that is a method call receiver should not be
// removed because it is necessarily not an entire statement.
macro_rules! visit_maybe_macro_call {
    ($this:expr, $args:expr) => {
        paste! {
            let statement = $this.call_statement.take();

            if_chain! {
                if let Some(test_name) = $this.test_name.clone();
                if statement.map_or(true, |statement| {
                    $this.backend.statement_is_removable(statement)
                        && !$this.is_last_statement_in_test(statement)
                });
                then {
                    if let Some(statement) = statement {
                        if !$args.is_ignored_as_call {
                            let span = statement.span(&$this.source_file);
                            $this.register_span(span, &test_name, SpanKind::Statement);
                        }
                    }

                    // smoelius: If the entire call is ignored, then treat the method call as
                    // ignored as well.
                    if !$args.is_ignored_as_call && $args.is_method_call && !$args.is_ignored_as_method_call {
                        $this.register_span($args.span.clone(), &test_name, SpanKind::MethodCall);
                    }

                    // smoelius: Return false (i.e., don't descend into the call arguments) only if
                    // the call or method call is ignored.
                    !$args.is_ignored_as_call && !$args.is_ignored_as_method_call
                } else {
                    true
                }
            }
        }
    };
}

macro_rules! visit_call_post {
    ($this:expr, $storage:expr) => {};
}

impl<'context, 'config, 'backend, 'ast, T: ParseLow>
    GenericVisitor<'context, 'config, 'backend, 'ast, T>
{
    pub fn test_span_maps(self) -> TestSpanMaps {
        self.test_span_maps
    }

    #[allow(clippy::unnecessary_wraps)]
    pub fn visit_test(
        &mut self,
        storage: &RefCell<<T::Types as AbstractTypes>::Storage<'ast>>,
        test: <T::Types as AbstractTypes>::Test<'ast>,
    ) -> bool {
        let name = test.name();

        if self.config.is_ignored_test(&name) {
            return false;
        }

        assert!(self.test_name.is_none());
        self.test_name = Some(name);

        let statements = self.backend.test_statements(storage, test);

        assert!(self.last_statement_in_test.is_none());
        self.last_statement_in_test = statements.split_last().map(|(&statement, _)| statement);

        true
    }

    pub fn visit_test_post(
        &mut self,
        _storage: &RefCell<<T::Types as AbstractTypes>::Storage<'ast>>,
        test: <T::Types as AbstractTypes>::Test<'ast>,
    ) {
        self.last_statement_in_test = None;

        // smoelius: Check whether the test was ignored.
        if self.test_name.is_none() {
            debug_assert!(self.config.is_ignored_test(&test.name()));
            return;
        }

        let name = test.name();
        assert!(self.test_name == Some(name));
        self.test_name = None;
    }

    pub fn visit_statement(
        &mut self,
        storage: &RefCell<<T::Types as AbstractTypes>::Storage<'ast>>,
        statement: <T::Types as AbstractTypes>::Statement<'ast>,
    ) -> bool {
        let n_before = self.n_statement_leaves_visited;
        self.n_before.push(n_before);

        if self.statement_is_call(storage, statement) {
            assert!(self.call_statement.is_none());
            self.call_statement = Some(statement);
        }

        true
    }

    pub fn visit_statement_post(
        &mut self,
        storage: &RefCell<<T::Types as AbstractTypes>::Storage<'ast>>,
        statement: <T::Types as AbstractTypes>::Statement<'ast>,
    ) {
        // smoelius: `self.call_statement` should have been cleared by `visit_maybe_macro_call!`. If
        // not, it's a bug.
        assert!(self.call_statement.is_none());

        let n_before = self.n_before.pop().unwrap();
        let n_after = self.n_statement_leaves_visited;

        // smoelius: Consider this a "leaf" if-and-only-if no "leaves" were added during the
        // recursive call.
        // smoelius: Note that ignored leaves should still be counted as leaves.
        if n_before != n_after {
            return;
        }
        self.n_statement_leaves_visited += 1;

        // smoelius: Call/macro call statements are handled by the visit/visit-post functions
        // specific to the call type.
        if let Some(test_name) = self.test_name.as_ref() {
            if self.backend.statement_is_removable(statement)
                && !self.is_last_statement_in_test(statement)
                && !self.statement_is_call(storage, statement)
                && !self.backend.statement_is_control(storage, statement)
                && !self.backend.statement_is_declaration(storage, statement)
            {
                let span = statement.span(&self.source_file);
                self.register_span(span, &test_name.clone(), SpanKind::Statement);
            }
        }
    }

    // smoelius: If `visit_call` returns false and the call is a method call, the
    // framework-specific visitor should still descend into the method call receiver.
    pub fn visit_call(
        &mut self,
        storage: &RefCell<<T::Types as AbstractTypes>::Storage<'ast>>,
        call: <T::Types as AbstractTypes>::Call<'ast>,
    ) -> bool {
        let call_span = call.span(&self.source_file);
        if let Some((field, name)) = self.callee_is_named_field(storage, call) {
            let inner_most_call_info = self.call_info(storage, &call_span, field, &name, true);
            let call_info = self.call_info(storage, &call_span, field, &name, false);
            assert!(call_info.is_method);
            visit_maybe_macro_call! {
                self,
                (VisitMaybeMacroCallArgs::<'_, '_, '_, T> {
                    _storage: storage,
                    span: &call_info.span,
                    is_ignored_as_call: (!inner_most_call_info.is_method && inner_most_call_info.is_ignored)
                        || (!inner_most_call_info.is_nested && call_info.is_ignored),
                    is_method_call: true,
                    is_ignored_as_method_call: call_info.is_ignored
                })
            }
        } else {
            let is_ignored_as_call = call
                .name()
                .map_or(false, |name| self.config.is_ignored_function(&name));
            visit_maybe_macro_call! {
                self,
                (VisitMaybeMacroCallArgs::<'_, '_, '_, T> {
                    _storage: storage,
                    span: &call_span,
                    is_ignored_as_call,
                    is_method_call: false,
                    is_ignored_as_method_call: false
                })
            }
        }
    }

    #[allow(clippy::unused_self)]
    pub fn visit_call_post(
        &mut self,
        _storage: &RefCell<<T::Types as AbstractTypes>::Storage<'ast>>,
        _call: <T::Types as AbstractTypes>::Call<'ast>,
    ) {
        visit_call_post! {
            self,
            storage
        }
    }

    pub fn visit_macro_call(
        &mut self,
        storage: &RefCell<<T::Types as AbstractTypes>::Storage<'ast>>,
        macro_call: <T::Types as AbstractTypes>::MacroCall<'ast>,
    ) -> bool {
        let name = macro_call.name();
        visit_maybe_macro_call! {
            self,
            (VisitMaybeMacroCallArgs::<'_, '_, '_, T> {
                _storage: storage,
                span: &macro_call.span(&self.source_file),
                is_ignored_as_call: self.config.is_ignored_macro(&name),
                is_method_call: false,
                is_ignored_as_method_call: false
            })
        }
    }

    #[allow(clippy::unused_self)]
    pub fn visit_macro_call_post(
        &mut self,
        _storage: &RefCell<<T::Types as AbstractTypes>::Storage<'ast>>,
        _macro_call: <T::Types as AbstractTypes>::MacroCall<'ast>,
    ) {
        visit_call_post! {
            self,
            storage
        }
    }

    fn is_last_statement_in_test(
        &self,
        statement: <T::Types as AbstractTypes>::Statement<'ast>,
    ) -> bool {
        self.last_statement_in_test == Some(statement)
    }

    fn statement_is_call(
        &self,
        storage: &RefCell<<T::Types as AbstractTypes>::Storage<'ast>>,
        statement: <T::Types as AbstractTypes>::Statement<'ast>,
    ) -> bool {
        let Some(mut expression) = self.backend.statement_is_expression(storage, statement) else {
            return false;
        };

        loop {
            #[allow(clippy::needless_bool)]
            if let Some(await_) = self.backend.expression_is_await(storage, expression) {
                expression = self.backend.await_arg(storage, await_);
            } else if let Some(field) = self.backend.expression_is_field(storage, expression) {
                expression = self.backend.field_base(storage, field);
            } else if self
                .backend
                .expression_is_call(storage, expression)
                .is_some()
                || self
                    .backend
                    .expression_is_macro_call(storage, expression)
                    .is_some()
            {
                return true;
            } else {
                return false;
            }
        }
    }

    fn register_span(&mut self, span: Span, test_name: &str, kind: SpanKind) {
        let test_span_map = match kind {
            SpanKind::Statement => &mut self.test_span_maps.statement,
            SpanKind::MethodCall => &mut self.test_span_maps.method_call,
        };
        let test_names = test_span_map.entry(span).or_default();
        test_names.insert(test_name.to_owned());
    }

    /// Serves two functions that would require similar implementations:
    /// - extracting method call paths, e.g.:
    ///
    ///   ```ignore
    ///   operator.connect(...).signer.sendTransaction(...)
    ///                         ^^^^^^^^^^^^^^^^^^^^^^
    ///    ```
    ///
    /// - extracting the innermost function or macro call path, e.g.:
    ///
    ///   ```ignore
    ///   operator.connect(...).signer.sendTransaction(...)
    ///   ^^^^^^^^^^^^^^^^
    ///   ```
    ///
    /// For the latter, the `innermost` flag must be set to true. Roughly speaking, `call_info`
    /// walks the expression from right to left. When it encounters arguments (`(...)`), it either
    /// returns the accumulated method path (when `innermost` is not set), or recurses (when
    /// `innermost` is set).
    ///
    /// `call_info`'s return value includes the call span, whether the call is a method call,
    /// whether the call is ignored, and whether the call is nested (i.e., whether `call_info`
    /// recurse). For the "is ignored" part, `call_info` can tell which `is_ignored` method to use,
    /// because it can tell from the context the call's type (i.e., function, macro, or method
    /// call).
    fn call_info(
        &mut self,
        storage: &RefCell<<T::Types as AbstractTypes>::Storage<'ast>>,
        call_span: &Span,
        field: <T::Types as AbstractTypes>::Field<'ast>,
        name: &str,
        innermost: bool,
    ) -> CallInfo {
        self.call_info_inner(storage, call_span, field, name, innermost, false)
    }

    fn call_info_inner(
        &mut self,
        storage: &RefCell<<T::Types as AbstractTypes>::Storage<'ast>>,
        call_span: &Span,
        mut field: <T::Types as AbstractTypes>::Field<'ast>,
        name: &str,
        innermost: bool,
        recursed: bool,
    ) -> CallInfo {
        let mut base = self.backend.field_base(storage, field);

        let mut path_rev = vec![name.to_owned()];

        while let Some((field_inner, name_inner)) = self.field_base_is_named_field(storage, field) {
            base = self.backend.field_base(storage, field_inner);
            path_rev.push(name_inner);
            field = field_inner;
        }

        let path = path_rev
            .iter()
            .map(String::as_str)
            .rev()
            .collect::<Vec<_>>()
            .join(".");

        if let Some(call) = self.backend.expression_is_call(storage, base) {
            if innermost {
                return if let Some((field, name)) = self.callee_is_named_field(storage, call) {
                    self.call_info_inner(
                        storage,
                        &call.span(&self.source_file),
                        field,
                        &name,
                        innermost,
                        true,
                    )
                } else {
                    let name = call.name();
                    let is_ignored = name
                        .as_ref()
                        .map_or(false, |name| self.config.is_ignored_function(name));
                    CallInfo {
                        span: call.span(&self.source_file),
                        is_method: false,
                        is_ignored,
                        is_nested: true,
                    }
                };
            }
        } else if let Some(macro_call) = self.backend.expression_is_macro_call(storage, base) {
            if innermost {
                let name = macro_call.name();
                let is_ignored = self.config.is_ignored_macro(&name);
                return CallInfo {
                    span: macro_call.span(&self.source_file),
                    is_method: false,
                    is_ignored,
                    is_nested: recursed,
                };
            }
        } else if let Some(name) = base.name() {
            if innermost {
                let name = format!("{name}.{path}");
                let is_ignored_as_function = self.config.is_ignored_function(&name);
                let is_ignored_as_method = self.config.is_ignored_method(&path);
                let is_ignored = match self.config.ignored_path_disambiguation() {
                    config::IgnoredPathDisambiguation::None => {
                        is_ignored_as_function || is_ignored_as_method
                    }
                    config::IgnoredPathDisambiguation::Function => is_ignored_as_function,
                    config::IgnoredPathDisambiguation::Method => is_ignored_as_method,
                };
                return CallInfo {
                    span: call_span.clone(),
                    is_method: false,
                    is_ignored,
                    is_nested: recursed,
                };
            }
        }

        let path_span = call_span
            .with_start(base.span(&self.source_file).end)
            .trim_start();
        let is_ignored = self.config.is_ignored_method(&path);
        CallInfo {
            span: path_span,
            is_method: true,
            is_ignored,
            is_nested: recursed,
        }
    }

    fn field_base_is_named_field(
        &self,
        storage: &RefCell<<T::Types as AbstractTypes>::Storage<'ast>>,
        field: <T::Types as AbstractTypes>::Field<'ast>,
    ) -> Option<(<T::Types as AbstractTypes>::Field<'ast>, String)> {
        let expression = self.backend.field_base(storage, field);
        if_chain! {
            if let Some(field) = self.backend.expression_is_field(storage, expression);
            if let Some(name) = field.name();
            then {
                Some((field, name))
            } else {
                None
            }
        }
    }

    fn callee_is_named_field(
        &self,
        storage: &RefCell<<T::Types as AbstractTypes>::Storage<'ast>>,
        call: <T::Types as AbstractTypes>::Call<'ast>,
    ) -> Option<(<T::Types as AbstractTypes>::Field<'ast>, String)> {
        let expression = self.backend.call_callee(storage, call);
        if_chain! {
            if let Some(field) = self.backend.expression_is_field(storage, expression);
            if let Some(name) = field.name();
            then {
                Some((field, name))
            } else {
                None
            }
        }
    }
}
