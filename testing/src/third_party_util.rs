use regex::Regex;
use std::{collections::BTreeMap, path::Path, sync::LazyLock};

/// Replaces occurrences of `path` in `s` with `$DIR`, and `\` with `/` in the paths that follow
/// them.
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

/// Replaces the timings in `s` with `[..]`, so that stdout can be compared run over run.
pub fn remove_timings(s: &str) -> String {
    TIMING_RE.replace_all(s, "[..]${1}s").to_string()
}

/// Returns true if `actual`'s lines are a permutation of `expected`'s, treating a `timed-out` line
/// in `actual` as a match for any line in `expected` with the same prefix.
///
/// A test can time out for reasons that have nothing to do with the change under test, e.g., a
/// loaded CI runner.
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

#[cfg(test)]
impl<'a> Summary<'a> {
    fn new(non_timeouts: &[(&'a str, usize)], n_timeouts: usize) -> Self {
        Self {
            non_timeouts: non_timeouts.iter().copied().collect(),
            n_timeouts,
        }
    }
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

/// Compares the lines sharing one prefix. `actual` may have fewer lines with a given non-timeout
/// suffix than `expected` has, provided the deficit is made up for by additional timeouts.
///
/// Each suffix in `actual` must appear in `expected`, and at most as many times as it appears
/// there. Rejecting a surplus here rather than relying on the caller's line counts ensures the
/// subtraction below cannot overflow.
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

/// Returns true if `xs` is a subsequence of `ys`, treating any two "N candidates in M source files"
/// lines as equal.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permutation_matches_identical_lines() {
        let lines = "A passed\nA failed\nB passed\n";
        assert!(permutation_ignoring_timeouts(lines, lines));
    }

    #[test]
    fn permutation_matches_reordered_lines() {
        assert!(permutation_ignoring_timeouts(
            "A passed\nB failed\n",
            "B failed\nA passed\n"
        ));
    }

    #[test]
    fn permutation_rejects_differing_line_counts() {
        assert!(!permutation_ignoring_timeouts(
            "A failed\n",
            "A failed\nA failed\n"
        ));
    }

    #[test]
    fn permutation_ignores_a_timeout() {
        assert!(permutation_ignoring_timeouts("A failed\n", "A timed-out\n"));
    }

    // A timeout must be ignored even when it changes where the line sorts relative to other lines
    // with the same prefix. See https://github.com/trailofbits/necessist/issues/1978.
    #[test]
    fn permutation_ignores_a_timeout_among_lines_sharing_a_prefix() {
        const EXPECTED: &str = r"$DIR/test/Sync.t.sol: `.executeActions(actions, params)` passed
$DIR/test/Sync.t.sol: `.executeActions(actions, params)` passed
$DIR/test/Sync.t.sol: `.getSyncedReserves()` nonbuildable
$DIR/test/Sync.t.sol: `.executeActions(actions, params)` failed
";
        const ACTUAL: &str = r"$DIR/test/Sync.t.sol: `.executeActions(actions, params)` passed
$DIR/test/Sync.t.sol: `.executeActions(actions, params)` passed
$DIR/test/Sync.t.sol: `.getSyncedReserves()` nonbuildable
$DIR/test/Sync.t.sol: `.executeActions(actions, params)` timed-out
";
        assert!(permutation_ignoring_timeouts(EXPECTED, ACTUAL));
    }

    #[test]
    fn permutation_ignores_multiple_timeouts_for_one_prefix() {
        assert!(permutation_ignoring_timeouts(
            "A passed\nA failed\n",
            "A timed-out\nA timed-out\n"
        ));
    }

    #[test]
    fn permutation_rejects_a_timeout_that_becomes_an_outcome() {
        assert!(!permutation_ignoring_timeouts(
            "A timed-out\n",
            "A failed\n"
        ));
    }

    #[test]
    fn permutation_rejects_a_changed_outcome() {
        assert!(!permutation_ignoring_timeouts("A failed\n", "A passed\n"));
    }

    #[test]
    fn permutation_rejects_a_changed_outcome_among_lines_sharing_a_prefix() {
        assert!(!permutation_ignoring_timeouts(
            "A passed\nA failed\n",
            "A passed\nA passed\n"
        ));
    }

    #[test]
    fn permutation_rejects_differing_prefixes() {
        assert!(!permutation_ignoring_timeouts(
            "A failed\n",
            "B timed-out\n"
        ));
    }

    #[test]
    fn compare_summaries_matches_identical_summaries() {
        let summary = Summary::new(&[("passed", 2), ("failed", 1)], 1);
        assert!(compare_summaries(&summary, &summary));
    }

    #[test]
    fn compare_summaries_ignores_a_timeout() {
        let expected = Summary::new(&[("passed", 1), ("failed", 1)], 0);
        let actual = Summary::new(&[("passed", 1)], 1);
        assert!(compare_summaries(&expected, &actual));
    }

    #[test]
    fn compare_summaries_rejects_a_timeout_that_is_unaccounted_for() {
        let expected = Summary::new(&[("passed", 1), ("failed", 1)], 0);
        let actual = Summary::new(&[("passed", 1), ("failed", 1)], 1);
        assert!(!compare_summaries(&expected, &actual));
    }

    // `actual` has a suffix that `expected` does not. Without the first loop in
    // `compare_summaries`, the suffix is never examined and the summaries compare equal.
    #[test]
    fn compare_summaries_rejects_a_suffix_that_expected_does_not_have() {
        let expected = Summary::new(&[("passed", 1)], 0);
        let actual = Summary::new(&[("passed", 1), ("failed", 1)], 0);
        assert!(!compare_summaries(&expected, &actual));
    }

    // As above, but `actual` merely has more lines with a suffix than `expected` does.
    #[test]
    fn compare_summaries_rejects_a_surplus_of_a_suffix() {
        let expected = Summary::new(&[("passed", 1)], 0);
        let actual = Summary::new(&[("passed", 2)], 0);
        assert!(!compare_summaries(&expected, &actual));
    }
}
