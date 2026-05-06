// Forward-propagation floor fixture for Zig
// Tests: (1) callsite satisfies pre, no diagnostic | (2) callsite violates pre, diagnostic | (3) loop path, top fallback

fn checkPositive(x: i32) bool {
    if (x <= 0) return false; // pre: x > 0
    return true;
}

fn callerSatisfiesPre() bool {
    const result = checkPositive(5); // satisfies pre (x=5 > 0)
    return result;
}

fn callerViolatesPre() bool {
    const result = checkPositive(-1); // violates pre (x=-1 <= 0)
    return result;
}

fn callerWithLoop() bool {
    var i: i32 = 0;
    while (i < 10) : (i += 1) {
        const result = checkPositive(i); // top fallback at loop entry
        if (!result) return false;
    }
    return true;
}
