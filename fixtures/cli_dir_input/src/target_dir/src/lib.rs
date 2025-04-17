#[test]
fn target_test() {
    let mut n = 0;
    n += 1;
    target_noop();
}

#[allow(dead_code)]
fn target_noop() {}

#[test]
fn unused_test() {
    let value = unused_expression();
    assert!(value);
}

#[allow(dead_code)]
fn unused_expression() -> bool {
    true
} 