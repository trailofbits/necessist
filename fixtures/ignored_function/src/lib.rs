fn ignored_function<T>(_: T) {}

fn foo() -> u32 {
    1
}

#[test]
fn test() {
    let mut n = 0;
    n += 1;
    ignored_function(foo());
    assert!(n > 0);
}
