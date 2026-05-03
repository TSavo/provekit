// caller_ok.go — SUCCESS CASE
//
// This Go file does NOT call C.process at all; the contract-annotated
// function performs the operation in pure Go.
//
// The Go lifter emits no cgo call-edges for GoCallerOk.
// The linker produces zero cross-kit bridges and zero linker-errors.
// The link bundle is clean.
//
// Run: provekit link examples/polyglot-rust-go/fixture-ok/
// Expected: link-bundle.json with 0 linker-errors, exit code 0.
package caller

//provekit:contract
func GoCallerOk(n int) int {
	if n <= 0 {
		return 0
	}
	return n * 2
}
