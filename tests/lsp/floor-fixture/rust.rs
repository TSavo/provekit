// Forward-propagation floor fixture for Rust
// Tests: (1) callsite satisfies pre, no diagnostic | (2) callsite violates pre, diagnostic | (3) loop path, top fallback

#[allow(non_snake_case)]
fn checkPositive(x: i32) -> bool {
    if x <= 0 {
        return false; // pre: x > 0
    }
    true
}

#[allow(non_snake_case)]
fn callerSatisfiesPre() -> bool {
    let result = checkPositive(5); // satisfies pre (x=5 > 0)
    result
}

#[allow(non_snake_case)]
fn callerViolatesPre() -> bool {
    let result = checkPositive(-1); // violates pre (x=-1 <= 0)
    result
}

#[allow(non_snake_case)]
fn callerWithLoop() -> bool {
    for i in 0..10 {
        let result = checkPositive(i); // top fallback at loop entry
        if !result {
            return false;
        }
    }
    true
}
