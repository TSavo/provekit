// rust-callee/src/lib.rs: FAILURE FIXTURE
//
// Exposes `process(n: i32) -> i32` via C FFI.
//
// Contract: pre = (n > 0).
// The `#[requires(n > 0)]` attribute is read by sugar-lift (via syn)
// and lifted by the contracts adapter into a contract memento.
// The file does not need to compile to be lifted; the lifter reads
// source text only.
//
// This is the TARGET of a cgo call from go-caller/caller_fail.go.
// The Sugar linker derives a bridge from the go call-edge to this
// contract, then attempts to discharge `post_caller ⊃ pre_callee`.
// Because caller_fail.go has no post-condition, the linker emits a
// linker-error memento of kind "unprovable-obligation".

#[requires(n > 0)]
#[no_mangle]
pub extern "C" fn process(n: i32) -> i32 {
    n * 2
}
