#[test]
fn invalid_utf8() {
    use std::io::{stdout, Write};

    #[allow(unused_assignments)]
    let mut write = true;

    write = false;

    if write {
        stdout().write(&[0xfe, b'\n']).unwrap();
    }
}
