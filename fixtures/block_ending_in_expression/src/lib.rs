#[test]
fn foo() {
    positive(|| {
        let mut n = 0;
        n += 1;
        n
    });
    noop();
}

fn positive(f: impl FnOnce() -> i32) {
    assert!(f() > 0);
}

fn noop() {}
