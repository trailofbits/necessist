use crate::{frameworks, sqlite, util, Backup, Outcome, Rewriter, SourceFile, Span};
use ansi_term::{Color::Yellow, Style};
use anyhow::{anyhow, bail, ensure, Context as _, Result};
use heck::ToKebabCase;
use indicatif::ProgressBar;
use log::debug;
use std::{
    collections::BTreeMap,
    env::{current_dir, var},
    fs::{read_to_string, OpenOptions},
    io::Write,
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
    sqlite: Option<sqlite::Sqlite>,
    opts: Necessist,
    root: PathBuf,
    println: &'a dyn Fn(&dyn AsRef<str>),
    framework: Box<dyn frameworks::Interface>,
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
    pub dump: bool,
    pub framework: Framework,
    pub keep_going: bool,
    pub no_dry_run: bool,
    pub quiet: bool,
    pub resume: bool,
    pub root: Option<PathBuf>,
    pub sqlite: bool,
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

#[allow(clippy::too_many_lines)]
pub fn necessist(opts: &Necessist) -> Result<()> {
    let mut opts = opts.clone();

    process_options(&mut opts)?;

    let root = opts
        .root
        .as_ref()
        .map_or_else(current_dir, |root| root.canonicalize())?;

    let (sqlite, completed) = if opts.dump || opts.sqlite {
        let create = !opts.dump && !opts.resume;
        let (sqlite, mut completed) = sqlite::init(&root, create)?;
        completed.sort_by(|left, right| left.span.cmp(&right.span));
        (Some(sqlite), completed)
    } else {
        (None, Vec::new())
    };

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

    if opts.dump {
        dump(&context, &completed);
        return Ok(());
    }

    let mut framework = find_framework(&context)?;

    let paths = canonicalize_test_files(&context)?;

    let mut spans = framework.parse(
        &context,
        &paths.iter().map(AsRef::as_ref).collect::<Vec<_>>(),
    )?;

    let n_spans = spans.len();

    spans.sort();

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

    #[allow(clippy::drop_non_drop)]
    drop(context);

    let mut context = Context {
        sqlite,
        opts,
        root,
        println: &|_| {},
        framework,
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
    }

    ctrlc::set_handler(|| CTRLC.store(true, Ordering::SeqCst))?;

    let mut completed_iter = completed.into_iter();

    for (test_file, spans) in &test_file_span_map {
        let mut spans = &spans[..];
        let n = skip_completed(&mut spans, &mut completed_iter);

        if let Some(bar) = progress.as_ref() {
            bar.inc(n as u64);
        }

        if spans.is_empty() {
            continue;
        }

        if !context.opts.no_dry_run {
            (context.println)(&format!(
                "{}: dry running",
                util::strip_current_dir(test_file).to_string_lossy()
            ));

            if let Err(error) = context
                .framework
                .dry_run(&context.light(), test_file.as_ref())
            {
                if context.opts.keep_going {
                    (context.println)(&format!(
                        "{}: dry run failed: {}",
                        util::strip_current_dir(test_file).to_string_lossy(),
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
            util::strip_current_dir(test_file).to_string_lossy()
        ));

        for span in spans {
            let (text, outcome) = attempt_removal(&context, span)?;

            if CTRLC.load(Ordering::SeqCst) {
                bail!("Ctrl-C detected");
            }

            if let Some(outcome) = outcome {
                emit(&mut context, span, &text, outcome)?;
            }

            if let Some(bar) = progress.as_ref() {
                bar.inc(1);
            }
        }
    }

    progress.as_ref().map(ProgressBar::finish);

    Ok(())
}

fn process_options(opts: &mut Necessist) -> Result<()> {
    // smoelius: This list of incompatibilities is not exhaustive.
    ensure!(
        !(opts.dump && opts.quiet),
        "--dump and --quiet are incompatible"
    );
    ensure!(
        !(opts.dump && opts.resume),
        "--dump and --resume are incompatible"
    );
    ensure!(
        !(opts.keep_going && opts.no_dry_run),
        "--keep-going and --no-dry-run are incompatible"
    );
    ensure!(
        !(opts.quiet && opts.verbose),
        "--quiet and --verbose are incompatible"
    );

    if opts.resume {
        opts.sqlite = true;
    }

    Ok(())
}

fn dump(context: &LightContext, removals: &[Removal]) {
    let mut other_than_passed = false;
    for removal in removals {
        emit_to_console(context, removal);
        other_than_passed |= removal.outcome != Outcome::Passed;
    }

    if !context.opts.verbose && other_than_passed {
        warn(
            context,
            None,
            "More output would be produced with --verbose",
        );
    }
}

fn find_framework(context: &LightContext) -> Result<Box<dyn frameworks::Interface>> {
    if context.opts.framework != Framework::Auto {
        return frameworks()
            .into_iter()
            .find(|framework| framework.name() == context.opts.framework.to_string())
            .ok_or_else(|| anyhow!("Could not find framework `{}`", context.opts.framework));
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

fn skip_completed(spans: &mut &[Span], iter: &mut impl Iterator<Item = Removal>) -> usize {
    let mut i = 0;
    while i < spans.len() {
        if let Some(removal) = iter.next() {
            assert_eq!(removal.span, spans[i], "File contents have changed");
            i += 1;
        } else {
            break;
        }
    }

    *spans = &spans[i..];

    i
}

fn build_test_file_span_map(spans: Vec<Span>) -> BTreeMap<SourceFile, Vec<Span>> {
    let mut test_file_span_map = BTreeMap::new();

    for span in spans {
        let test_file_spans = test_file_span_map
            .entry(span.source_file.clone())
            .or_insert_with(Vec::default);
        test_file_spans.push(span);
    }

    test_file_span_map
}

fn attempt_removal(context: &Context, span: &Span) -> Result<(String, Option<Outcome>)> {
    let _backup = Backup::new(span.source_file.as_ref())?;

    let contents = read_to_string(span.source_file.as_ref())?;
    let mut rewriter = Rewriter::new(&contents);
    let text = rewriter.rewrite(span, "");

    let mut file = OpenOptions::new()
        .truncate(true)
        .write(true)
        .open(span.source_file.as_ref())?;
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
        let pid = popen.pid().ok_or_else(|| anyhow!("Could not get pid"))?;
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

pub(crate) fn warn(context: &LightContext, span: Option<&Span>, msg: &str) {
    if !context.opts.quiet {
        (context.println)(&format!(
            "{}: {}{}",
            if atty::is(atty::Stream::Stdout) {
                Yellow.bold()
            } else {
                Style::default()
            }
            .paint("Warning"),
            span.map_or(String::new(), |span| format!(
                "{}: ",
                span.to_console_string()
            )),
            msg
        ));
    }
}
