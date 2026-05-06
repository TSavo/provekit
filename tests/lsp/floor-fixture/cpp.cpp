// Forward-propagation floor fixture for C++
// Tests: (1) callsite satisfies pre, no diagnostic | (2) callsite violates pre, diagnostic | (3) loop path, top fallback

bool checkPositive(int x) {
    if (x <= 0) { return false; }  // pre: x > 0
    return true;
}

bool callerSatisfiesPre() {
    bool result = checkPositive(5);  // satisfies pre (x=5 > 0)
    return result;
}

bool callerViolatesPre() {
    bool result = checkPositive(-1);  // violates pre (x=-1 <= 0)
    return result;
}

bool callerWithLoop() {
    for (int i = 0; i < 10; i++) {
        bool result = checkPositive(i);  // top fallback at loop entry
        if (!result) { return false; }
    }
    return true;
}