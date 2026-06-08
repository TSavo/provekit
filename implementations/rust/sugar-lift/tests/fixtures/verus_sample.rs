// Fixture: a representative slice of verus annotations. NOT compiled
// (this file has no Cargo target); the lift adapter parses it as text
// via syn and emits structured "skipped" warnings: verus has its own
// syntax-extension layer and v0 does not lift verus contracts.

verus! {
    spec fn nonneg(x: int) -> bool {
        x >= 0
    }

    proof fn lemma_add_nonneg(a: int, b: int)
        requires
            a >= 0,
            b >= 0,
        ensures
            a + b >= 0,
    {
    }

    fn checked_double(x: u32) -> (r: u32)
        requires
            x < 1_000_000,
        ensures
            r == 2 * x,
    {
        x + x
    }
}

verus! {
    fn another_block_item(n: u64) -> (r: u64)
        requires n > 0,
        ensures r >= 1,
    {
        n
    }
}
