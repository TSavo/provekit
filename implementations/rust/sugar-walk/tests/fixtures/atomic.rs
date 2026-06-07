// Fixture: function that calls AtomicU32::fetch_add.
// Charon emits the callee's fun_decl with a path of
// [core, sync, atomic, AtomicU32, fetch_add].
// The lifter must emit Effect::AtomicAccess { kind: Rmw, ... }.
use std::sync::atomic::{AtomicU32, Ordering};

fn bump(counter: &AtomicU32) -> u32 {
    counter.fetch_add(1, Ordering::SeqCst)
}
