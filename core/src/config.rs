use crate::LightContext;
use anyhow::{bail, Result};
use regex::Regex;
use std::{fs::read_to_string, path::Path};

#[derive(Clone, Copy, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum IgnoredPathDisambiguation {
    #[default]
    None,
    Function,
    Method,
}

pub struct Compiled {
    ignored_functions: Vec<Regex>,
    ignored_macros: Vec<Regex>,
    ignored_methods: Vec<Regex>,
    ignored_path_disambiguation: IgnoredPathDisambiguation,
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
    #[must_use]
    pub fn ignored_path_disambiguation(&self) -> IgnoredPathDisambiguation {
        self.ignored_path_disambiguation
    }
}

#[derive(Default, serde::Deserialize, serde::Serialize)]
pub struct Toml {
    #[serde(default)]
    pub ignored_functions: Vec<String>,
    #[serde(default)]
    pub ignored_macros: Vec<String>,
    #[serde(default)]
    pub ignored_methods: Vec<String>,
    #[serde(default)]
    pub ignored_path_disambiguation: Option<IgnoredPathDisambiguation>,
}

impl Toml {
    pub fn read(_context: &LightContext, root: &Path) -> Result<Self> {
        let path_buf = root.join("necessist.toml");

        if !path_buf.try_exists()? {
            return Ok(Self::default());
        }

        let contents = read_to_string(path_buf)?;

        toml::from_str(&contents).map_err(Into::into)
    }

    pub fn merge(&mut self, other: &Self) -> Option<&mut Self> {
        let Toml {
            ignored_functions,
            ignored_macros,
            ignored_methods,
            ignored_path_disambiguation,
        } = other;

        if self.ignored_path_disambiguation.is_some()
            && other.ignored_path_disambiguation.is_some()
            && self.ignored_path_disambiguation != *ignored_path_disambiguation
        {
            return None;
        }

        self.ignored_functions.extend_from_slice(ignored_functions);
        self.ignored_macros.extend_from_slice(ignored_macros);
        self.ignored_methods.extend_from_slice(ignored_methods);

        self.ignored_path_disambiguation = *ignored_path_disambiguation;

        Some(self)
    }

    pub fn compile(&self) -> Result<Compiled> {
        let Toml {
            ignored_functions,
            ignored_macros,
            ignored_methods,
            ignored_path_disambiguation,
        } = self;

        let ignored_functions = compile_ignored(ignored_functions)?;
        let ignored_macros = compile_ignored(ignored_macros)?;
        let ignored_methods = compile_ignored(ignored_methods)?;

        Ok(Compiled {
            ignored_functions,
            ignored_macros,
            ignored_methods,
            ignored_path_disambiguation: ignored_path_disambiguation.unwrap_or_default(),
        })
    }
}

fn compile_ignored(ignored: impl IntoIterator<Item = impl AsRef<str>>) -> Result<Vec<Regex>> {
    ignored
        .into_iter()
        .map(|pattern| compile_pattern(pattern.as_ref()))
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

#[test]
fn patterns() {
    const EXAMPLES: &[(&str, &[&str], &[&str])] = &[
        (
            "assert",
            &["assert"],
            &["assert_eq", "assertEqual", "assert.Equal"],
        ),
        (
            "assert_eq",
            &["assert_eq"],
            &["assert", "assertEqual", "assert.Equal"],
        ),
        (
            "assertEqual",
            &["assertEqual"],
            &["assert", "assert_eq", "assert.Equal"],
        ),
        (
            "assert.Equal",
            &["assert.Equal"],
            &["assert", "assert_eq", "assertEqual"],
        ),
        (
            "assert.*",
            &["assert.Equal"],
            &["assert", "assert_eq", "assertEqual"],
        ),
        (
            "assert*",
            &["assert", "assert_eq", "assertEqual", "assert.Equal"],
            &[],
        ),
        ("*.Equal", &["assert.Equal"], &["Equal"]),
    ];

    for (pattern, positive, negative) in EXAMPLES {
        let re = compile_pattern(pattern).unwrap();
        for text in *positive {
            assert!(re.is_match(text));
        }
        for text in *negative {
            assert!(!re.is_match(text));
        }
    }
}
