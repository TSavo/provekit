// rust-callee/src/lib.rs: SUCCESS FIXTURE
//
// Same rust callee as fixture-fail. Exposes `process(n: i32) -> i32` via C FFI.
//
// Contract: pre = (n > 0).
//
// In the success fixture, go-caller/caller_ok.go still calls C.process.
// The Go lifter emits the same cgo call-edge as the failure fixture, and the
// link bundle stays clean because the caller post-condition establishes n > 0.
//
// Run: provekit link examples/polyglot-rust-go/fixture-ok/
// Expected: link-bundle.json with 1 bridge, 0 linker-errors, exit code 0.

#[requires(n > 0)]
#[no_mangle]
pub extern "C" fn process(n: i32) -> i32 {
    n * 2
}
