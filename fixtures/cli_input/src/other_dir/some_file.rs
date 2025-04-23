#[test]
fn other_some_test() {
    let mut n = 0;
    n += 1;
    other_some_noop();
}

#[allow(dead_code)]
fn other_some_noop() {}
