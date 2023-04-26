#[test]
fn passed() {
    let mut n = 0;
    n += 1;
    noop();
}

fn noop() {}

#[test]
fn timed_out() {
    let mut n = 0;
    while n < 1 {
        n += 1;
    }
}

#[test]
fn failed() {
    let mut n = 0;
    n += 1;
    assert!(n >= 1);
}

#[test]
fn nonbuildable() {
    let _ = |xs: &[&str]| -> String {
        return xs.join("");
    };
}
