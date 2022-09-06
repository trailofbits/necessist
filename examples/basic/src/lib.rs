#[test]
pub fn passed() {
    #[allow(unused_variables, unused_assignments)]
    (|mut n: u32| {
        n += 1;
    })(0);
}

#[test]
pub fn timed_out() {
    (|mut n: u32| {
        while n < 1 {
            n += 1;
        }
    })(0);
}

#[test]
pub fn failed() {
    (|mut n: u32| {
        n += 1;
        assert!(n >= 1);
    })(0);
}

#[test]
pub fn nonbuildable() {
    let _ = || -> u32 {
        return 0;
    };
}

#[test]
pub fn skipped() {
    assert!(true);
}

#[test]
pub fn inconclusive() {
    &|| ();
}
