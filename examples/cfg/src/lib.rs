#[cfg(feature = "passed")]
#[test]
fn passed() {
    let mut n = 0;
    n += 1;
}

#[cfg(feature = "timed-out")]
#[test]
fn timed_out() {
    let mut n = 0;
    while n < 1 {
        n += 1;
    }
}

#[cfg(feature = "failed")]
#[test]
fn failed() {
    let mut n = 0;
    n += 1;
    assert!(n >= 1);
}

#[cfg(feature = "nonbuildable")]
#[test]
fn nonbuildable() {
    let _ = |xs: &[&str]| -> String {
        return xs.join("");
    };
}
