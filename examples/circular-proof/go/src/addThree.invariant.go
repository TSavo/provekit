package main

import (
	"github.com/provekit/ir-symbolic/ir"
)

// Circular Proof Demo — Go addThree Contract
//
// Contract: addThree(x: int32) → int32
//   Post: output = input + 3
//
// This bridges to TypeScript finalizeValue.

func init() {
	// addThree contract
	// Post: out = x + 3
	ir.Contract("addThree", ir.ContractArgs{
		Post: ir.Eq(
			ir.Var("out", ir.Int),
			ir.Add(ir.Var("x", ir.Int), ir.Num(3)),
		),
		OutBinding: "out",
	})
}
