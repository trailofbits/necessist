#[test]
fn other_file_test() {
    let mut n = 0;
    n += 1;
    other_file_noop();
}

#[allow(dead_code)]
fn other_file_noop() {}
