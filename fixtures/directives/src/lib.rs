#[test]
fn directives() {
    let mut n = 0;
    // necessist: skip
    n += 1;
    //necessist:skip
    n += 2;
    // necessist: skip, reason for skipping
    n += 3;
    n += 4;
    // necessist: skip

    n += 5;
    // necessist: invalid
    n += 6;
    n += 7; // necessist: skip
    n += 8;
    // necessist: skip-file, too late
    n += 9;
    //necessist:skip-file, still too late
    n += 10;
    assert_eq!(55, n);
}
