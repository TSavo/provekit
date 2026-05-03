// rust-callee/src/lib.rs — SUCCESS FIXTURE
//
// Same rust callee as fixture-fail. Exposes `process(n: i32) -> i32` via C FFI.
//
// Contract: pre = (n > 0).
//
// In the success fixture, go-caller/caller_ok.go does NOT call C.process at all.
// The Go lifter emits no cgo call-edges. The linker produces zero cross-kit
// bridges and zero linker-errors. The link bundle is clean.
//
// Run: provekit link examples/polyglot-rust-go/fixture-ok/
// Expected: link-bundle.json with 0 linker-errors, exit code 0.

#[requires(n > 0)]
#[no_mangle]
pub extern "C" fn process(n: i32) -> i32 {
    n * 2
}
