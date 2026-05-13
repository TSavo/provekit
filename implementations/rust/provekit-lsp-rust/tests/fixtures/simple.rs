fn documented_value() -> i64 {
    42
}

#[test]
fn value_is_non_negative() {
    let x: i64 = documented_value();
    assert!(x >= 0);
}
