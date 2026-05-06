// Forward-propagation floor fixture for Swift
// Tests: (1) callsite satisfies pre, no diagnostic | (2) callsite violates pre, diagnostic | (3) loop path, top fallback

func checkPositive(_ x: Int) -> Bool {
    if x <= 0 { return false }  // pre: x > 0
    return true
}

func callerSatisfiesPre() -> Bool {
    let result = checkPositive(5)  // satisfies pre (x=5 > 0)
    return result
}

func callerViolatesPre() -> Bool {
    let result = checkPositive(-1)  // violates pre (x=-1 <= 0)
    return result
}

func callerWithLoop() -> Bool {
    for i in 0..<10 {
        let result = checkPositive(i)  // top fallback at loop entry
        if !result { return false }
    }
    return true
}