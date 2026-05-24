package main

import (
	"fmt"
	"os"

	liftgo "github.com/tsavo/provekit/go/provekit-lift-go"
)

func main() {
	rpc := false
	defaultOpts := liftgo.LiftOptions{}
	for _, arg := range os.Args[1:] {
		switch arg {
		case "--rpc":
			rpc = true
		case "--dialect=core":
			// Verify-facing dialect: emit SMT-LIB core op symbols so the
			// body-derived postcondition discharges through the z3 verifier.
			// The kit-dispatch `go` lift surface launches this binary with
			// this flag (see implementations/go/.provekit/lift/go/manifest.toml).
			defaultOpts.NormalizeCoreArith = true
		}
	}
	if rpc {
		if err := liftgo.RunRPCWithDefault(os.Stdin, os.Stdout, defaultOpts); err != nil {
			fmt.Fprintf(os.Stderr, "provekit-lift-go-source rpc: %v\n", err)
			os.Exit(1)
		}
		return
	}
	fmt.Fprintln(os.Stderr, "usage: provekit-lift-go --rpc [--dialect=core]")
	os.Exit(1)
}
