use crate::{
    config,
    framework::{self, Applicable, Postprocess, SourceFileTestSpanMap, SpanKind, ToImplementation},
    note, source_warn, sqlite, util, warn, Backup, Outcome, Rewriter, SourceFile, Span,
    ToConsoleString, WarnFlags, Warning,
};
use ansi_term::Style;
use anyhow::{anyhow, bail, ensure, Context as _, Result};
use heck::ToKebabCase;
use indicatif::ProgressBar;
use itertools::{peek_nth, PeekNth};
use log::debug;
use once_cell::sync::OnceCell;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    env::{current_dir, var},
    fmt::Display,
    io::{IsTerminal, Write},
    iter::Peekable,
    path::{Path, PathBuf},
    process::{Command, ExitStatus as StdExitStatus, Stdio},
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use strum::IntoEnumIterator;
use subprocess::{Exec, ExitStatus};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

static CTRLC: AtomicBool = AtomicBool::new(false);

#[derive(Clone)]
pub(crate) struct Removal {
    pub span: Span,
    pub text: String,
    pub outcome: Outcome,
}

#[derive(Debug)]
enum MismatchKind {
    Missing,
    Unexpected,
}

impl Display for MismatchKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", format!("{self:?}").to_kebab_case())
    }
}

struct Mismatch {
    kind: MismatchKind,
    removal: Removal,
}

struct Context<'a> {
    opts: Necessist,
    root: Rc<PathBuf>,
    println: &'a dyn Fn(&dyn AsRef<str>),
    framework: Box<dyn framework::Interface>,
    progress: Option<&'a ProgressBar>,
}

impl<'a> Context<'a> {
    fn light(&self) -> LightContext {
        LightContext {
            opts: &self.opts,
            root: &self.root,
            println: self.println,
        }
    }
}

pub struct LightContext<'a> {
    pub opts: &'a Necessist,
    pub root: &'a Rc<PathBuf>,
    pub println: &'a dyn Fn(&dyn AsRef<str>),
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Default)]
pub struct Necessist {
    pub allow: Vec<Warning>,
    pub default_config: bool,
    pub deny: Vec<Warning>,
    pub dump: bool,
    pub dump_candidates: bool,
    pub no_dry_run: bool,
    pub no_sqlite: bool,
    pub quiet: bool,
    pub reset: bool,
    pub resume: bool,
    pub root: Option<PathBuf>,
    pub timeout: Option<u64>,
    pub verbose: bool,
    pub source_files: Vec<PathBuf>,
    pub args: Vec<String>,
}

/// Necessist's main entrypoint.
// smoelius: The reason `framework` is not included as a field in `Necessist` is to avoid having
// to parameterize every function that takes a `Necessist` as an argument.
pub fn necessist<Identifier: Applicable + Display + IntoEnumIterator + ToImplementation>(
    opts: &Necessist,
    framework: framework::Auto<Identifier>,
) -> Result<()> {
    let opts = opts.clone();

    process_options(&opts)?;

    let root = opts
        .root
        .as_ref()
        .map_or_else(current_dir, dunce::canonicalize)
        .map(Rc::new)?;

    #[cfg(feature = "lock_root")]
    let _file: std::fs::File = lock_root(&root)?;

    let mut context = LightContext {
        opts: &opts,
        root: &root,
        println: &|_| {},
    };

    let println = |msg: &dyn AsRef<str>| {
        println!("{}", msg.as_ref());
    };

    if !opts.quiet {
        context.println = &println;
    }

    let Some((framework, n_spans, source_file_test_span_map)) = prepare(&context, framework)?
    else {
        return Ok(());
    };

    let mut context = Context {
        opts,
        root,
        println: &|_| {},
        framework,
        progress: None,
    };

    if !context.opts.quiet {
        context.println = &println;
    }

    let progress =
        if var("RUST_LOG").is_err() && !context.opts.quiet && std::io::stdout().is_terminal() {
            Some(ProgressBar::new(n_spans as u64))
        } else {
            None
        };

    let progress_println = |msg: &dyn AsRef<str>| {
        #[allow(clippy::unwrap_used)]
        progress.as_ref().unwrap().println(msg);
    };

    if progress.is_some() {
        context.println = &progress_println;
        context.progress = progress.as_ref();
    }

    run(context, source_file_test_span_map)
}

#[allow(clippy::type_complexity)]
fn prepare<Identifier: Applicable + Display + IntoEnumIterator + ToImplementation>(
    context: &LightContext,
    framework: framework::Auto<Identifier>,
) -> Result<Option<(Box<dyn framework::Interface>, usize, SourceFileTestSpanMap)>> {
    if context.opts.default_config {
        default_config(context, context.root)?;
        return Ok(None);
    }

    let config = config::Toml::read(context, context.root)?;

    if context.opts.dump {
        let past_removals = past_removals_init_lazy(context)?;
        dump(context, &past_removals);
        return Ok(None);
    }

    let mut framework = find_framework(context, framework)?;

    let paths = canonicalize_source_files(context)?;

    let source_file_test_span_map = framework.parse(
        context,
        &config,
        &paths.iter().map(AsRef::as_ref).collect::<Vec<_>>(),
    )?;

    let n_spans = source_file_test_span_map
        .values()
        .map(|test_span_maps| {
            test_span_maps
                .statement
                .values()
                .map(BTreeSet::len)
                .sum::<usize>()
                + test_span_maps
                    .method_call
                    .values()
                    .map(BTreeSet::len)
                    .sum::<usize>()
        })
        .sum();

    if context.opts.dump_candidates {
        dump_candidates(context, &source_file_test_span_map)?;
        return Ok(None);
    }

    (context.println)({
        let n_tests = source_file_test_span_map
            .values()
            .map(|test_span_maps| {
                assert_eq!(
                    test_span_maps.statement.keys().len(),
                    test_span_maps.method_call.keys().len()
                );
                test_span_maps.statement.keys().len()
            })
            .sum::<usize>();
        let n_source_files = source_file_test_span_map.keys().len();
        &format!(
            "{} candidates in {} test{} in {} source file{}",
            n_spans,
            n_tests,
            if n_tests == 1 { "" } else { "s" },
            n_source_files,
            if n_source_files == 1 { "" } else { "s" }
        )
    });

    Ok(Some((framework, n_spans, source_file_test_span_map)))
}

fn run(mut context: Context, source_file_test_span_map: SourceFileTestSpanMap) -> Result<()> {
    ctrlc::set_handler(|| CTRLC.store(true, Ordering::SeqCst))?;

    let past_removals = past_removals_init_lazy(&context.light())?;

    let mut past_removal_iter = past_removals.into_iter().peekable();

    for (source_file, test_span_maps) in source_file_test_span_map {
        let mut test_span_iter = peek_nth(test_span_maps.iter());

        let (mismatch, n) = skip_past_removals(&mut test_span_iter, &mut past_removal_iter);

        update_progress(&context, mismatch, n)?;

        if test_span_iter.peek().is_none() {
            continue;
        }

        if !context.opts.no_dry_run {
            (context.println)(&format!(
                "{}: dry running",
                util::strip_current_dir(&source_file).to_string_lossy()
            ));

            let result = context.framework.dry_run(&context.light(), &source_file);

            if let Err(error) = &result {
                source_warn(
                    &context.light(),
                    Warning::DryRunFailed,
                    &source_file,
                    &format!("dry run failed: {error:?}"),
                    WarnFlags::empty(),
                )?;
            }

            if CTRLC.load(Ordering::SeqCst) {
                bail!("Ctrl-C detected");
            }

            if result.is_err() {
                let n = skip_present_spans(&context, test_span_iter)?;
                update_progress(&context, None, n)?;
                continue;
            }
        }

        (context.println)(&format!(
            "{}: mutilating",
            util::strip_current_dir(&source_file).to_string_lossy()
        ));

        let mut instrumentation_backup =
            instrument_statements(&context, &source_file, &mut test_span_iter)?;

        loop {
            let (mismatch, n) = skip_past_removals(&mut test_span_iter, &mut past_removal_iter);

            update_progress(&context, mismatch, n)?;

            let Some((test_name, span, span_kind)) = test_span_iter.next() else {
                break;
            };

            if span_kind != SpanKind::Statement {
                drop(instrumentation_backup.take());
            }

            let text = span.source_text()?;

            let explicit_removal =
                instrumentation_backup.is_none() || span_kind != SpanKind::Statement;

            let _explicit_backup = if explicit_removal {
                let (_, explicit_backup) = span.remove()?;
                Some(explicit_backup)
            } else {
                None
            };

            let outcome = if let Some((exec, postprocess)) =
                context.framework.exec(&context.light(), test_name, span)?
            {
                // smoelius: Even if the removal is explicit (i.e., not with instrumentation), it
                // doesn't hurt to set `NECESSIST_REMOVAL`.
                let exec = exec.env("NECESSIST_REMOVAL", span.id());

                perform_exec(&context, exec, postprocess)?
            } else {
                assert!(
                    explicit_removal,
                    "Instrumentation failed to build after it was verified to"
                );

                Some(Outcome::Nonbuildable)
            };

            if CTRLC.load(Ordering::SeqCst) {
                bail!("Ctrl-C detected");
            }

            if let Some(outcome) = outcome {
                emit(&mut context, span, &text, outcome)?;
            }

            update_progress(&context, None, 1)?;
        }
    }

    context.progress.map(ProgressBar::finish);

    Ok(())
}

macro_rules! incompatible {
    ($opts:ident, $x:ident, $y:ident) => {
        ensure!(
            !($opts.$x && $opts.$y),
            "--{} and --{} are incompatible",
            stringify!($x).to_kebab_case(),
            stringify!($y).to_kebab_case()
        );
    };
}

fn process_options(opts: &Necessist) -> Result<()> {
    // smoelius: This list of incompatibilities is not exhaustive.
    incompatible!(opts, dump, quiet);
    incompatible!(opts, dump, reset);
    incompatible!(opts, dump, resume);
    incompatible!(opts, dump, no_sqlite);
    incompatible!(opts, quiet, verbose);
    incompatible!(opts, reset, no_sqlite);
    incompatible!(opts, resume, no_sqlite);

    Ok(())
}

#[cfg(feature = "lock_root")]
fn lock_root(root: &Path) -> Result<std::fs::File> {
    if enabled("TRYCMD") {
        crate::flock::lock_path(root)
    } else {
        crate::flock::try_lock_path(root)
    }
    .with_context(|| format!("Failed to lock {root:?}"))
}

#[cfg(feature = "lock_root")]
fn enabled(key: &str) -> bool {
    var(key).map_or(false, |value| value != "0")
}

fn default_config(_context: &LightContext, root: &Path) -> Result<()> {
    let path_buf = root.join("necessist.toml");

    if path_buf.try_exists()? {
        bail!("A configuration file already exists at {:?}", path_buf);
    }

    let toml = toml::to_string(&config::Toml::default())?;

    std::fs::write(path_buf, toml).map_err(Into::into)
}

fn dump(context: &LightContext, removals: &[Removal]) {
    let mut other_than_passed = false;
    for removal in removals {
        emit_to_console(context, removal);
        other_than_passed |= removal.outcome != Outcome::Passed;
    }

    if !context.opts.verbose && other_than_passed {
        note(context, "More output would be produced with --verbose");
    }
}

fn find_framework<Identifier: Applicable + Display + IntoEnumIterator + ToImplementation>(
    context: &LightContext,
    identifier: framework::Auto<Identifier>,
) -> Result<Box<dyn framework::Interface>> {
    let implementation = identifier.to_implementation(context)?;

    drop(identifier);

    implementation.ok_or_else(|| anyhow!("Found no applicable frameworks"))
}

fn canonicalize_source_files(context: &LightContext) -> Result<Vec<PathBuf>> {
    context
        .opts
        .source_files
        .iter()
        .map(|path| {
            let path_buf = dunce::canonicalize(path)
                .with_context(|| format!("Failed to canonicalize {path:?}"))?;
            ensure!(
                path_buf.starts_with(context.root.as_path()),
                "{:?} is not in {:?}",
                path_buf,
                context.root
            );
            Ok(path_buf)
        })
        .collect::<Result<Vec<_>>>()
}

#[must_use]
fn skip_past_removals<'a, I, J>(
    test_span_iter: &mut PeekNth<I>,
    removal_iter: &mut Peekable<J>,
) -> (Option<Mismatch>, usize)
where
    I: Iterator<Item = (&'a str, &'a Span, SpanKind)>,
    J: Iterator<Item = Removal>,
{
    let mut mismatch = None;
    let mut n = 0;
    while let Some(&(_, span, _)) = test_span_iter.peek() {
        let Some(removal) = removal_iter.peek() else {
            break;
        };
        match span.cmp(&removal.span) {
            std::cmp::Ordering::Less => {
                mismatch = Some(Mismatch {
                    kind: MismatchKind::Unexpected,
                    removal: removal.clone(),
                });
                break;
            }
            std::cmp::Ordering::Equal => {
                let _: Option<(&str, &Span, _)> = test_span_iter.next();
                let _removal: Option<Removal> = removal_iter.next();
                n += 1;
            }
            std::cmp::Ordering::Greater => {
                if mismatch.is_none() {
                    mismatch = Some(Mismatch {
                        kind: MismatchKind::Missing,
                        removal: removal.clone(),
                    });
                }
                let _removal: Option<Removal> = removal_iter.next();
            }
        }
    }

    (mismatch, n)
}

fn skip_present_spans<'a>(
    context: &Context,
    test_span_iter: impl Iterator<Item = (&'a str, &'a Span, SpanKind)>,
) -> Result<usize> {
    let mut n = 0;

    let sqlite = sqlite_init_lazy(&context.light())?;

    for (_, span, _) in test_span_iter {
        if let Some(sqlite) = sqlite.borrow_mut().as_mut() {
            let text = span.source_text()?;
            let removal = Removal {
                span: span.clone(),
                text,
                outcome: Outcome::Skipped,
            };
            sqlite::insert(sqlite, &removal)?;
        }
        n += 1;
    }

    Ok(n)
}

fn update_progress(context: &Context, mismatch: Option<Mismatch>, n: usize) -> Result<()> {
    if let Some(Mismatch {
        kind,
        removal: Removal { span, text, .. },
    }) = mismatch
    {
        warn(
            &context.light(),
            Warning::FilesChanged,
            &format!(
                "\
Configuration or source files have changed since necessist.db was created; the following entry is \
                 {kind}:
    {}: `{}`",
                span.to_console_string(),
                text.replace('\r', ""),
            ),
            WarnFlags::ONCE,
        )?;
    }

    if let Some(bar) = context.progress {
        bar.inc(n as u64);
    }

    Ok(())
}

fn dump_candidates(
    context: &LightContext,
    source_file_test_span_map: &SourceFileTestSpanMap,
) -> Result<()> {
    for span in source_file_test_span_map
        .values()
        .flat_map(|test_span_maps| {
            test_span_maps
                .statement
                .values()
                .chain(test_span_maps.method_call.values())
        })
        .flatten()
    {
        let text = span.source_text()?;

        (context.println)(&format!(
            "{}: `{}`",
            span.to_console_string(),
            text.replace('\r', "")
        ));
    }

    Ok(())
}

fn instrument_statements<'a, I>(
    context: &Context,
    source_file: &SourceFile,
    test_span_iter: &mut PeekNth<I>,
) -> Result<Option<Backup>>
where
    I: Iterator<Item = (&'a str, &'a Span, SpanKind)>,
{
    let backup = Backup::new(source_file)?;

    let mut rewriter = Rewriter::new(source_file.contents(), source_file.offset_calculator());

    let n_instrumentable_statements = count_instrumentable_statements(test_span_iter);

    context.framework.instrument_source_file(
        &context.light(),
        &mut rewriter,
        source_file,
        n_instrumentable_statements,
    )?;

    let mut i_span = 0;
    let mut insertion_map = BTreeMap::<_, Vec<_>>::new();
    // smoelius: Do not advance the underlying iterator while instrumenting. This way, if a
    // statement cannot be removed with instrumentation, it will be removed explicitly.
    while let Some((_, span, SpanKind::Statement)) = test_span_iter.peek_nth(i_span) {
        let (prefix, suffix) = context.framework.statement_prefix_and_suffix(span)?;
        let insertions = insertion_map.entry(span.start()).or_default();
        insertions.push(prefix);
        let insertions = insertion_map.entry(span.end()).or_default();
        insertions.push(suffix);
        i_span += 1;
    }

    assert_eq!(n_instrumentable_statements, i_span);

    for (line_column, insertions) in insertion_map {
        for insertion in insertions {
            rewriter.insert(source_file, line_column, &insertion);
        }
    }

    let mut file = std::fs::OpenOptions::new()
        .truncate(true)
        .write(true)
        .open(source_file)?;
    file.write_all(rewriter.contents().as_bytes())?;
    drop(file);

    let result = context
        .framework
        .build_source_file(&context.light(), source_file);
    if let Err(error) = result {
        warn(
            &context.light(),
            Warning::InstrumentationNonbuildable,
            &format!(
                "Instrumentation caused `{}` to be nonbuildable: {error:?}",
                source_file.to_console_string(),
            ),
            WarnFlags::empty(),
        )?;
        return Ok(None);
    }

    Ok(Some(backup))
}

fn count_instrumentable_statements<'a, I>(test_span_iter: &mut PeekNth<I>) -> usize
where
    I: Iterator<Item = (&'a str, &'a Span, SpanKind)>,
{
    let mut n_instrumentable_statements = 0;
    while matches!(
        test_span_iter.peek_nth(n_instrumentable_statements),
        Some((_, _, SpanKind::Statement))
    ) {
        n_instrumentable_statements += 1;
    }
    n_instrumentable_statements
}

fn perform_exec(
    context: &Context,
    exec: Exec,
    postprocess: Option<Box<Postprocess>>,
) -> Result<Option<Outcome>> {
    debug!("{:?}", exec);

    #[cfg(all(feature = "limit_threads", unix))]
    let nprocs_prev = rlimit::set_soft_rlimit(
        rlimit::Resource::NPROC,
        *rlimit::NPROC_INIT + rlimit::NPROC_ALLOWANCE,
    )?;

    let mut popen = exec.popen()?;
    let status = if let Some(dur) = timeout(&context.opts) {
        popen.wait_timeout(dur)?
    } else {
        popen.wait().map(Option::Some)?
    };

    #[cfg(all(feature = "limit_threads", unix))]
    rlimit::set_soft_rlimit(rlimit::Resource::NPROC, nprocs_prev)?;

    if status.is_some() {
        if let Some(postprocess) = postprocess {
            if !postprocess(&context.light(), popen)? {
                return Ok(None);
            }
        }
    } else {
        let pid = popen.pid().ok_or_else(|| anyhow!("Failed to get pid"))?;
        transitive_kill(pid)?;
        let _: ExitStatus = popen.wait()?;
    }

    let Some(status) = status else {
        return Ok(Some(Outcome::TimedOut));
    };

    Ok(Some(if status.success() {
        Outcome::Passed
    } else {
        Outcome::Failed
    }))
}

#[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
fn emit(context: &mut Context, span: &Span, text: &str, outcome: Outcome) -> Result<()> {
    let removal = Removal {
        span: span.clone(),
        text: text.to_owned(),
        outcome,
    };

    let sqlite = sqlite_init_lazy(&context.light())?;

    if let Some(sqlite) = sqlite.borrow_mut().as_mut() {
        sqlite::insert(sqlite, &removal)?;
    }

    emit_to_console(&context.light(), &removal);

    Ok(())
}

fn emit_to_console(context: &LightContext, removal: &Removal) {
    let Removal {
        span,
        text,
        outcome,
    } = removal;

    if !context.opts.quiet && (context.opts.verbose || *outcome == Outcome::Passed) {
        let msg = format!(
            "{}: `{}` {}",
            span.to_console_string(),
            text.replace('\r', ""),
            if std::io::stdout().is_terminal() {
                outcome.style().bold()
            } else {
                Style::default()
            }
            .paint(outcome.to_string())
        );
        (context.println)(&msg);
    }
}

fn sqlite_init_lazy(context: &LightContext) -> Result<Rc<RefCell<Option<sqlite::Sqlite>>>> {
    let (sqlite, _) = sqlite_and_past_removals_init_lazy(context)?;
    Ok(sqlite)
}

fn past_removals_init_lazy(context: &LightContext) -> Result<Vec<Removal>> {
    let (_, past_removals) = sqlite_and_past_removals_init_lazy(context)?;
    Ok(past_removals.take())
}

thread_local! {
    #[allow(clippy::type_complexity)]
    static SQLITE_AND_PAST_REMOVALS: OnceCell<(
        Rc<RefCell<Option<sqlite::Sqlite>>>,
        Rc<RefCell<Vec<Removal>>>,
    )> = const { OnceCell::new() };
}

#[allow(clippy::type_complexity)]
fn sqlite_and_past_removals_init_lazy(
    context: &LightContext,
) -> Result<(
    Rc<RefCell<Option<sqlite::Sqlite>>>,
    Rc<RefCell<Vec<Removal>>>,
)> {
    SQLITE_AND_PAST_REMOVALS.with(|sqlite_and_past_removals| {
        sqlite_and_past_removals
            .get_or_try_init(|| {
                if context.opts.no_sqlite {
                    Ok((
                        Rc::new(RefCell::new(None)),
                        Rc::new(RefCell::new(Vec::new())),
                    ))
                } else {
                    let (sqlite, mut past_removals) = sqlite::init(
                        context,
                        context.root,
                        context.opts.dump,
                        context.opts.reset,
                        context.opts.resume,
                    )?;
                    past_removals.sort_by(|left, right| left.span.cmp(&right.span));
                    Ok((
                        Rc::new(RefCell::new(Some(sqlite))),
                        Rc::new(RefCell::new(past_removals)),
                    ))
                }
            })
            .cloned()
    })
}

#[allow(clippy::module_name_repetitions)]
#[cfg(all(feature = "limit_threads", unix))]
mod rlimit {
    pub use ::rlimit::Resource;
    use ::rlimit::{getrlimit, setrlimit};
    use anyhow::Result;
    use once_cell::sync::Lazy;
    use std::process::Command;

    #[allow(clippy::unwrap_used)]
    pub static NPROC_INIT: Lazy<u64> = Lazy::new(|| {
        let output = Command::new("ps").arg("-eL").output().unwrap();
        let stdout = std::str::from_utf8(&output.stdout).unwrap();
        stdout.lines().count().try_into().unwrap()
    });

    // smoelius: Limit the number of threads that a test can allocate to approximately 1024 (an
    // arbitrary choice).
    //
    // The limit is not strict for the following reason. `NPROC_INIT` counts the number of threads
    // *started by any user*. But `setrlimit` (used to enforce the limit) applies to just the
    // current user. So by setting the limit to `NPROC_INIT + NPROC_ALLOWANCE`, the number of
    // threads the test can allocate is actually 1024 plus the number of threads started by other
    // users.
    pub const NPROC_ALLOWANCE: u64 = 1024;

    pub fn set_soft_rlimit(resource: Resource, limit: u64) -> Result<u64> {
        let (soft, hard) = getrlimit(resource)?;
        setrlimit(Resource::NPROC, std::cmp::min(hard, limit), hard)?;
        Ok(soft)
    }
}

fn timeout(opts: &Necessist) -> Option<Duration> {
    match opts.timeout {
        None => Some(DEFAULT_TIMEOUT),
        Some(0) => None,
        Some(secs) => Some(Duration::from_secs(secs)),
    }
}

#[cfg_attr(dylint_lib = "supplementary", allow(commented_code))]
fn transitive_kill(pid: u32) -> Result<()> {
    let mut pids = vec![(pid, false)];

    while let Some((pid, visited)) = pids.pop() {
        if visited {
            let _status: StdExitStatus = kill()
                .arg(pid.to_string())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()?;
            // smoelius: The process may have already exited.
            // ensure!(status.success());
        } else {
            pids.push((pid, true));

            for line in child_processes(pid)? {
                let pid = line
                    .parse::<u32>()
                    .with_context(|| format!("failed to parse `{line}`"))?;
                pids.push((pid, false));
            }
        }
    }

    Ok(())
}

#[cfg(not(windows))]
fn kill() -> Command {
    Command::new("kill")
}

#[cfg(windows)]
fn kill() -> Command {
    let mut command = Command::new("taskkill");
    command.args(["/f", "/pid"]);
    command
}

#[cfg(not(windows))]
fn child_processes(pid: u32) -> Result<Vec<String>> {
    let output = Command::new("pgrep")
        .args(["-P", &pid.to_string()])
        .output()?;
    let stdout = String::from_utf8(output.stdout)?;
    Ok(stdout.lines().map(ToOwned::to_owned).collect())
}

#[cfg(windows)]
fn child_processes(pid: u32) -> Result<Vec<String>> {
    let output = Command::new("wmic")
        .args([
            "process",
            "where",
            &format!("ParentProcessId={pid}"),
            "get",
            "ProcessId",
        ])
        .output()?;
    let stdout = String::from_utf8(output.stdout)?;
    Ok(stdout
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .skip(1)
        .map(ToOwned::to_owned)
        .collect())
}
