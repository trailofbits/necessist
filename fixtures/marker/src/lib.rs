// The test deletes `sentinel`, so the file survives only if the test never runs.
#[test]
fn marks() {
    let _ = std::fs::remove_file("sentinel");
    let mut n = 0;
    n += 1;
    assert!(n > 0);
}
