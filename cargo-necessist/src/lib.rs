#![feature(box_patterns)]
#![warn(clippy::expect_used)]
#![warn(clippy::unwrap_used)]
#![warn(clippy::panic)]

use ansi_term::{
    Color::{Cyan, Green, Red, Yellow},
    Style,
};
use anyhow::{ensure, Result};
use cargo::{
    core::{package::Package, Workspace},
    sources::PathSource,
    util::config::Config,
};
use clap::Clap;
use log::debug;
use necessist_common::{self as necessist, removed_message};
use regex::Regex;
use std::{
    cell::Cell,
    ffi::OsStr,
    fmt::Debug,
    fs::File,
    io::Read,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};
use subprocess::{Exec, NullFile, Redirection};
use syn::{
    export::ToTokens,
    spanned::Spanned,
    visit::{visit_stmt, Visit},
    Expr, ExprCall, ExprMacro, ExprMethodCall, ExprPath, ItemFn, Macro, PathSegment, Stmt,
};

const LEVEL_SKIPPED: u32 = 1;
const LEVEL_NONBUILDABLE: u32 = 2;
const LEVEL_FAILED_NONLOCAL: u32 = 3;
const LEVEL_FAILED_LOCAL: u32 = 4;
const LEVEL_MAX: u32 = u32::MAX;

const MACRO_WHITELIST: &[&str] = &[
    "assert",
    "assert_eq",
    "assert_ne",
    "panic",
    "unimplemented",
    "unreachable",
];

mod removal {
    pub enum Result {
        Inconclusive,
        Skipped,
        Nonbuildable,
        Failed,
        Passed,
    }

    impl std::fmt::Display for Result {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "{}",
                match self {
                    Result::Inconclusive => "inconclusive",
                    Result::Skipped => "skipped",
                    Result::Nonbuildable => "nonbuildable",
                    Result::Failed => "failed",
                    Result::Passed => "passed",
                }
            )
        }
    }
}

fn level(stmt: &Stmt, result: &removal::Result) -> u32 {
    match result {
        removal::Result::Inconclusive => LEVEL_MAX,
        removal::Result::Skipped => LEVEL_SKIPPED,
        removal::Result::Nonbuildable => LEVEL_NONBUILDABLE,
        removal::Result::Failed => {
            if matches!(stmt, Stmt::Local(_)) {
                LEVEL_FAILED_LOCAL
            } else {
                LEVEL_FAILED_NONLOCAL
            }
        }
        removal::Result::Passed => LEVEL_MAX,
    }
}

fn style(result: &removal::Result) -> Style {
    match result {
        removal::Result::Inconclusive => Red.normal(),
        removal::Result::Skipped => Yellow.normal(),
        removal::Result::Nonbuildable => Style::default(),
        removal::Result::Failed => Cyan.normal(),
        removal::Result::Passed => Green.normal(),
    }
}

#[derive(Clap, Debug)]
struct Opts {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Clap, Debug)]
enum SubCommand {
    Necessist(Necessist),
}

#[derive(Clap, Debug)]
struct Necessist {
    #[clap(
        short,
        long,
        about = "Quieter (-qqqq most quiet)",
        parse(from_occurrences)
    )]
    quiet: u32,
    #[clap(long, about = "Show build output (for debugging necessist)")]
    show_build: bool,
    #[clap(
        long,
        about = "Skip function calls, macro invocations, and method calls matching regex (https://docs.rs/regex)",
        value_name = "regex"
    )]
    skip_calls: Option<String>,
    #[clap(
        long,
        about = "Skip local (let) bindings (alias: --skip-lets)",
        alias = "skip-lets"
    )]
    skip_locals: bool,
}

pub fn cargo_necessist<T: AsRef<OsStr>>(args: &[T]) -> Result<()> {
    let SubCommand::Necessist(opts) = Opts::parse_from(args).subcmd;

    let skip_calls_re = opts
        .skip_calls
        .as_ref()
        .map(|re| Regex::new(&re))
        .transpose()?;

    let default_config = Config::default()?;

    let path = Path::new("./Cargo.toml").canonicalize()?;

    let ws = Workspace::new(&path, &default_config)?;

    if warnings_are_denied(&ws)? {
        warn("rust appears to be configured to deny warnings; allowing warnings is strongly recommended");
    }

    for pkg in ws.members() {
        if test(&opts, &pkg, None, false).is_err() {
            warn(&format!("{}: tests did not build; skipping", pkg.name()));
            continue;
        }

        if test(&opts, &pkg, None, true).is_err() {
            warn(&format!("{}: tests failed; skipping", pkg.name()));
            continue;
        }

        let paths = PathSource::new(pkg.root(), pkg.package_id().source_id(), &default_config)
            .list_files(pkg)?;

        for path in paths {
            debug!("{:?}", path);

            if path.extension() != Some(OsStr::new("rs")) {
                continue;
            }

            let mut file = File::open(&path)?;

            let mut content = String::new();
            file.read_to_string(&mut content)?;

            match syn::parse_file(&content) {
                Ok(file) => {
                    ItemFnVisitor {
                        context: &Context {
                            opts: &opts,
                            skip_calls_re: skip_calls_re.as_ref(),
                            ws: &ws,
                            pkg: &pkg,
                        },
                        path,
                    }
                    .visit_file(&file);
                }
                Err(err) => {
                    warn(&format!(
                        "Could not parse {}: {}",
                        path.to_string_lossy(),
                        err
                    ));
                }
            }
        }
    }

    Ok(())
}

struct Context<'a> {
    opts: &'a Necessist,
    skip_calls_re: Option<&'a Regex>,
    ws: &'a Workspace<'a>,
    pkg: &'a Package,
}

struct ItemFnVisitor<'a> {
    context: &'a Context<'a>,
    path: PathBuf,
}

impl<'ast, 'a> Visit<'ast> for ItemFnVisitor<'a> {
    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        if is_instrumented(item) {
            StmtVisitor {
                context: self.context,
                path: self.path.clone(),
                ident: item.sig.ident.to_string(),
                leaves_visited: Cell::new(0),
            }
            .visit_item_fn(item);
        }
    }
}

struct StmtVisitor<'a> {
    context: &'a Context<'a>,
    path: PathBuf,
    ident: String,
    leaves_visited: Cell<usize>,
}

impl<'ast, 'a> Visit<'ast> for StmtVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        let before = self.leaves_visited.get();
        visit_stmt(self, stmt);
        let after = self.leaves_visited.get();

        // smoelius: This is a leaf if-and-only-if no leaves were visited during the recursive call.
        if before != after {
            return;
        }
        self.leaves_visited.set(after + 1);

        let span = necessist::Span::from_span(
            &self
                .context
                .pkg
                .root()
                .parent()
                .unwrap()
                .to_path_buf()
                .join(&self.path),
            stmt.span(),
        );

        if is_whitelisted_macro(stmt)
            || (self.context.opts.skip_locals && matches!(stmt, Stmt::Local(_)))
            || self
                .context
                .skip_calls_re
                .map_or(false, |re| is_skipped_call(re, stmt))
        {
            self.emit(&span, stmt, removal::Result::Skipped);
            return;
        }

        if let Ok(removed) = test(
            &self.context.opts,
            &self.context.pkg,
            Some((&self.ident, &span)),
            false,
        ) {
            if !removed {
                self.emit(&span, stmt, removal::Result::Inconclusive);
                return;
            }

            if let Ok(removed) = test(
                &self.context.opts,
                &self.context.pkg,
                Some((&self.ident, &span)),
                true,
            ) {
                // smoelius: A "removed" message should be generated when the target is built, but
                // not when it is run.
                assert!(!removed);

                self.emit(&span, stmt, removal::Result::Passed);
                return;
            }

            self.emit(&span, stmt, removal::Result::Failed);
            return;
        }

        self.emit(&span, stmt, removal::Result::Nonbuildable);
    }
}

impl<'a> StmtVisitor<'a> {
    fn emit(&self, span: &necessist::Span, stmt: &Stmt, result: removal::Result) {
        if self.context.opts.quiet < level(stmt, &result) {
            println!(
                "{}: `{}` {}",
                strip_span(self.context.ws.root(), &span).unwrap(),
                stmt.to_token_stream(),
                style(&result).bold().paint(result.to_string())
            )
        }
    }
}

fn strip_span(path: &Path, span: &necessist::Span) -> Result<necessist::Span> {
    let mut span = span.clone();
    span.source_file = span.source_file.strip_prefix(path)?.to_path_buf();
    Ok(span)
}

fn test(
    opts: &Necessist,
    pkg: &Package,
    ident_span: Option<(&str, &necessist::Span)>,
    run: bool,
) -> Result<bool> {
    let env = ident_span.map_or(vec![], |(_, span)| {
        let mut env = vec![("NECESSIST".to_owned(), "1".to_owned())];
        env.extend(span.to_vec());
        if opts.show_build {
            env.push(("NECESSIST_DEBUG".to_owned(), "1".to_owned()));
        }
        env
    });

    let pkg_name = pkg.name();
    let mut args = vec!["test", "-p", &pkg_name];
    if !run {
        args.extend(&["--no-run"]);
    } else {
        args.extend(&["--lib"]);
        // smoelius: This could run more tests than just the one that interests us. However, there
        // does not seem to be an easy way to match a test to the file in which it appears. So, for
        // now, this seems to be as good an approach as any.
        if let Some((ident, _)) = ident_span {
            args.extend(&["--", "--test", ident]);
        }
    }

    let mut exec = Exec::cmd("cargo")
        .env_extend(&env)
        .args(&args)
        .stdout(Redirection::Pipe);

    if !opts.show_build {
        exec = exec.stderr(NullFile);
    }

    debug!("{:?}", exec);

    let mut popen = exec.clone().popen()?;
    let stream = popen.stdout.take().unwrap();

    let mut removed = false;
    let removed_msg = ident_span.map(|(_, span)| removed_message(span));
    for line in BufReader::new(stream).lines() {
        let line = line?;
        debug! {"{}", line};
        removed |= removed_msg.as_ref().map_or(false, |msg| &line == msg);
    }

    let status = popen.wait()?;
    ensure!(status.success(), "command failed: {:?}", exec);

    Ok(removed)
}

fn warnings_are_denied(ws: &Workspace) -> Result<bool> {
    let config = ws.config().build_config()?;
    Ok(config.rustflags.as_ref().map_or(false, |list| {
        list.as_slice()
            .windows(2)
            .position(|pair| pair == &["-D", "warnings"])
            .is_some()
    }))
}

fn is_instrumented(item: &ItemFn) -> bool {
    item.attrs.iter().any(|attr| {
        attr.path
            .segments
            .iter()
            .all(|PathSegment { ident, arguments }| ident == "necessist" && arguments.is_empty())
    })
}

fn is_whitelisted_macro(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Expr(expr) => Some(expr),
        Stmt::Semi(expr, ..) => Some(expr),
        _ => None,
    }
    .map_or(false, |expr| match expr {
        Expr::Macro(ExprMacro {
            mac: Macro { path, .. },
            ..
        }) => path.get_ident().map_or(false, |ident| {
            MACRO_WHITELIST.contains(&ident.to_string().as_str())
        }),
        _ => false,
    })
}

fn is_skipped_call(re: &Regex, stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Expr(expr) => Some(expr),
        Stmt::Semi(expr, ..) => Some(expr),
        _ => None,
    }
    .map_or(None, |expr| match expr {
        Expr::Call(ExprCall {
            func: box Expr::Path(ExprPath { path, .. }),
            ..
        }) => path.get_ident(),
        Expr::Macro(ExprMacro {
            mac: Macro { path, .. },
            ..
        }) => path.get_ident(),
        Expr::MethodCall(ExprMethodCall { method, .. }) => Some(method),
        _ => None,
    })
    .map_or(false, |ident| re.is_match(&ident.to_string().as_str()))
}

fn warn(msg: &str) {
    println!("{}: {}", Yellow.bold().paint("warning"), msg);
}
