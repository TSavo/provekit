// Circular Proof Demo — C++ Callee
//
// This C++ module exposes multiply2x via WASM/FFI.
// It calls a Go function via cgo for the final computation.
//
// Contract: multiply2x(x) → 2*x
//   Post: output = 2 * input

#include <cstdint>

// Forward declaration of Go function (via cgo)
extern "C" int32_t addThree(int32_t x);

// multiply2x: doubles the input, then calls Go addThree
// Contract: output = 2 * input
extern "C" int32_t multiply2x(int32_t x) {
    int32_t doubled = x * 2;
    // Bridge to Go: addThree(doubled)
    // Go guarantees: addThree(y) = y + 3
    return addThree(doubled);
}
