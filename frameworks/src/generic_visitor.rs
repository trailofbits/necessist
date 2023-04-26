use super::{AbstractTypes, MaybeNamed, Named, ParseLow, Spanned};
use if_chain::if_chain;
use necessist_core::{Config, LightContext, SourceFile, Span};
use paste::paste;
use std::cell::RefCell;

pub struct GenericVisitor<'context, 'config, 'framework, 'ast, T: ParseLow + ?Sized> {
    pub context: &'context LightContext<'context>,
    pub config: &'config Config,
    pub framework: &'framework mut T,
    pub source_file: SourceFile,
    pub test_name: Option<String>,
    pub last_statement_in_test: Option<<T::Types as AbstractTypes>::Statement<'ast>>,
    pub n_statement_leaves_visited: usize,
    pub n_before: Vec<usize>,
    pub spans_visited: Vec<Span>,
}

macro_rules! visit_call {
    ($this:expr, $storage:expr, $inner_field_access:expr, $is_method_call_receiver:expr, $call:expr, $x:ident) => {
        paste! {
            let statement = $this.framework.[< $x _call_is_statement >]($storage, $call);

            if_chain! {
                if let Some(test_name) = $this.test_name.as_ref();
                if statement.map_or(true, |statement| !$this.is_last_statement_in_test(statement));
                if !$is_method_call_receiver;
                then {
                    let dotted_path = $call.name().map(|name| {
                        let mut path_rev = vec![name];

                        let mut maybe_field_access = $inner_field_access;
                        while let Some(field_access) = maybe_field_access {
                            let Some(name) = field_access.name() else {
                                break
                            };
                            path_rev.push(name);
                            maybe_field_access = $this.framework.field_access_has_inner_field_access($storage, field_access);
                        }

                        path_rev
                            .iter()
                            .map(String::as_str)
                            .rev()
                            .collect::<Vec<_>>()
                            .join(".")
                    });

                    // smoelius: Return false (i.e., don't descend into the call arguments) only if the
                    // callee is ignored.
                    if dotted_path.map_or(true, |dotted_path| !$this.[< is_ignored_ $x >]($this.config, &dotted_path)) {
                        let span = if let Some(statement) = statement {
                            statement.span(&$this.source_file)
                        } else {
                            $call.span(&$this.source_file)
                        };
                        $this.framework.on_candidate_found($this.context, $storage, &test_name, &span);
                        $this.spans_visited.push(span);
                        true
                    } else {
                        false
                    }
                } else {
                    true
                }
            }
        }
    };
}

macro_rules! visit_call_post {
    ($this:expr, $storage:expr, $call:expr, $x:ident) => {};
}

impl<'context, 'config, 'framework, 'ast, T: ParseLow>
    GenericVisitor<'context, 'config, 'framework, 'ast, T>
{
    pub fn spans_visited(self) -> Vec<Span> {
        self.spans_visited
    }

    #[allow(clippy::unnecessary_wraps)]
    pub fn visit_test(
        &mut self,
        storage: &RefCell<<T::Types as AbstractTypes>::Storage<'ast>>,
        test: <T::Types as AbstractTypes>::Test<'ast>,
    ) -> bool {
        let name = test.name();
        assert!(self.test_name.is_none());
        self.test_name = Some(name);

        let statements = self.framework.test_statements(storage, test);

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

        let name = test.name();
        assert!(self.test_name == Some(name));
        self.test_name = None;
    }

    pub fn visit_statement(
        &mut self,
        _storage: &RefCell<<T::Types as AbstractTypes>::Storage<'ast>>,
        _statement: <T::Types as AbstractTypes>::Statement<'ast>,
    ) -> bool {
        let n_before = self.n_statement_leaves_visited;
        self.n_before.push(n_before);

        true
    }

    pub fn visit_statement_post(
        &mut self,
        storage: &RefCell<<T::Types as AbstractTypes>::Storage<'ast>>,
        statement: <T::Types as AbstractTypes>::Statement<'ast>,
    ) {
        let n_before = self.n_before.pop().unwrap();
        let n_after = self.n_statement_leaves_visited;

        // smoelius: Consider this a "leaf" if-and-only-if no "leaves" were added during the
        // recursive call.
        // smoelius: Note that ignored leaves should still be counted as leaves.
        if n_before != n_after {
            return;
        }
        self.n_statement_leaves_visited += 1;

        // smoelius: Call statements are handled by the visit/visit-post functions specific
        // to the call type.
        if let Some(test_name) = self.test_name.as_ref() {
            if !self.is_last_statement_in_test(statement)
                && !self.framework.statement_is_call(storage, statement)
                && !self.framework.statement_is_control(storage, statement)
                && !self.framework.statement_is_declaration(storage, statement)
            {
                let span = statement.span(&self.source_file);
                self.framework
                    .on_candidate_found(self.context, storage, test_name, &span);
                self.spans_visited.push(span);
            }
        }
    }

    pub fn visit_function_call(
        &mut self,
        storage: &RefCell<<T::Types as AbstractTypes>::Storage<'ast>>,
        function_call: <T::Types as AbstractTypes>::FunctionCall<'ast>,
    ) -> bool {
        let inner_field_access = self
            .framework
            .function_call_has_inner_field_access(storage, function_call);
        let is_method_call_receiver = self
            .framework
            .function_call_is_method_call_receiver(storage, function_call);
        visit_call! {
            self,
            storage,
            inner_field_access,
            is_method_call_receiver,
            function_call,
            function
        }
    }

    #[allow(clippy::unused_self)]
    pub fn visit_function_call_post(
        &mut self,
        _storage: &RefCell<<T::Types as AbstractTypes>::Storage<'ast>>,
        _function_call: <T::Types as AbstractTypes>::FunctionCall<'ast>,
    ) {
        visit_call_post! {
            self,
            storage,
            function_call,
            function
        }
    }

    pub fn visit_macro_call(
        &mut self,
        storage: &RefCell<<T::Types as AbstractTypes>::Storage<'ast>>,
        macro_call: <T::Types as AbstractTypes>::MacroCall<'ast>,
    ) -> bool {
        let inner_field_access = None::<<T::Types as AbstractTypes>::FieldAccess<'ast>>;
        let is_method_call_receiver = self
            .framework
            .macro_call_is_method_call_receiver(storage, macro_call);
        visit_call! {
            self,
            storage,
            inner_field_access,
            is_method_call_receiver,
            macro_call,
            macro
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
            storage,
            macro_call,
            macro
        }
    }

    // smoelius: When `visit_method_call` returns false, the framework-specific visitor should still
    // traverse the receiver.
    pub fn visit_method_call(
        &mut self,
        storage: &RefCell<<T::Types as AbstractTypes>::Storage<'ast>>,
        method_call: <T::Types as AbstractTypes>::MethodCall<'ast>,
    ) -> bool {
        let inner_field_access = self
            .framework
            .method_call_has_inner_field_access(storage, method_call);
        let is_method_call_receiver = false;
        visit_call! {
            self,
            storage,
            inner_field_access,
            is_method_call_receiver,
            method_call,
            method
        }
    }

    #[allow(clippy::unused_self)]
    pub fn visit_method_call_post(
        &mut self,
        _storage: &RefCell<<T::Types as AbstractTypes>::Storage<'ast>>,
        _method_call: <T::Types as AbstractTypes>::MethodCall<'ast>,
    ) {
        visit_call_post! {
            self,
            storage,
            method_call,
            method
        }
    }

    fn is_last_statement_in_test(
        &self,
        statement: <T::Types as AbstractTypes>::Statement<'ast>,
    ) -> bool {
        self.last_statement_in_test == Some(statement)
    }

    #[allow(clippy::unused_self)]
    fn is_ignored_function(&self, config: &Config, dotted_path: &str) -> bool {
        T::IGNORED_FUNCTIONS.binary_search(&dotted_path).is_ok()
            || config.ignored_functions.iter().any(|s| s == dotted_path)
    }

    #[allow(clippy::unused_self)]
    fn is_ignored_macro(&self, config: &Config, dotted_path: &str) -> bool {
        T::IGNORED_MACROS.binary_search(&dotted_path).is_ok()
            || config.ignored_macros.iter().any(|s| s == dotted_path)
    }

    #[allow(clippy::unused_self)]
    fn is_ignored_method(&self, config: &Config, dotted_path: &str) -> bool {
        T::IGNORED_METHODS.binary_search(&dotted_path).is_ok()
            || config.ignored_methods.iter().any(|s| s == dotted_path)
    }
}
