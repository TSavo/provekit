package main

import (
	"fmt"
	"os"

	emitgotesting "github.com/tsavo/provekit/go/provekit-emit-go-testing"
)

func main() {
	if len(os.Args) > 1 && os.Args[1] == "--rpc" {
		if err := emitgotesting.RunRPC(os.Stdin, os.Stdout); err != nil {
			fmt.Fprintf(os.Stderr, "provekit-emit-go-testing rpc: %v\n", err)
			os.Exit(1)
		}
		return
	}
	fmt.Fprintln(os.Stderr, "usage: provekit-emit-go-testing --rpc")
	os.Exit(1)
}
