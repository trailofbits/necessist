// Keep these cases in sync with the directive unit tests in `backends/src/directives.rs`.

#[test]
fn directives() {
    let mut n = 0;
    // necessist: skip
    n += 1;
    //necessist:skip
    n += 2;
    // necessist: skip, reason for skipping
    n += 3;
    // necessist: skip

    n += 4;
    n += 5; // necessist: skip
    n += 6;
    // necessist: skip-filex
    n += 7;
    // necessist: skip-file, too late 😞
    n += 8;
    assert_eq!(36, n);
}

#[test]
fn skip_method_call() {
    let mut xs = Vec::new();
    xs
        // necessist: skip
        .push(0);
    assert_eq!(vec![0], xs);
}

// Keep the below directive on the last line to ensure a trailing `necessist: skip` does not cause
// problems.
// necessist: skip
