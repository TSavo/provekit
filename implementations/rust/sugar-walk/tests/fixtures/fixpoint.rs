// Fixture for Task C (fix-point lift): outer calls inner, but if the
// registry is built without a fix-point loop, outer may lift without
// seeing inner's contract if the declaration order is reversed in the
// LLBC. In practice, Charon preserves declaration order, so this
// fixture is structurally the same as calls.rs — the fix-point test
// validates that a second pass produces stable CIDs.

fn inner(y: u32) -> u32 {
    if y < 3 {
        panic!();
    }
    y
}

fn outer(x: u32) -> u32 {
    inner(x)
}
