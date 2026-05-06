// Forward-propagation floor fixture for Rust
// Tests: (1) callsite satisfies pre, no diagnostic | (2) callsite violates pre, diagnostic | (3) loop path, top fallback

fn check_positive(x: i32) -> bool {
    if x <= 0 { return false; }  // pre: x > 0
    true
}

fn caller_satisfies_pre() -> bool {
    let result = check_positive(5);  // satisfies pre (x=5 > 0)
    result
}

fn caller_violates_pre() -> bool {
    let result = check_positive(-1);  // violates pre (x=-1 <= 0)
    result
}

fn caller_with_loop() -> bool {
    for i in 0..10 {
        let result = check_positive(i);  // top fallback at loop entry
        if !result { return false; }
    }
    true
}