package main

import (
	"fmt"
	"os"

	liftgo "github.com/tsavo/provekit/go/provekit-lift-go"
)

func main() {
	if len(os.Args) > 1 && os.Args[1] == "--rpc" {
		if err := liftgo.RunRPC(os.Stdin, os.Stdout); err != nil {
			fmt.Fprintf(os.Stderr, "provekit-lift-go-source rpc: %v\n", err)
			os.Exit(1)
		}
		return
	}
	fmt.Fprintln(os.Stderr, "usage: provekit-lift-go --rpc")
	os.Exit(1)
}
