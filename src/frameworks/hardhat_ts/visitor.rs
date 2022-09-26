use crate::{LineColumn, SourceFile, Span};
use if_chain::if_chain;
use std::{
    path::{Path, PathBuf},
    rc::Rc,
};
use swc_common::{BytePos, Loc, SourceMap, Span as SwcSpan, Spanned};
use swc_ecma_ast::{
    CallExpr, Callee, Expr, ExprOrSpread, ExprStmt, MemberExpr, MemberProp, Module, Stmt,
    TsTypeParamInstantiation,
};
use swc_ecma_visit::{visit_call_expr, visit_stmt, Visit};

pub(super) fn visit(
    source_map: Rc<SourceMap>,
    root: Rc<PathBuf>,
    test_file: &Path,
    module: &Module,
) -> Vec<Span> {
    let mut visitor = Visitor::new(source_map, root, test_file);
    visitor.visit_module(module);
    visitor.spans
}

pub struct Visitor {
    source_map: Rc<SourceMap>,
    source_file: SourceFile,
    in_it_call_expr: bool,
    n_stmt_leaves_visited: usize,
    spans: Vec<Span>,
}

#[allow(dead_code)]
struct MethodCall<'a> {
    pub span: &'a SwcSpan,
    pub obj: &'a Expr,
    pub path: Vec<&'a MemberProp>,
    pub args: &'a Vec<ExprOrSpread>,
    pub type_args: &'a Option<Box<TsTypeParamInstantiation>>,
}

impl Visit for Visitor {
    fn visit_call_expr(&mut self, call_expr: &CallExpr) {
        if is_it_call_expr(call_expr) {
            assert!(!self.in_it_call_expr);
            self.in_it_call_expr = true;

            visit_call_expr(self, call_expr);

            assert!(self.in_it_call_expr);
            self.in_it_call_expr = false;

            return;
        }

        visit_call_expr(self, call_expr);

        if let Some(MethodCall {
            span, obj, path, ..
        }) = is_method_call(call_expr)
        {
            if self.in_it_call_expr && !is_ignored_method(&path) {
                let mut span = *span;
                span.lo = obj.span().hi;
                assert!(span.lo <= span.hi);
                self.elevate_span(span.to_internal_span(&self.source_map, &self.source_file));
            }
        }
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        let n_before = self.n_stmt_leaves_visited;
        visit_stmt(self, stmt);
        let n_after = self.n_stmt_leaves_visited;

        // smoelius: Consider this a "leaf" if-and-only-if no "leaves" were added during the
        // recursive call.
        if n_before != n_after {
            return;
        }
        self.n_stmt_leaves_visited += 1;

        if self.in_it_call_expr
            && !matches!(
                stmt,
                Stmt::Break(_) | Stmt::Continue(_) | Stmt::Decl(_) | Stmt::Return(_)
            )
            && !is_ignored_call_expr(stmt)
        {
            let span = stmt
                .span()
                .to_internal_span(&self.source_map, &self.source_file);
            self.elevate_span(span);
        }
    }
}

impl Visitor {
    fn new(source_map: Rc<SourceMap>, root: Rc<PathBuf>, test_file: &Path) -> Self {
        Self {
            source_map,
            source_file: SourceFile::new(root, Rc::new(test_file.to_path_buf())),
            in_it_call_expr: false,
            n_stmt_leaves_visited: 0,
            spans: Vec::new(),
        }
    }

    fn elevate_span(&mut self, span: Span) {
        self.spans.push(span);
    }
}

fn is_it_call_expr(call_expr: &CallExpr) -> bool {
    if_chain! {
        if let CallExpr {
            callee: Callee::Expr(callee),
            ..
        } = call_expr;
        if let Expr::Ident(ident) = &**callee;
        if ident.as_ref() == "it";
        then {
            true
        } else {
            false
        }
    }
}

fn is_method_call(call_expr: &CallExpr) -> Option<MethodCall> {
    if let CallExpr {
        span,
        callee: Callee::Expr(ref expr),
        args,
        type_args,
    } = call_expr
    {
        let mut expr = expr;
        let mut path_reversed = Vec::new();
        while let Expr::Member(MemberExpr { span: _, obj, prop }) = &**expr {
            expr = obj;
            path_reversed.push(prop);
        }
        if path_reversed.is_empty() {
            None
        } else {
            Some(MethodCall {
                span,
                obj: expr,
                path: {
                    path_reversed.reverse();
                    path_reversed
                },
                args,
                type_args,
            })
        }
    } else {
        None
    }
}

fn is_ignored_method(path: &[&MemberProp]) -> bool {
    if let Some(MemberProp::Ident(ident)) = path.first() {
        ident.as_ref() == "to"
    } else {
        false
    }
}

fn is_ignored_call_expr(stmt: &Stmt) -> bool {
    if let Stmt::Expr(ExprStmt { ref expr, .. }) = stmt {
        let mut expr = expr;
        loop {
            match &**expr {
                Expr::Await(await_expr) => {
                    expr = &await_expr.arg;
                }
                Expr::Member(member_expr) => {
                    expr = &member_expr.obj;
                }
                Expr::Call(CallExpr {
                    callee: Callee::Expr(callee),
                    ..
                }) => {
                    if_chain! {
                        if let Expr::Ident(ident) = &**callee;
                        if ident.as_ref() == "expect";
                        then {
                            return true;
                        } else {
                            expr = callee;
                        }
                    }
                }
                _ => {
                    return false;
                }
            }
        }
    } else {
        false
    }
}

trait ToInternalSpan {
    fn to_internal_span(&self, source_map: &SourceMap, source_file: &SourceFile) -> Span;
}

impl ToInternalSpan for SwcSpan {
    fn to_internal_span(&self, source_map: &SourceMap, source_file: &SourceFile) -> Span {
        Span {
            source_file: source_file.clone(),
            start: self.lo.to_line_column(source_map),
            end: self.hi.to_line_column(source_map),
        }
    }
}

trait ToLineColumn {
    fn to_line_column(&self, source_map: &SourceMap) -> LineColumn;
}

impl ToLineColumn for BytePos {
    fn to_line_column(&self, source_map: &SourceMap) -> LineColumn {
        let Loc {
            line, col_display, ..
        } = source_map.lookup_char_pos(*self);
        LineColumn {
            line,
            column: col_display,
        }
    }
}
