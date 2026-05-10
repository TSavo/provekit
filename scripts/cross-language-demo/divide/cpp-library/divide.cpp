// One C++ library. Four language consumers (TS, Rust, Go, C++).
// One propertyHash. Cross-language DAG composition.
//
// This file is illustrative. The library function is what every
// consumer ultimately calls: via N-API for TS, via FFI for Rust,
// via cgo for Go, directly for C++.

#include <stdexcept>

extern "C" {

// The function under contract.
// Precondition: d != 0
// Postcondition: returns n / d (integer division)
int divide(int n, int d) {
    if (d == 0) {
        throw std::runtime_error("denominator must not be zero");
    }
    return n / d;
}

}  // extern "C"
