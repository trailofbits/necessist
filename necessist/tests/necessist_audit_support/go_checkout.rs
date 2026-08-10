use super::super::{GO_REPO, GO_REV, cache_dir, command_output, display_path, workspace_root};
use std::{
    env::{join_paths, split_paths, var_os},
    ffi::OsString,
    fs::{copy, create_dir_all, read_dir, remove_dir_all},
    path::{Path, PathBuf},
    process::Command,
};

pub(crate) struct GoCheckout {
    root: PathBuf,
    run_dir: PathBuf,
    envs: Vec<(OsString, OsString)>,
}

impl GoCheckout {
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn run_dir(&self) -> &Path {
        &self.run_dir
    }

    fn env_as_path(&self, key: &str) -> &Path {
        self.envs
            .iter()
            .find_map(|(name, value)| (name == key).then(|| Path::new(value)))
            .unwrap_or_else(|| panic!("missing Go environment variable {key}"))
    }

    pub(crate) fn gocache(&self) -> &Path {
        self.env_as_path("GOCACHE")
    }

    pub(crate) fn gotmpdir(&self) -> &Path {
        self.env_as_path("GOTMPDIR")
    }

    pub(crate) fn path_env(&self) -> OsString {
        let mut paths = vec![workspace_root().join("target/debug"), self.root.join("bin")];
        if let Some(path_var) = var_os("PATH") {
            paths.extend(split_paths(&path_var));
        }
        join_paths(paths).unwrap()
    }

    pub(crate) fn envs(&self) -> impl Iterator<Item = (&OsString, &OsString)> {
        self.envs.iter().map(|(key, value)| (key, value))
    }
}

pub(crate) fn prepare_go_checkout_for_run(harness_name: &str) -> GoCheckout {
    let base = prepare_base_go_checkout();
    let run_dir = cache_dir().join("runs").join(harness_name);
    let go_root = run_dir.join(format!("go-{GO_REV}"));

    if go_root.try_exists().unwrap() {
        remove_dir_all(&go_root).unwrap();
    }
    create_dir_all(&run_dir).unwrap();
    copy_dir(base.root(), &go_root);

    let envs = go_envs(&run_dir);
    GoCheckout {
        root: go_root,
        run_dir,
        envs,
    }
}

fn prepare_base_go_checkout() -> GoCheckout {
    let base_dir = cache_dir().join("base");
    let go_root = base_dir.join(format!("go-{GO_REV}"));
    let envs = go_envs(&base_dir);
    let smtp_dir = go_root.join("src/net/smtp");

    if smtp_dir.join("necessist.db").is_file() {
        eprintln!(
            "Using cached necessist.db in {}",
            display_path(&smtp_dir).display()
        );
        return GoCheckout {
            root: go_root,
            run_dir: base_dir,
            envs,
        };
    }

    let existing_dir = cache_dir().join(format!("go-{GO_REV}"));
    let existing_db = existing_dir.join("src/net/smtp/necessist.db");
    if existing_db.is_file() {
        create_dir_all(&base_dir).unwrap();
        copy_dir(&existing_dir, &go_root);
        eprintln!(
            "Using cached necessist.db in {}",
            display_path(&smtp_dir).display()
        );
        return GoCheckout {
            root: go_root,
            run_dir: base_dir,
            envs,
        };
    }

    eprintln!(
        "Preparing necessist.db in {}",
        display_path(&smtp_dir).display()
    );

    if !go_root.join(".git").is_dir() {
        create_dir_all(&base_dir).unwrap();
        command_output(
            Command::new("git")
                .arg("init")
                .arg(&go_root)
                .current_dir(&base_dir),
        );
        command_output(
            Command::new("git")
                .args(["fetch", "--depth=1", GO_REPO, GO_REV])
                .current_dir(&go_root),
        );
        command_output(
            Command::new("git")
                .args(["checkout", "--detach", "FETCH_HEAD"])
                .current_dir(&go_root),
        );
    }

    if !go_root.join("bin/go").is_file() {
        command_output(
            Command::new("bash")
                .arg("make.bash")
                .current_dir(go_root.join("src"))
                .envs(envs.iter().map(|(key, value)| (key, value))),
        );
    }

    let go_checkout = GoCheckout {
        root: go_root,
        run_dir: base_dir,
        envs,
    };
    prepare_necessist_db(&go_checkout, &smtp_dir);
    assert!(
        smtp_dir.join("necessist.db").is_file(),
        "necessist.db was not created in {}",
        display_path(&smtp_dir).display()
    );

    go_checkout
}

fn copy_dir(from: &Path, to: &Path) {
    create_dir_all(to).unwrap();
    for entry in read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let from_path = entry.path();
        let to_path = to.join(entry.file_name());
        let file_type = entry.file_type().unwrap();
        if file_type.is_dir() {
            copy_dir(&from_path, &to_path);
        } else if file_type.is_file() {
            copy(&from_path, &to_path).unwrap();
        } else {
            panic!(
                "unexpected file of type {file_type:?} at: {}",
                from_path.display()
            );
        }
    }
}

fn prepare_necessist_db(go_checkout: &GoCheckout, smtp_dir: &Path) {
    command_output(
        Command::new("cargo")
            .args([
                "run",
                "--package=necessist",
                "--",
                "--framework=go",
                "--reset",
                &format!("--root={}", smtp_dir.to_string_lossy()),
                "--timeout=5",
                "smtp_test.go",
            ])
            .current_dir(workspace_root())
            .env("PATH", go_checkout.path_env())
            .env("GOROOT", go_checkout.root())
            .envs(go_checkout.envs()),
    );
}

fn go_envs(run_dir: &Path) -> Vec<(OsString, OsString)> {
    let gocache = run_dir.join("gocache");
    let gotmpdir = run_dir.join("gotmpdir");
    create_dir_all(&gocache).unwrap();
    create_dir_all(&gotmpdir).unwrap();

    let mut envs = vec![
        (OsString::from("CGO_ENABLED"), OsString::from("0")),
        (OsString::from("GOCACHE"), gocache.into_os_string()),
        (OsString::from("GOTMPDIR"), gotmpdir.into_os_string()),
    ];

    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        envs.extend([
            (OsString::from("GOHOSTARCH"), OsString::from("amd64")),
            (OsString::from("GOARCH"), OsString::from("amd64")),
        ]);
    }

    envs
}
