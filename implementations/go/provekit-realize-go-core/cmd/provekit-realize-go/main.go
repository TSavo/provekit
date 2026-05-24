package main

import (
	"fmt"
	"os"

	realizego "github.com/tsavo/provekit/go/provekit-realize-go-core"
)

func main() {
	if len(os.Args) > 1 && os.Args[1] == "--rpc" {
		if err := realizego.RunRPC(os.Stdin, os.Stdout); err != nil {
			fmt.Fprintf(os.Stderr, "provekit-realize-go rpc: %v\n", err)
			os.Exit(1)
		}
		return
	}
	fmt.Fprintln(os.Stderr, "usage: provekit-realize-go --rpc")
	os.Exit(1)
}
