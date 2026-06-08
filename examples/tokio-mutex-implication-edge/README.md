# Tokio mutex implication edge

This showcase proves one narrow async effect edge for `tokio::sync::Mutex`:
the protected data invariant established at `Mutex::new(producer().await)`
discharges the precondition of the critical-section consumer at
`consumer(*m.lock().await)`. The call result is bound inside an inner block
before returning, so the `MutexGuard` temporary is dropped before the local
mutex while the lifted term still contains the critical-section consumer call.

The lock is treated only as a typed conduit for the protected value. The
receipt does not prove lock release, guard `Drop`, RAII, deadlock freedom,
lock ordering, acquisition cardinality, interleaving, data-race freedom,
`Send`/`Sync`, or Rust type/borrow/drop facts. Those are either compiler
legality facts for compiling programs or outside this proof lane.

Both twins compile and type-check. The bad twin fails only because the
protected data invariant is too weak for the critical-section precondition.
