// caller_fail.go - FAILURE CASE
//
// This Go file calls C.process(-1) without any guard.
// The Go lifter emits a cgo call-edge from GoCallerFail to rust-kit:process.
// The linker finds no post-condition on GoCallerFail, and emits:
//
//	kind: "linker-error", errorKind: "unprovable-obligation"
//
// Expected checked-in receipt: link-bundle.json with 1 linker-error.
package caller

/*
#include "rust_callee.h"
#include <stdint.h>
extern int process(int n);
*/
import "C"

//provekit:contract
func GoCallerFail(n int) int {
	// BUG: passes n directly without checking n > 0.
	// The rust callee requires n > 0, but we have no post-condition
	// establishing that, so the linker cannot discharge the obligation.
	return int(C.process(C.int(n)))
}
