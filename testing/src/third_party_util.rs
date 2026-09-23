use regex::Regex;
use std::{collections::BTreeMap, path::Path, sync::LazyLock};

pub fn normalize_paths(mut s: &str, path: &Path) -> String {
    let path_str = path.to_string_lossy();
    let mut buf = String::new();
    while let Some(i) = s.find(&*path_str) {
        buf.push_str(&s[..i]);
        buf.push_str("$DIR");
        s = &s[i + path_str.len()..];
        // smoelius: Replace `\` up until the next whitespace.
        let n = s.find(char::is_whitespace).unwrap_or(s.len());
        buf.push_str(&s[..n].replace('\\', "/"));
        s = &s[n..];
    }
    // smoelius: Push whatever is remaining.
    buf.push_str(s);
    buf
}

// smoelius: Don't put a `\b` at the start of this pattern. `assert_cmd::output::OutputError`
// escapes control characters (e.g., `\t`) and its output appears in the stdout files. So adding a
// `\b` could introduce false negatives.
pub static TIMING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[0-9]+\.[0-9]+(m?)s\b").unwrap());

pub fn remove_timings(s: &str) -> String {
    TIMING_RE.replace_all(s, "[..]${1}s").to_string()
}

#[must_use]
pub fn permutation_ignoring_timeouts(expected: &str, actual: &str) -> bool {
    let expected_lines = expected.lines().collect::<Vec<_>>();
    let actual_lines = actual.lines().collect::<Vec<_>>();
    if expected_lines.len() != actual_lines.len() {
        return false;
    }
    let expected_map = summarize_lines(&expected_lines);
    let actual_map = summarize_lines(&actual_lines);
    if expected_map.len() != actual_map.len() {
        return false;
    }
    expected_map.into_iter().zip(actual_map).all(
        |((expected_prefix, expected_summary), (actual_prefix, actual_summary))| {
            if expected_prefix != actual_prefix {
                return false;
            }
            compare_summaries(&expected_summary, &actual_summary)
        },
    )
}

#[derive(Default)]
struct Summary<'a> {
    non_timeouts: BTreeMap<&'a str, usize>,
    n_timeouts: usize,
}

fn summarize_lines<'a>(lines: &[&'a str]) -> BTreeMap<&'a str, Summary<'a>> {
    let mut map = BTreeMap::<_, Summary>::new();
    for line in lines {
        let (prefix, suffix) = line.rsplit_once(' ').unwrap_or((line, ""));
        let summary = map.entry(prefix).or_default();
        if suffix == "timed-out" {
            summary.n_timeouts += 1;
        } else {
            *summary.non_timeouts.entry(suffix).or_default() += 1;
        }
    }
    map
}

fn compare_summaries(expected: &Summary<'_>, actual: &Summary<'_>) -> bool {
    for (actual_suffix, actual_n) in &actual.non_timeouts {
        let Some(expected_n) = expected.non_timeouts.get(actual_suffix) else {
            return false;
        };
        if actual_n > expected_n {
            return false;
        }
    }
    let mut deficit = 0;
    for (expected_suffix, expected_n) in &expected.non_timeouts {
        let actual_n = actual
            .non_timeouts
            .get(expected_suffix)
            .copied()
            .unwrap_or_default();
        deficit += expected_n - actual_n;
    }
    expected.n_timeouts + deficit == actual.n_timeouts
}

pub fn subsequence<'a, 'b>(
    xs: impl IntoIterator<Item = &'a str>,
    ys: impl IntoIterator<Item = &'b str>,
) -> bool {
    let re = Regex::new(r"^(\d+) candidates in (\d+) source file(s)?$").unwrap();

    let mut xs = xs.into_iter().peekable();
    let mut ys = ys.into_iter();

    while let Some(&x) = xs.peek() {
        let Some(y) = ys.next() else {
            dbg!(x);
            return false;
        };
        if x == y || (re.is_match(x) && re.is_match(y)) {
            let _: Option<&str> = xs.next();
        }
    }

    true
}
