use std::process::Command;
use subprocess::Exec;

pub fn exec_from_command(command: &Command) -> Exec {
    let mut exec = Exec::cmd(command.get_program()).args(&command.get_args().collect::<Vec<_>>());
    for (key, val) in command.get_envs() {
        if let Some(val) = val {
            exec = exec.env(key, val);
        } else {
            exec = exec.env_remove(key);
        }
    }
    if let Some(path) = command.get_current_dir() {
        exec = exec.cwd(path);
    }
    exec
}
