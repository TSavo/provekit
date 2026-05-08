// Forward-propagation floor fixture for Go
// Tests: (1) callsite satisfies pre, no diagnostic | (2) callsite violates pre, diagnostic | (3) loop path, top fallback

package floorfixture

func checkPositive(x int) bool {
	if x <= 0 {
		return false // pre: x > 0
	}
	return true
}

func callerSatisfiesPre() bool {
	result := checkPositive(5) // satisfies pre (x=5 > 0)
	return result
}

func callerViolatesPre() bool {
	result := checkPositive(-1) // violates pre (x=-1 <= 0)
	return result
}

func callerWithLoop() bool {
	for i := 0; i < 10; i++ {
		result := checkPositive(i) // top fallback at loop entry
		if !result {
			return false
		}
	}
	return true
}
