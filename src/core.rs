use crate::{
    frameworks, sqlite, util, warn, warn_once, Backup, Outcome, Rewriter, SourceFile, Span, WarnKey,
};
use ansi_term::Style;
use anyhow::{anyhow, bail, ensure, Context as _, Result};
use heck::ToKebabCase;
use indicatif::ProgressBar;
use log::debug;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    env::{current_dir, var},
    fs::{read_to_string, write, OpenOptions},
    io::Write,
    iter::Peekable,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

static CTRLC: AtomicBool = AtomicBool::new(false);

pub(crate) struct Removal {
    pub span: Span,
    pub text: String,
    pub outcome: Outcome,
}

struct Context<'a> {
    opts: Necessist,
    root: PathBuf,
    println: &'a dyn Fn(&dyn AsRef<str>),
    sqlite: Option<sqlite::Sqlite>,
    framework: Box<dyn frameworks::Interface>,
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

pub(crate) struct LightContext<'a> {
    pub opts: &'a Necessist,
    pub root: &'a Path,
    pub println: &'a dyn Fn(&dyn AsRef<str>),
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Default)]
pub struct Necessist {
    pub default_config: bool,
    pub dump: bool,
    pub framework: Framework,
    pub keep_going: bool,
    pub no_dry_run: bool,
    pub no_sqlite: bool,
    pub quiet: bool,
    pub reset: bool,
    pub resume: bool,
    pub root: Option<PathBuf>,
    pub timeout: Option<u64>,
    pub verbose: bool,
    pub test_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[non_exhaustive]
#[remain::sorted]
pub enum Framework {
    Auto,
    HardhatTs,
    Rust,
}

impl Default for Framework {
    fn default() -> Self {
        Framework::Auto
    }
}

impl std::fmt::Display for Framework {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", format!("{:?}", self).to_kebab_case())
    }
}

#[derive(Default, Deserialize, Serialize)]
pub(crate) struct Config {
    #[serde(default)]
    pub ignored_functions: Vec<String>,
    #[serde(default)]
    pub ignored_macros: Vec<String>,
}

pub fn necessist(opts: &Necessist) -> Result<()> {
    let mut opts = opts.clone();

    process_options(&mut opts)?;

    let root = opts
        .root
        .as_ref()
        .map_or_else(current_dir, |root| root.canonicalize())?;

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

    let (sqlite, framework, n_spans, test_file_span_map, past_removals) =
        if let Some(elements) = prepare(&context)? {
            elements
        } else {
            return Ok(());
        };

    let mut context = Context {
        sqlite,
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
        if var("RUST_LOG").is_err() && !context.opts.quiet && atty::is(atty::Stream::Stdout) {
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

    run(context, test_file_span_map, past_removals)
}

#[allow(clippy::type_complexity)]
fn prepare(
    context: &LightContext,
) -> Result<
    Option<(
        Option<sqlite::Sqlite>,
        Box<dyn frameworks::Interface>,
        usize,
        BTreeMap<SourceFile, Vec<Span>>,
        Vec<Removal>,
    )>,
> {
    if context.opts.default_config {
        default_config(context, context.root)?;
        return Ok(None);
    }

    let config = read_config(context, context.root)?;

    let (sqlite, past_removals) = if context.opts.no_sqlite {
        (None, Vec::new())
    } else {
        let (sqlite, mut past_removals) = sqlite::init(
            context.root,
            !context.opts.dump && !context.opts.reset && !context.opts.resume,
            context.opts.reset,
        )?;
        past_removals.sort_by(|left, right| left.span.cmp(&right.span));
        (Some(sqlite), past_removals)
    };

    if context.opts.dump {
        dump(context, &past_removals);
        return Ok(None);
    }

    let mut framework = find_framework(context)?;

    let paths = canonicalize_test_files(context)?;

    let spans = framework.parse(
        context,
        &config,
        &paths.iter().map(AsRef::as_ref).collect::<Vec<_>>(),
    )?;

    let n_spans = spans.len();

    let test_file_span_map = build_test_file_span_map(spans);

    (context.println)({
        let n_test_files = test_file_span_map.keys().len();
        &format!(
            "{} candidates in {} test file{}",
            n_spans,
            n_test_files,
            if n_test_files == 1 { "" } else { "s" }
        )
    });

    Ok(Some((
        sqlite,
        framework,
        n_spans,
        test_file_span_map,
        past_removals,
    )))
}

fn run(
    mut context: Context,
    test_file_span_map: BTreeMap<SourceFile, Vec<Span>>,
    past_removals: Vec<Removal>,
) -> Result<()> {
    ctrlc::set_handler(|| CTRLC.store(true, Ordering::SeqCst))?;

    let mut past_removal_iter = past_removals.into_iter().peekable();

    for (test_file, spans) in test_file_span_map {
        let mut span_iter = spans.iter().peekable();

        let (mismatch, n) = skip_past_removals(&mut span_iter, &mut past_removal_iter);

        update_progress(&context, mismatch, n);

        if span_iter.peek().is_none() {
            continue;
        }

        if !context.opts.no_dry_run {
            (context.println)(&format!(
                "{}: dry running",
                util::strip_current_dir(&test_file).to_string_lossy()
            ));

            if let Err(error) = context.framework.dry_run(&context.light(), &test_file) {
                if context.opts.keep_going {
                    (context.println)(&format!(
                        "{}: dry run failed: {}",
                        util::strip_current_dir(&test_file).to_string_lossy(),
                        error
                    ));
                    continue;
                }

                return Err(error).with_context(|| "dry run failed");
            }

            if CTRLC.load(Ordering::SeqCst) {
                bail!("Ctrl-C detected");
            }
        }

        (context.println)(&format!(
            "{}: mutilating",
            util::strip_current_dir(&test_file).to_string_lossy()
        ));

        loop {
            let (mismatch, n) = skip_past_removals(&mut span_iter, &mut past_removal_iter);

            update_progress(&context, mismatch, n);

            let span = if let Some(span) = span_iter.next() {
                span
            } else {
                break;
            };

            let (text, outcome) = attempt_removal(&context, span)?;

            if CTRLC.load(Ordering::SeqCst) {
                bail!("Ctrl-C detected");
            }

            if let Some(outcome) = outcome {
                emit(&mut context, span, &text, outcome)?;
            }

            update_progress(&context, false, 1);
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

fn process_options(opts: &mut Necessist) -> Result<()> {
    // smoelius: This list of incompatibilities is not exhaustive.
    incompatible!(opts, dump, quiet);
    incompatible!(opts, dump, reset);
    incompatible!(opts, dump, resume);
    incompatible!(opts, dump, no_sqlite);
    incompatible!(opts, keep_going, no_dry_run);
    incompatible!(opts, quiet, verbose);
    incompatible!(opts, reset, no_sqlite);
    incompatible!(opts, resume, no_sqlite);

    Ok(())
}

fn default_config(context: &LightContext, root: &Path) -> Result<()> {
    let path = root.join("necessist.toml");

    if path.exists() {
        bail!("A configuration file already exists at {:?}", path);
    }

    warn(context, "Configuration files are experimental");

    let toml = toml::to_string(&Config::default())?;

    write(path, toml).map_err(Into::into)
}

fn read_config(context: &LightContext, root: &Path) -> Result<Config> {
    let path = root.join("necessist.toml");

    if !path.exists() {
        return Ok(Config::default());
    }

    warn(context, "Configuration files are experimental");

    let contents = read_to_string(path)?;

    toml::from_str(&contents).map_err(Into::into)
}

fn dump(context: &LightContext, removals: &[Removal]) {
    let mut other_than_passed = false;
    for removal in removals {
        emit_to_console(context, removal);
        other_than_passed |= removal.outcome != Outcome::Passed;
    }

    if !context.opts.verbose && other_than_passed {
        warn(context, "More output would be produced with --verbose");
    }
}

fn find_framework(context: &LightContext) -> Result<Box<dyn frameworks::Interface>> {
    if context.opts.framework != Framework::Auto {
        return frameworks()
            .into_iter()
            .find(|framework| framework.name() == context.opts.framework.to_string())
            .ok_or_else(|| anyhow!("Failed to find framework `{}`", context.opts.framework));
    }

    let unflattened_frameworks = frameworks()
        .into_iter()
        .map(|framework| {
            if framework.applicable(context)? {
                Ok(Some(framework))
            } else {
                Ok(None)
            }
        })
        .collect::<Result<Vec<_>>>()?;

    let applicable_frameworks = unflattened_frameworks
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    ensure!(
        applicable_frameworks.len() <= 1,
        "Found multiple applicable frameworks: {:#?}",
        applicable_frameworks
    );

    applicable_frameworks
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("Found no applicable frameworks"))
}

fn canonicalize_test_files(context: &LightContext) -> Result<Vec<PathBuf>> {
    context
        .opts
        .test_files
        .iter()
        .map(|path| {
            let path = path.canonicalize()?;
            ensure!(
                path.starts_with(context.root),
                "{:?} is not in {:?}",
                path,
                context.root
            );
            Ok(path)
        })
        .collect::<Result<Vec<_>>>()
}

#[must_use]
fn skip_past_removals<'a, I, J>(
    span_iter: &mut Peekable<I>,
    removal_iter: &mut Peekable<J>,
) -> (bool, usize)
where
    I: Iterator<Item = &'a Span>,
    J: Iterator<Item = Removal>,
{
    let mut mismatch = false;
    let mut n = 0;
    while let Some(&span) = span_iter.peek() {
        let removal = if let Some(removal) = removal_iter.peek() {
            removal
        } else {
            break;
        };
        match span.cmp(&removal.span) {
            std::cmp::Ordering::Less => {
                mismatch = true;
                break;
            }
            std::cmp::Ordering::Equal => {
                let _ = span_iter.next();
                let _removal = removal_iter.next();
                n += 1;
            }
            std::cmp::Ordering::Greater => {
                mismatch = true;
                let _removal = removal_iter.next();
            }
        }
    }

    (mismatch, n)
}

fn update_progress(context: &Context, mismatch: bool, n: usize) {
    if mismatch {
        warn_once(
            &context.light(),
            "Configuration or test files have changed since necessist.db was created",
            WarnKey::ConfigurationOrTestFilesHaveChanged,
        );
    }

    if let Some(bar) = context.progress {
        bar.inc(n as u64);
    }
}

fn build_test_file_span_map(mut spans: Vec<Span>) -> BTreeMap<SourceFile, Vec<Span>> {
    let mut test_file_span_map = BTreeMap::new();

    spans.sort();

    for span in spans {
        let test_file_spans = test_file_span_map
            .entry(span.source_file.clone())
            .or_insert_with(Vec::default);
        test_file_spans.push(span);
    }

    test_file_span_map
}

fn attempt_removal(context: &Context, span: &Span) -> Result<(String, Option<Outcome>)> {
    let _backup = Backup::new(&*span.source_file)?;

    let contents = read_to_string(&*span.source_file)?;
    let mut rewriter = Rewriter::new(&contents);
    let text = rewriter.rewrite(span, "");

    let mut file = OpenOptions::new()
        .truncate(true)
        .write(true)
        .open(&*span.source_file)?;
    file.write_all(rewriter.contents().as_bytes())?;

    let exec = context.framework.exec(&context.light(), span)?;

    let (exec, postprocess) = if let Some((exec, postprocess)) = exec {
        (exec, postprocess)
    } else {
        return Ok((text, Some(Outcome::Nonbuildable)));
    };

    debug!("{:?}", exec);

    let mut popen = exec.popen()?;
    let status = if let Some(dur) = timeout(&context.opts) {
        popen.wait_timeout(dur)?
    } else {
        popen.wait().map(Option::Some)?
    };

    if status.is_some() {
        if let Some(postprocess) = postprocess {
            if !postprocess(&context.light(), popen)? {
                return Ok((text, None));
            }
        }
    } else {
        let pid = popen.pid().ok_or_else(|| anyhow!("Failed to get pid"))?;
        recursive_kill(pid)?;
        let _ = popen.wait()?;
    }

    let status = if let Some(status) = status {
        status
    } else {
        return Ok((text, Some(Outcome::TimedOut)));
    };

    Ok((
        text,
        Some(if status.success() {
            Outcome::Passed
        } else {
            Outcome::Failed
        }),
    ))
}

#[cfg_attr(
    dylint_lib = "non_local_effect_before_error_return",
    allow(non_local_effect_before_error_return)
)]
fn emit(context: &mut Context, span: &Span, text: &str, outcome: Outcome) -> Result<()> {
    let removal = Removal {
        span: span.clone(),
        text: text.to_owned(),
        outcome,
    };

    if let Some(sqlite) = context.sqlite.as_mut() {
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
            text,
            if atty::is(atty::Stream::Stdout) {
                outcome.style().bold()
            } else {
                Style::default()
            }
            .paint(outcome.to_string())
        );
        (context.println)(&msg);
    }
}

fn timeout(opts: &Necessist) -> Option<Duration> {
    match opts.timeout {
        None => Some(DEFAULT_TIMEOUT),
        Some(0) => None,
        Some(secs) => Some(Duration::from_secs(secs)),
    }
}

fn recursive_kill(pid: u32) -> Result<()> {
    let output = Command::new("pgrep")
        .args(["-P", &pid.to_string()])
        .output()?;

    let stdout = String::from_utf8(output.stdout)?;

    for line in stdout.lines() {
        let pid = line.parse::<u32>()?;
        recursive_kill(pid)?;
    }

    let _status = Command::new("kill")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    // smoelius: The process may have already exited.
    // ensure!(status.success());

    Ok(())
}
