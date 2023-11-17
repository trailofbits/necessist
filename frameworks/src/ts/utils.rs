use crate::utils::{OutputAccessors, OutputStrippedOfAnsiScapes};
use anyhow::{ensure, Result};
use log::debug;
use necessist_core::LightContext;
use std::process::Command;

pub fn install_node_modules(context: &LightContext) -> Result<()> {
    if context.root.join("node_modules").try_exists()? {
        return Ok(());
    }

    // smoelius: If a `pnpm-lock.yaml` file exists, use `pnpm install`. If a `yarn.lock` file
    // exists, use `yarn`. If neither exist, default to `npm install`.
    let mut command = if context.root.join("pnpm-lock.yaml").try_exists()? {
        let mut command = Command::new("pnpm");
        command.arg("install");
        command
    } else if context.root.join("yarn.lock").try_exists()? {
        Command::new("yarn")
    } else {
        let mut command = Command::new("npm");
        command.arg("install");
        command
    };

    command.current_dir(context.root.as_path());

    debug!("{:?}", command);

    let output = command.output_stripped_of_ansi_escapes()?;
    ensure!(output.status().success(), "{:#?}", output);
    Ok(())
}
