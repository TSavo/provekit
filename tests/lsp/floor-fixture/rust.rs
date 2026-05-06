fn check_positive(x: i32) -> bool {
    if x <= 0 { return false; }
    true
}

fn caller_satisfies_pre() -> bool {
    check_positive(5)
}

fn caller_violates_pre() -> bool {
    check_positive(-1)
}
