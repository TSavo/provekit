import { contract, num, Eq, Var, Mul } from "@provekit/ir";

// Circular Proof Demo — TypeScript Final Callee Contract
//
// Contract: finalizeValue(z: number) → number
//   Post: output = z * 2
//
// This is the final node in the circular chain.
// Go calls back into this function.

contract("finalizeValue", {
  post: Eq(Var("out", "Int"), Mul(Var("z", "Int"), num(2))),
  outBinding: "out"
});
