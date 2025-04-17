#[test]
fn adjacent_test() {
    let mut n = 0;
    n += 1;
    adjacent_noop();
}

#[allow(dead_code)]
fn adjacent_noop() {}

#[test]
fn unused_adjacent_test() {
    let value = unused_adjacent_expression();
    assert!(value);
}

#[allow(dead_code)]
fn unused_adjacent_expression() -> bool {
    true
} 