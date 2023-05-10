use crate::{warn, LightContext, WarnFlags, Warning};
use anyhow::{bail, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{fs::read_to_string, path::Path};

pub struct Compiled {
    ignored_functions: Vec<Regex>,
    ignored_macros: Vec<Regex>,
    ignored_methods: Vec<Regex>,
}

impl Compiled {
    #[must_use]
    pub fn is_ignored_function(&self, name: &str) -> bool {
        self.ignored_functions.iter().any(|re| re.is_match(name))
    }
    #[must_use]
    pub fn is_ignored_macro(&self, name: &str) -> bool {
        self.ignored_macros.iter().any(|re| re.is_match(name))
    }
    #[must_use]
    pub fn is_ignored_method(&self, name: &str) -> bool {
        self.ignored_methods.iter().any(|re| re.is_match(name))
    }
}

#[derive(Default, Deserialize, Serialize)]
pub struct Toml {
    #[serde(default)]
    pub ignored_functions: Vec<String>,
    #[serde(default)]
    pub ignored_macros: Vec<String>,
    #[serde(default)]
    pub ignored_methods: Vec<String>,
}

impl Toml {
    pub fn read(context: &LightContext, root: &Path) -> Result<Self> {
        let path_buf = root.join("necessist.toml");

        if !path_buf.try_exists()? {
            return Ok(Self::default());
        }

        warn(
            context,
            Warning::ConfigFilesExperimental,
            "Configuration files are experimental",
            WarnFlags::empty(),
        )?;

        let contents = read_to_string(path_buf)?;

        toml::from_str(&contents).map_err(Into::into)
    }

    pub fn merge(&mut self, other: &Self) -> &mut Self {
        let Toml {
            ignored_functions,
            ignored_macros,
            ignored_methods,
        } = other;

        self.ignored_functions.extend_from_slice(ignored_functions);
        self.ignored_macros.extend_from_slice(ignored_macros);
        self.ignored_methods.extend_from_slice(ignored_methods);

        self
    }

    pub fn compile(&self) -> Result<Compiled> {
        let Toml {
            ignored_functions,
            ignored_macros,
            ignored_methods,
        } = self;

        let ignored_functions = compile_ignored(ignored_functions)?;
        let ignored_macros = compile_ignored(ignored_macros)?;
        let ignored_methods = compile_ignored(ignored_methods)?;

        Ok(Compiled {
            ignored_functions,
            ignored_macros,
            ignored_methods,
        })
    }
}

fn compile_ignored(ignored: &[String]) -> Result<Vec<Regex>> {
    ignored
        .iter()
        .map(AsRef::as_ref)
        .map(compile_pattern)
        .collect()
}

fn compile_pattern(pattern: &str) -> Result<Regex> {
    let escaped = escape(pattern)?;

    Regex::new(&(String::from("^") + &escaped + "$")).map_err(Into::into)
}

fn escape(pattern: &str) -> Result<String> {
    let mut s = String::new();

    for ch in pattern.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            s.push(ch);
        } else if ch == '.' {
            s.push_str("\\.");
        } else if ch == '*' {
            s.push_str(".*");
        } else {
            bail!(
                "Patterns can contain only letters, numbers, '.', '_', or `*`, which does not \
                 include '{}'",
                ch
            );
        }
    }

    Ok(s)
}
