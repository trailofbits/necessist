#[necessist::necessist]
#[test]
pub fn passed() {
    #[allow(unused_variables, unused_assignments)]
    (|mut n: u32| {
        n += 1;
    })(0);
}

#[necessist::necessist]
#[test]
pub fn timed_out() {
    (|mut n: u32| {
        while n < 1 {
            n += 1;
        }
    })(0);
}

#[necessist::necessist]
#[test]
pub fn failed() {
    (|mut n: u32| {
        n += 1;
        assert!(n >= 1);
    })(0);
}

#[necessist::necessist]
#[test]
pub fn nonbuildable() {
    let _ = || -> u32 {
        return 0;
    };
}

#[necessist::necessist]
#[test]
pub fn skipped() {
    assert!(true);
}

#[necessist::necessist]
#[test]
pub fn inconclusive() {
    &|| ();
}
