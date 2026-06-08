// Package sample is the Go library whose contract Sugar extracts and
// verifies end-to-end. `Double` is a body-bearing function: the Go lifter
// lifts its body `x * 2` to the verify-facing function-contract
// `post = result == (* x 2)`. `sugar mint` auto-writes the
// `Double -> targetContractCid` bridge (#1443); `sugar verify` reduces the
// harvested `Double(3) == 6` assertion through the body via z3.
package sample

// Double returns twice its argument. The honest body is `x * 2`; the
// integration test's negative case mutates it to `x * 3` to prove the spine
// catches a real violation (Unsatisfied, exit 1, no witness).
func Double(x int) int {
	return x * 2
}
