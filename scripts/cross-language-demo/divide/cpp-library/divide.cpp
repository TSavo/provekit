// One C++ library. Four language consumers (TS, Rust, Go, C++).
// One propertyHash. Cross-language DAG composition.
//
// This file is illustrative. The library function is what every
// consumer ultimately calls: via N-API for TS, via FFI for Rust,
// via cgo for Go, directly for C++.

#include <stdexcept>

extern "C" {

// Native source contract surface:
// - the guard establishes d != 0 before division
// - the return expression is the postcondition the C++ source lifter projects
int divide(int n, int d) {
    if (d == 0) {
        throw std::runtime_error("denominator must not be zero");
    }
    return n / d;
}

}  // extern "C"
