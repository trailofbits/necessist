use super::super::{
    GO_REV, JSON_REPORT, OUTPUT_POLL_INTERVAL, TIMEOUT, display_path, format_output, skill_dir,
};
use super::accept::contains_acceptable_result;
use super::go_checkout::{GoCheckout, prepare_go_checkout_for_run};
use serde_json::{Value, from_str};
use std::{
    env::vars_os,
    fs::{read_to_string, remove_file},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::{Mutex, OnceLock, PoisonError},
    thread::sleep,
    time::Instant,
};

pub(crate) struct Tool {
    pub(crate) name: &'static str,
    pub(crate) tool_path: PathBuf,
    args_fn: fn(&GoCheckout, String) -> Vec<String>,
    stdin_null: bool,
}

impl Tool {
    pub(crate) fn claude(tool_path: PathBuf) -> Self {
        Self {
            name: "claude",
            tool_path,
            args_fn: |_go_checkout, prompt| {
                vec![
                    format!("--add-dir={}", skill_dir().display()),
                    "--allowedTools=Bash,Read,Write".to_owned(),
                    "--permission-mode=acceptEdits".to_owned(),
                    "--print".to_owned(),
                    prompt,
                ]
            },
            stdin_null: false,
        }
    }

    pub(crate) fn codex(tool_path: PathBuf) -> Self {
        Self {
            name: "codex",
            tool_path,
            args_fn: codex_args,
            stdin_null: true,
        }
    }

    pub(crate) fn run(&self) {
        let go_checkout = {
            let _guard = cache_lock().lock().unwrap_or_else(PoisonError::into_inner);

            eprintln!("Preparing cached Go checkout {GO_REV}");
            prepare_go_checkout_for_run(self.name)
        };

        let smtp_dir = go_checkout.root().join("src/net/smtp");
        let report_path = smtp_dir.join(JSON_REPORT);

        drop(remove_file(&report_path));
        eprintln!(
            "Running {} in {}",
            self.name,
            display_path(&smtp_dir).display()
        );

        run_tool_command(&go_checkout, self, &smtp_dir, &report_path).unwrap_or_else(|error| {
            panic!(
                "{} did not produce an acceptable report: {error}",
                self.name
            );
        });
    }
}

fn codex_args(go_checkout: &GoCheckout, prompt: String) -> Vec<String> {
    let mut builder = CodexCommandBuilder::new(go_checkout);

    builder.push_config("sandbox_workspace_write.network_access", "true");
    builder.push_config("shell_environment_policy.inherit", "\"none\"");
    builder.push_env_config("CGO_ENABLED", &"0");
    builder.push_env_config("GOARCH", &"amd64");
    builder.push_env_config("GOCACHE", &go_checkout.gocache().display());
    builder.push_env_config("GOHOSTARCH", &"amd64");
    builder.push_env_config("GOROOT", &go_checkout.root().display());
    builder.push_env_config("GOTMPDIR", &go_checkout.gotmpdir().display());
    builder.push_env_config("PATH", &go_checkout.path_env().to_string_lossy());

    builder.finish(prompt)
}

struct CodexCommandBuilder {
    args: Vec<String>,
}

impl CodexCommandBuilder {
    fn new(go_checkout: &GoCheckout) -> Self {
        Self {
            args: vec![
                "exec".to_owned(),
                format!("--add-dir={}", go_checkout.run_dir().to_string_lossy()),
                "--sandbox=workspace-write".to_owned(),
            ],
        }
    }

    fn push_config(&mut self, key: &str, value: &str) {
        self.args.push(format!("--config={key}={value}"));
    }

    fn push_env_config(&mut self, key: &str, value: &dyn std::fmt::Display) {
        self.push_config(
            &format!("shell_environment_policy.set.{key}"),
            &format!("\"{value}\""),
        );
    }

    fn finish(mut self, prompt: String) -> Vec<String> {
        self.args.push(prompt);
        self.args
    }
}

fn run_tool_command(
    go_checkout: &GoCheckout,
    tool: &Tool,
    smtp_dir: &Path,
    report_path: &Path,
) -> Result<(), String> {
    let prompt = format!(
        "Use the Necessist audit skill at {}.",
        skill_dir().join("SKILL.md").display()
    );

    let mut command = Command::new(&tool.tool_path);
    command
        .args((tool.args_fn)(go_checkout, prompt))
        .current_dir(smtp_dir)
        .env_clear()
        .envs(vars_os())
        .env("PATH", go_checkout.path_env())
        .env("GOROOT", go_checkout.root())
        .envs(go_checkout.envs());

    if tool.stdin_null {
        command.stdin(Stdio::null());
    }

    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let start = Instant::now();

    loop {
        if child.try_wait().unwrap().is_some() {
            let output = child.wait_with_output().unwrap();
            if !output.status.success() {
                return Err(format!("skill command failed\n{}", format_output(&output)));
            }

            if !report_path.is_file() {
                return Err(format!(
                    "`{JSON_REPORT}` was not created\n{}",
                    format_output(&output)
                ));
            }

            eprintln!("Verifying {JSON_REPORT} produced by {}", tool.name);
            return verify_json_report(report_path, Some(&output));
        }

        if report_path.is_file() && verify_json_report(report_path, None).is_ok() {
            eprintln!("Verifying {JSON_REPORT} produced by {}", tool.name);
            drop(child.kill());
            drop(child.wait());
            return Ok(());
        }

        if start.elapsed() >= TIMEOUT {
            drop(child.kill());
            drop(child.wait());
            return Err(format!("skill command timed out after {TIMEOUT:?}"));
        }

        sleep(OUTPUT_POLL_INTERVAL);
    }
}

fn verify_json_report(report_path: &Path, output: Option<&Output>) -> Result<(), String> {
    let contents = read_to_string(report_path).unwrap();
    let value: Value = from_str(&contents).unwrap();
    let object = value.as_object().unwrap();

    assert_eq!(
        object.get("version"),
        Some(&Value::String("0.1.0".to_owned()))
    );

    let findings = json_array(object.get("findings"), "findings");
    let leads = json_array(object.get("leads"), "leads");

    if contains_acceptable_result(findings, leads) {
        return Ok(());
    }

    let mut msg = format!("`findings`/`leads` do not contain an acceptable result in {contents}");
    if let Some(output) = output {
        msg.push('\n');
        msg.push_str(&format_output(output));
    }
    Err(msg)
}

fn json_array<'a>(value: Option<&'a Value>, key: &str) -> &'a Vec<Value> {
    value
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("missing {key} array"))
}

fn cache_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(Mutex::default)
}
