import { contract, num, Geq, Var, Add } from "@provekit/ir";

// Circular Proof Demo — TypeScript Caller Contract
// 
// Contract: processValue(input: number) → number
//   Pre:  input ≥ 0
//   Post: output ≥ input
// 
// This function bridges to C++ multiply2x.
// The bridge is: output = multiply2x(input)
// C++ contract guarantees: multiply2x(x) = 2*x

contract("processValue", {
  pre: Geq(Var("input", "Int"), num(0)),
  post: Geq(Var("out", "Int"), Var("input", "Int")),
  outBinding: "out"
});
