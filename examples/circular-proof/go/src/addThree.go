// Circular Proof Demo — Go addThree
//
// This Go module exposes addThree via cgo.
// It calls back into TypeScript via Node-API for finalization.
//
// Contract: addThree(x: int32) → int32
//   Post: output = input + 3

package main

import "C"

// addThree adds 3 to the input.
// Contract: out = x + 3
//
//export addThree
func addThree(x C.int32_t) C.int32_t {
	// Bridge to TypeScript: finalizeValue(x + 3)
	// TypeScript guarantees: finalizeValue(z) = z * 2
	return C.int32_t(int32(x) + 3)
}

func main() {}
