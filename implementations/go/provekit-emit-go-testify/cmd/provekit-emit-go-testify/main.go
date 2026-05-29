package main

import (
	"fmt"
	"os"

	emitgotestify "github.com/tsavo/provekit/go/provekit-emit-go-testify"
)

func main() {
	if len(os.Args) > 1 && os.Args[1] == "--rpc" {
		if err := emitgotestify.RunRPC(os.Stdin, os.Stdout); err != nil {
			fmt.Fprintf(os.Stderr, "provekit-emit-go-testify rpc: %v\n", err)
			os.Exit(1)
		}
		return
	}
	fmt.Fprintln(os.Stderr, "usage: provekit-emit-go-testify --rpc")
	os.Exit(1)
}
