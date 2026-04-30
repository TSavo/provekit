// Cross-language equivalence runner — Go path.
//
// Usage: go run main.go <fixture-name>
// Emits: compact JSON of the Declaration[] for the named fixture.

package main

import (
	"fmt"
	"os"

	"github.com/provekit/ir-symbolic/ir"
)

func main() {
	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, "usage: cross-lang-runner <fixture-name>")
		os.Exit(2)
	}
	fixture := os.Args[1]

	ir.ResetCollector()
	finish := ir.BeginCollecting()

	switch fixture {
	case "forall_int_gt_zero":
		ir.Property("forall_int_gt_zero", ir.ForAll(ir.Int, func(x ir.IrTerm) ir.IrFormula {
			return ir.Gt(x, ir.Num(0))
		}))
	case "eq_parseint_zero_zero":
		ir.Property("eq_parseint_zero_zero", ir.Eq(ir.ParseInt(ir.StrConst("0")), ir.Num(0)))
	case "forall_string_parseint_gte_zero":
		ir.Property("forall_string_parseint_gte_zero", ir.ForAll(ir.String, func(s ir.IrTerm) ir.IrFormula {
			return ir.Gte(ir.ParseInt(s), ir.Num(0))
		}))
	default:
		fmt.Fprintf(os.Stderr, "unknown fixture: %s\n", fixture)
		os.Exit(2)
	}

	decls := finish()
	out, err := ir.MarshalDeclarations(decls)
	if err != nil {
		fmt.Fprintf(os.Stderr, "marshal: %v\n", err)
		os.Exit(1)
	}
	fmt.Print(string(out))
}
