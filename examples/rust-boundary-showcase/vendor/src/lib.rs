// The vendor's REAL source — PLAIN rust, no sugar attribute of any kind.
// `reverse_chars` is an ordinary `pub fn`. In the `library-bindings` layer the
// lift DERIVES the binding from the crate name + fn name (no
// `#[sugar::sugar]` required): write a function, it's sugar. Lifting it with
// SUGAR_LEAN_SOURCE=1 mints a LEAN binding (locus + source_cid/template_cid,
// NO inline body). The body lives ONLY here on disk; the Source Oracle resolves
// it on demand IFF this source recomputes to the pinned CIDs.
//
// run.sh's DRIFT leg tampers this body after the mint — the pin then no longer
// matches disk, and materialize REFUSES. Keep this body in sync with run.sh's
// restore block (it rewrites this file before each run).

pub fn reverse_chars(s: &str) -> String {
    s.chars().rev().collect()
}
