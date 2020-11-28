#![feature(box_patterns)]
#![feature(is_sorted)]
#![warn(clippy::expect_used)]
#![warn(clippy::unwrap_used)]
#![warn(clippy::panic)]

use ansi_term::{
    Color::{Blue, Cyan, Green, Red, Yellow},
    Style,
};
use anyhow::Result;
use cargo::{
    core::{package::Package, Workspace},
    sources::PathSource,
    util::config::Config,
};
use clap::Clap;
use git2::{Oid, Repository};
use log::debug;
use necessist_common::{self as necessist, removed_message};
use regex::Regex;
use rustorm::*;
use std::{
    cell::{Cell, RefCell},
    ffi::OsStr,
    fmt::Debug,
    fs::File,
    include_str,
    io::{self, ErrorKind, Read},
    path::Path,
    time::Duration,
};
use subprocess::{Exec, NullFile, PopenError, Redirection};
use syn::{
    export::ToTokens,
    spanned::Spanned,
    visit::{visit_stmt, Visit},
    Expr, ExprCall, ExprMacro, ExprMethodCall, ExprPath, ItemFn, Macro, PathSegment, Stmt,
};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

const LEVEL_SKIPPED: u32 = 1;
const LEVEL_NONBUILDABLE: u32 = 2;
const LEVEL_FAILED_NONLOCAL: u32 = 3;
const LEVEL_TIMEDOUT_NONLOCAL: u32 = 3;
const LEVEL_FAILED_LOCAL: u32 = 4;
const LEVEL_TIMEDOUT_LOCAL: u32 = 4;
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
        TimedOut,
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
                    Result::TimedOut => "timed-out",
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
        removal::Result::TimedOut => {
            if matches!(stmt, Stmt::Local(_)) {
                LEVEL_TIMEDOUT_LOCAL
            } else {
                LEVEL_TIMEDOUT_NONLOCAL
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
        removal::Result::Failed => Blue.normal(),
        removal::Result::TimedOut => Cyan.normal(),
        removal::Result::Passed => Green.normal(),
    }
}

#[derive(Debug, FromDao, ToDao, ToColumnNames, ToTableName)]
pub struct Removal {
    pub pkg: String,
    pub test: String,
    pub span: String,
    pub stmt: String,
    pub result: String,
    pub url: String,
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
    #[clap(long, about = "Skip `break` and `continue` statements")]
    skip_controls: bool,
    #[clap(
        long,
        about = "Skip local (`let`) bindings (alias: --skip-lets)",
        alias = "skip-lets"
    )]
    skip_locals: bool,
    #[clap(long, about = "Output to a sqlite database instead of to the console")]
    sqlite: bool,
    #[clap(
        long,
        about = "Maximum number of seconds to run any test; 60 is the default, 0 means no timeout"
    )]
    timeout: Option<u64>,
}

pub fn cargo_necessist<T: AsRef<OsStr>>(args: &[T]) -> Result<()> {
    let SubCommand::Necessist(opts) = Opts::parse_from(args).subcmd;

    let skip_calls_re = opts
        .skip_calls
        .as_ref()
        .map(|re| Regex::new(&re))
        .transpose()?;

    let sqlite = if opts.sqlite {
        let mut pool = Pool::new();
        let mut em = pool.em("sqlite://necessist.db")?;
        let sql = include_str!("create_table_removal.sql");
        if let Err(err) = em.execute_sql_with_return::<Removal>(sql, &[]) {
            warn(None, &err.to_string());
        }
        let em = Some(RefCell::new(em));

        let remote = Repository::open(".").ok().and_then(|repository| {
            repository
                .find_remote("origin")
                .ok()
                .and_then(|origin| origin.url().map(str::to_owned))
                .and_then(|url| {
                    repository
                        .refname_to_id("HEAD")
                        .ok()
                        .map(|head| (url, head))
                })
        });

        (em, remote)
    } else {
        (None, None)
    };

    let default_config = Config::default()?;

    let path = Path::new("./Cargo.toml").canonicalize()?;

    let ws = Workspace::new(&path, &default_config)?;

    if warnings_are_denied(&ws)? {
        warn(None, "rust appears to be configured to deny warnings; allowing warnings is strongly recommended");
    }

    for pkg in ws.members() {
        if let Err(err) = test(&opts, &pkg, None, None, false) {
            warn(
                Some(pkg),
                &format!("skipping because tests did not build: {}", err),
            );
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

            let instrumentation: Vec<_> = {
                let re = Regex::new(r"^[[:space:]]*#\[necessist::necessist\]$").unwrap();
                content
                    .lines()
                    .enumerate()
                    .filter_map(|(i, line)| if re.is_match(line) { Some(1 + i) } else { None })
                    .collect()
            };
            assert!(instrumentation.is_sorted());

            match syn::parse_file(&content) {
                Ok(file) => {
                    ItemFnVisitor {
                        context: &Context {
                            opts: &opts,
                            skip_calls_re: skip_calls_re.as_ref(),
                            em: sqlite.0.as_ref(),
                            remote: sqlite.1.as_ref(),
                            ws: &ws,
                            pkg: &pkg,
                        },
                        path: &path,
                        instrumentation: &instrumentation,
                    }
                    .visit_file(&file);
                }
                Err(err) => {
                    warn(
                        Some(pkg),
                        &format!("could not parse {}: {}", path.to_string_lossy(), err),
                    );
                }
            }
        }
    }

    Ok(())
}

struct Context<'a> {
    opts: &'a Necessist,
    skip_calls_re: Option<&'a Regex>,
    em: Option<&'a RefCell<EntityManager>>,
    remote: Option<&'a (String, Oid)>,
    ws: &'a Workspace<'a>,
    pkg: &'a Package,
}

struct ItemFnVisitor<'a> {
    context: &'a Context<'a>,
    path: &'a Path,
    instrumentation: &'a [usize],
}

impl<'ast, 'a> Visit<'ast> for ItemFnVisitor<'a> {
    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        if is_instrumented(item) {
            // smoelius: The tests must be run individually because they could timeout.
            match test(
                self.context.opts,
                self.context.pkg,
                Some(&item.sig.ident.to_string()),
                None,
                true,
            ) {
                Ok(_) => {}
                Err(err) => {
                    warn(
                        Some(self.context.pkg),
                        &format!(
                            "skipping test `{}` because it {}",
                            item.sig.ident,
                            if is_timeout(err) {
                                "timed-out"
                            } else {
                                "failed"
                            }
                        ),
                    );
                    return;
                }
            }

            StmtVisitor {
                context: self.context,
                path: self.path,
                instrumentation: self.instrumentation,
                ident: item.sig.ident.to_string(),
                leaves_visited: Cell::new(0),
            }
            .visit_item_fn(item);
        }
    }
}

struct StmtVisitor<'a> {
    context: &'a Context<'a>,
    path: &'a Path,
    instrumentation: &'a [usize],
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
                .expect("Could not determine workspace path")
                .to_path_buf()
                .join(&self.path),
            stmt.span(),
        );

        if is_whitelisted_macro(stmt)
            || (self.context.opts.skip_locals && matches!(stmt, Stmt::Local(_)))
            || (self.context.opts.skip_controls && is_control(stmt))
            || self
                .context
                .skip_calls_re
                .map_or(false, |re| is_skipped_call(re, stmt))
        {
            self.emit(&span, stmt, removal::Result::Skipped);
            return;
        }

        match test(
            &self.context.opts,
            &self.context.pkg,
            Some(&self.ident),
            Some(&span),
            false,
        ) {
            Ok(removed) => {
                if !removed {
                    self.emit(&span, stmt, removal::Result::Inconclusive);
                    return;
                }

                match test(
                    &self.context.opts,
                    &self.context.pkg,
                    Some(&self.ident),
                    Some(&span),
                    true,
                ) {
                    Ok(removed) => {
                        // smoelius: A "removed" message should be generated when the target is built,
                        // but not when it is run.
                        assert!(!removed);

                        self.emit(&span, stmt, removal::Result::Passed)
                    }
                    Err(err) => {
                        self.emit(
                            &span,
                            stmt,
                            if is_timeout(err) {
                                removal::Result::TimedOut
                            } else {
                                removal::Result::Failed
                            },
                        );
                    }
                }
            }
            Err(err) => {
                assert!(!is_timeout(err));
                self.emit(&span, stmt, removal::Result::Nonbuildable);
            }
        }
    }
}

impl<'a> StmtVisitor<'a> {
    fn emit(&self, span: &necessist::Span, stmt: &Stmt, result: removal::Result) {
        let stripped_span =
            strip_span(self.context.ws.root(), &span).expect("Unexpected span source file");
        if let Some(em) = self.context.em {
            let removal = Removal {
                pkg: self.context.pkg.name().to_string(),
                test: self.ident.to_string(),
                span: stripped_span.to_string(),
                stmt: stmt.to_token_stream().to_string(),
                result: result.to_string(),
                url: self
                    .context
                    .remote
                    .map(|(base_url, oid)| {
                        url_from_stripped_span(
                            base_url,
                            oid,
                            self.instrumentation
                                .binary_search(&span.start.line)
                                .unwrap_or_else(|i| i),
                            &stripped_span,
                        )
                    })
                    .unwrap_or_default(),
            };
            em.borrow_mut()
                .single_insert(&removal)
                .expect("Could not insert into table `removal`");
        } else if self.context.opts.quiet < level(stmt, &result) {
            println!(
                "{}: `{}` {}",
                stripped_span,
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

// smoelius: The `adjustment` parameter accounts for the `#[necessist::necessist]` inserted by
// `necessist_instrument.sh`. Without this parameter, the generated line numbers would be too high.
// Changes to `necessist_instrument.sh` might warrant changes to this function as well.
fn url_from_stripped_span(
    base_url: &str,
    oid: &Oid,
    adjustment: usize,
    span: &necessist::Span,
) -> String {
    let base_url = base_url.strip_suffix(".git").unwrap_or(base_url).to_owned();

    let ssh_re = Regex::new(r"^[^@]*@([^:]*):(.*)$").unwrap();
    let base_url = if let Some(captures) = ssh_re.captures(&base_url) {
        assert!(captures.len() == 3);
        format!("https://{}/{}", &captures[1], &captures[2])
    } else {
        base_url
    };

    base_url
        + "/blob/"
        + &oid.to_string()
        + "/"
        + &span.source_file.to_string_lossy()
        + "#L"
        + &(span.start.line - adjustment).to_string()
        + "-L"
        + &(span.end.line - adjustment).to_string()
}

fn test(
    opts: &Necessist,
    pkg: &Package,
    ident: Option<&str>,
    span: Option<&necessist::Span>,
    run: bool,
) -> subprocess::Result<bool> {
    let env = span.map_or(vec![], |span| {
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
        if let Some(ident) = ident {
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

    let mut popen = exec.popen()?;
    // smoelius: Ensure the process is killed, e.g., if a timeout occurs. Otherwise, we will wait on
    // it.
    let result = || -> std::result::Result<_, PopenError> {
        let mut communicator = popen.communicate_start(None);
        if run {
            if let Some(time) = timeout(opts) {
                communicator = communicator.limit_time(time);
            }
        }
        let (stdout, _) = communicator.read_string()?;

        let mut removed = false;
        let removed_msg = span.map(|span| removed_message(span));
        for line in stdout.expect("stdout was not piped").lines() {
            debug! {"{}", line};
            removed |= removed_msg.as_ref().map_or(false, |msg| line == msg);
        }

        let status = if run {
            popen.wait_timeout(Duration::default())?
        } else {
            Some(popen.wait()?)
        };
        status.map_or(
            Err(PopenError::IoError(io::Error::new(
                io::ErrorKind::TimedOut,
                "timeout",
            ))),
            |status| {
                if status.success() {
                    Ok(removed)
                } else {
                    Err(PopenError::LogicError("failed"))
                }
            },
        )
    }();

    popen.kill().unwrap_or_default();

    result
}

fn timeout(opts: &Necessist) -> Option<Duration> {
    match opts.timeout {
        None => Some(DEFAULT_TIMEOUT),
        Some(0) => None,
        Some(secs) => Some(Duration::from_secs(secs)),
    }
}

fn warnings_are_denied(ws: &Workspace) -> Result<bool> {
    let config = ws.config().build_config()?;
    Ok(config.rustflags.as_ref().map_or(false, |list| {
        list.as_slice()
            .windows(2)
            .any(|pair| pair == ["-D", "warnings"])
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
    .and_then(|expr| match expr {
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

fn is_control(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Expr(expr) => Some(expr),
        Stmt::Semi(expr, ..) => Some(expr),
        _ => None,
    }
    .map_or(false, |expr| match expr {
        Expr::Break(_) => true,
        Expr::Continue(_) => true,
        _ => false,
    })
}

fn is_timeout(err: PopenError) -> bool {
    match err {
        PopenError::IoError(err) => err.kind() == ErrorKind::TimedOut,
        _ => false,
    }
}

fn warn(pkg: Option<&Package>, msg: &str) {
    println!(
        "{}: {}{}",
        Yellow.bold().paint("warning"),
        &pkg.map_or("".to_owned(), |pkg| format!("{}: ", pkg.name())),
        msg
    );
}

#[cfg(test)]
mod test {
    use assert_cmd::prelude::*;
    use lazy_static::lazy_static;
    use predicates::prelude::*;
    use std::{
        path::{Path, PathBuf},
        process::Command,
        sync::Mutex,
    };

    const TEST_DIR: &str = "../examples";
    const TOOLCHAIN: &str = "nightly-2020-10-14";
    const TIMEOUT: &str = "10";

    lazy_static! {
        static ref MUTEX: Mutex<()> = Mutex::new(());
    }

    #[test]
    fn toolchain_sanity() {
        let _mutex_guard = MUTEX.lock().unwrap();

        Command::new("rustup")
            .current_dir(TEST_DIR)
            .env("RUSTUP_TOOLCHAIN", TOOLCHAIN)
            .args(&["show", "active-toolchain"])
            .assert()
            .success()
            .stdout(predicate::str::starts_with(TOOLCHAIN));
    }

    #[test]
    fn console() {
        let _mutex_guard = MUTEX.lock().unwrap();

        Command::new("cargo")
            .current_dir(TEST_DIR)
            .env("RUSTUP_TOOLCHAIN", TOOLCHAIN)
            .args(&["necessist", "--timeout", TIMEOUT])
            .assert()
            .success()
            .stdout(predicate::path::eq_file("console.stdout"));
    }

    struct RemoveFile(PathBuf);

    impl Drop for RemoveFile {
        fn drop(&mut self) {
            std::fs::remove_file(self.0.as_path())
                .map_err(|err| eprintln!("{}", err))
                .unwrap_or_default();
        }
    }

    #[test]
    fn sqlite() {
        let _mutex_guard = MUTEX.lock().unwrap();

        let path = Path::new(TEST_DIR).join("necessist.db");

        assert!(!path.exists());

        let _remove_file = RemoveFile(path);

        Command::new("cargo")
            .current_dir(TEST_DIR)
            .env("RUSTUP_TOOLCHAIN", TOOLCHAIN)
            .args(&["necessist", "--timeout", TIMEOUT, "--sqlite"])
            .assert()
            .success();

        Command::new("sqlite3")
            .current_dir(TEST_DIR)
            .args(&["necessist.db", "select * from removal"])
            .assert()
            .success()
            .stdout(predicate::path::eq_file("sqlite.stdout"));
    }
}
