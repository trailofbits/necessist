use anyhow::{ensure, Result};
use log::debug;
use necessist_core::LightContext;
use std::process::Command;

pub fn install_node_modules(context: &LightContext) -> Result<()> {
    if context.root.join("node_modules").try_exists()? {
        return Ok(());
    }

    // smoelius: If a `yarn.lock` file exists, use `yarn`. Otherwise, default to `npm install`.
    let mut command = if context.root.join("yarn.lock").try_exists()? {
        Command::new("yarn")
    } else {
        let mut command = Command::new("npm");
        command.arg("install");
        command
    };

    command.current_dir(context.root.as_path());

    debug!("{:?}", command);

    let output = command.output()?;
    ensure!(output.status.success(), "{:#?}", output);
    Ok(())
}
