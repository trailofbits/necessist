#[test]
fn dry_run_failed() {
    let mut n = 0;
    n += 1;
    let _ = ["x"].join("");
    assert!(n >= 2);
}
