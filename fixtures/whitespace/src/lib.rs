#[test]
fn multiline() {
    let mut v = Vec::new();
    v.extend(
	[1, 2],
    );
    v.push(
        3,
    );
    assert_eq!(3, v.len());
}
