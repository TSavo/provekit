// caller_ok.go - SUCCESS CASE
//
// This Go file still calls C.process. The caller establishes n > 0 before
// crossing the cgo boundary, and the contract records that postcondition.
//
// The Go lifter emits a cgo call-edge from GoCallerOk to rust-kit:process.
// The linker derives the same bridge shape as the failure fixture, then
// discharges post_caller => pre_callee because both sides are n > 0.
//
// Expected checked-in receipt: link-bundle.json with 1 bridge, 0 linker-errors.
package caller

/*
#include "rust_callee.h"
#include <stdint.h>
extern int process(int n);
*/
import "C"

//provekit:contract post=n>0
func GoCallerOk(n int) int {
	n = 1
	return int(C.process(C.int(n)))
}
