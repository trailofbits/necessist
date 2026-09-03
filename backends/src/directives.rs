//! Source directive support
//!
//! A directive is a line comment of the form `// necessist: ...` or `# necessist: ...`. Directives
//! are gathered by [`Directives::collect`] before a source file is parsed, since an honored
//! `skip-file` directive causes the file not to be parsed at all.

use anyhow::Result;
use necessist_core::{LightContext, LineColumn, SourceFile, Span, WarnFlags, Warning, source_warn};
use std::collections::BTreeSet;

#[derive(Clone, Copy)]
pub enum DirectiveSyntax {
    Php,
    Python,
    Slash,
}

impl DirectiveSyntax {
    fn line_comment_prefixes(self) -> &'static [&'static str] {
        match self {
            Self::Php => &["#", "//"],
            Self::Python => &["#"],
            Self::Slash => &["//"],
        }
    }
}

pub struct Directives {
    syntax: DirectiveSyntax,
    skip_file: bool,
    skip_lines: BTreeSet<usize>,
}

impl Directives {
    fn new(syntax: DirectiveSyntax) -> Self {
        Self {
            syntax,
            skip_file: false,
            skip_lines: BTreeSet::new(),
        }
    }

    pub fn collect(
        context: &LightContext,
        syntax: DirectiveSyntax,
        source_file: &SourceFile,
    ) -> Result<Self> {
        let contents = source_file.contents();

        let mut directives = Self::new(syntax);
        let mut in_file_header = true;

        for (index, line) in contents.lines().enumerate() {
            let line_number = index + 1;

            if directives.is_skip_file_directive(line) {
                if in_file_header {
                    directives.skip_file = true;
                } else {
                    let message = if matches!(directives.syntax, DirectiveSyntax::Php) {
                        "`necessist: skip-file` is preceded by a line that is not a line comment, \
                         whitespace, or `<?php`"
                    } else {
                        "`necessist: skip-file` is preceded by a line that is not a line comment \
                         or whitespace"
                    };
                    source_warn(
                        context,
                        Warning::SkipFileMispositioned,
                        &directive_span(source_file, line_number, line),
                        message,
                        WarnFlags::empty(),
                    )?;
                }
            } else if directives.is_skip_directive(line) {
                directives.skip_lines.insert(line_number + 1);
            } else if let Some(directive) = directives.directive_text(line) {
                source_warn(
                    context,
                    Warning::DirectiveUnrecognized,
                    &directive_span(source_file, line_number, line),
                    &format!("`necessist: {directive}` is not a recognized directive"),
                    WarnFlags::empty(),
                )?;
            }

            if !directives.is_file_header_line(line) {
                in_file_header = false;
            }
        }

        Ok(directives)
    }

    pub fn skip_file(&self) -> bool {
        self.skip_file
    }

    /// Returns whether a candidate beginning at `span` is skipped by a `necessist: skip` directive
    pub fn skip(&self, span: &Span) -> bool {
        // Computing the start line requires reading the spanned text. Most files contain no
        // directives, so check for that case first.
        !self.skip_lines.is_empty() && self.skip_lines.contains(&self.span_code_start_line(span))
    }

    // A code span can include preceding comments, such as `necessist: skip`. `span_code_start_line`
    // finds the first non-comment, non-whitespace line so the directive can be matched to the code
    // it skips. Without this adjustment, the `skip_method_call` test in the `directives` `trycmd`
    // test would fail.
    fn span_code_start_line(&self, span: &Span) -> usize {
        let Ok(text) = span.source_text() else {
            return span.start.line;
        };
        let line_offset = text
            .lines()
            .position(|line| !self.is_line_comment_or_whitespace(line))
            .unwrap_or_default();
        span.start.line + line_offset
    }

    fn is_skip_file_directive(&self, line: &str) -> bool {
        self.directive_text(line)
            .and_then(|rest| rest.strip_prefix("skip-file"))
            .is_some_and(has_word_boundary)
    }

    fn is_skip_directive(&self, line: &str) -> bool {
        self.directive_text(line)
            .and_then(|rest| rest.strip_prefix("skip"))
            .is_some_and(has_word_boundary)
    }

    fn directive_text<'a>(&self, line: &'a str) -> Option<&'a str> {
        let line = line.trim_start();
        let rest = self
            .syntax
            .line_comment_prefixes()
            .iter()
            .find_map(|prefix| line.strip_prefix(prefix))?;
        let rest = rest.trim_start().strip_prefix("necessist:")?;
        Some(rest.trim_start())
    }

    // A PHP test file opens with `<?php`. Allowing such a line in a file header is needed to make
    // `necessist: skip-file` usable in such a file.
    fn is_file_header_line(&self, line: &str) -> bool {
        self.is_line_comment_or_whitespace(line)
            || matches!(self.syntax, DirectiveSyntax::Php) && line.trim() == "<?php"
    }

    fn is_line_comment_or_whitespace(&self, line: &str) -> bool {
        let rest = line.trim_start();
        rest.is_empty()
            || self
                .syntax
                .line_comment_prefixes()
                .iter()
                .any(|prefix| rest.starts_with(prefix))
    }
}

fn directive_span(source_file: &SourceFile, line_number: usize, line: &str) -> Span {
    let column = line.chars().take_while(|ch| ch.is_whitespace()).count();
    Span {
        source_file: source_file.clone(),
        start: LineColumn {
            line: line_number,
            column,
        },
        end: LineColumn {
            line: line_number,
            column: column + line.trim().chars().count(),
        },
    }
}

fn has_word_boundary(rest: &str) -> bool {
    rest.chars()
        .next()
        .is_none_or(|ch| ch != '-' && !ch.is_alphanumeric() && ch != '_')
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn skip_directives_are_backend_specific() {
        let php = Directives::new(DirectiveSyntax::Php);
        let python = Directives::new(DirectiveSyntax::Python);
        let slash = Directives::new(DirectiveSyntax::Slash);
        assert!(php.is_skip_directive("# necessist: skip"));
        assert!(php.is_skip_directive("// necessist: skip"));
        assert!(python.is_skip_directive("# necessist: skip"));
        assert!(!python.is_skip_directive("// necessist: skip"));
        assert!(!slash.is_skip_directive("# necessist: skip"));
        assert!(slash.is_skip_directive("// necessist: skip"));
    }

    #[test]
    fn skip_directive_syntax() {
        // Keep these cases in sync with `fixtures/directives`.
        const CASES: &[&str] = &[
            "    // necessist: skip",
            "    //necessist:skip",
            "    // necessist: skip, reason for skipping",
        ];
        for &line in CASES {
            let directives = Directives::new(DirectiveSyntax::Slash);
            assert!(directives.is_skip_directive(line), "{line:?}");
            assert!(!directives.is_skip_file_directive(line), "{line:?}");
        }
    }

    #[test]
    fn skip_file_directive_syntax() {
        // Keep these cases in sync with `fixtures/directives`, `fixtures/php_skip_file`,
        // `fixtures/skip_invalid_file`, and `fixtures/python_skip_file`.
        const CASES: &[(DirectiveSyntax, &str)] = &[
            (
                DirectiveSyntax::Slash,
                "    // necessist: skip-file, too late 😞",
            ),
            (DirectiveSyntax::Slash, "// necessist: skip-file"),
            (
                DirectiveSyntax::Slash,
                "// necessist: skip-file, deliberately invalid Rust follows",
            ),
            (DirectiveSyntax::Python, "# necessist: skip-file"),
        ];
        for &(syntax, line) in CASES {
            let directives = Directives::new(syntax);
            assert!(!directives.is_skip_directive(line), "{line:?}");
            assert!(directives.is_skip_file_directive(line), "{line:?}");
        }
    }

    #[test]
    fn unrecognized_directive_syntax() {
        // Keep these cases in sync with `fixtures/directives`.
        const CASES: &[&str] = &[
            "    n += 5; // necessist: skip",
            "    // necessist: skip-filex",
        ];
        for &line in CASES {
            let directives = Directives::new(DirectiveSyntax::Slash);
            assert!(!directives.is_skip_directive(line), "{line:?}");
            assert!(!directives.is_skip_file_directive(line), "{line:?}");
        }
    }

    #[test]
    fn file_header_lines_allowed() {
        // Keep the `<?php` case in sync with `fixtures/php_skip_file`.
        const CASES: &[(DirectiveSyntax, &str)] = &[
            (DirectiveSyntax::Slash, ""),
            (DirectiveSyntax::Slash, "\t"),
            (DirectiveSyntax::Slash, " "),
            (DirectiveSyntax::Slash, "// comment"),
            (DirectiveSyntax::Slash, "/// doc comment"),
            (DirectiveSyntax::Slash, "//! inner doc comment"),
            (DirectiveSyntax::Python, "# Python comment"),
            (DirectiveSyntax::Python, "#! /usr/bin/env python"),
            (DirectiveSyntax::Python, "#!/usr/bin/env python3"),
            (DirectiveSyntax::Php, "<?php"),
        ];
        for &(syntax, line) in CASES {
            let directives = Directives::new(syntax);
            assert!(directives.is_file_header_line(line), "{line:?}");
        }
    }

    #[test]
    fn file_header_lines_rejected() {
        const CASES: &[(DirectiveSyntax, &str)] = &[
            (DirectiveSyntax::Slash, "#[test]"),
            (DirectiveSyntax::Slash, "/* comment */"),
            (DirectiveSyntax::Slash, "<?php"),
            (DirectiveSyntax::Php, "<?php declare(strict_types=1);"),
            (DirectiveSyntax::Slash, "other"),
        ];
        for &(syntax, line) in CASES {
            let directives = Directives::new(syntax);
            assert!(!directives.is_file_header_line(line), "{line:?}");
        }
    }

    // `<?php` is allowed in a file header, but it is not a line comment. `span_code_start_line`
    // must not skip over it when looking for the line a candidate begins on.
    #[test]
    fn php_open_tag_is_not_a_line_comment() {
        assert!(Directives::new(DirectiveSyntax::Php).is_file_header_line("<?php"));
        assert!(!Directives::new(DirectiveSyntax::Php).is_line_comment_or_whitespace("<?php"));
    }
}
