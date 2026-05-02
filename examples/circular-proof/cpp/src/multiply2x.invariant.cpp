// Circular Proof Demo — C++ multiply2x Contract
//
// Contract: multiply2x(x: int32) → int32
//   Post: output = 2 * input
//
// This bridges to Go addThree.

#include "provekit/ir.hpp"

using namespace provekit::ir;

void define_contracts() {
    // multiply2x contract
    // Post: out = 2 * x
    contract("multiply2x", ContractArgs{
        .post = Eq(Var("out", Int), Mul(num(2), Var("x", Int))),
        .out_binding = "out"
    });
}
