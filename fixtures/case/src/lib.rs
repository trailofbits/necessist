#[test]
fn cased() {
    let mut s = String::new();
    s.push_str("ab");
    s.push_str("cd");
    assert!(!s.is_empty());
}
