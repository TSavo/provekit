/*
 * Formal-position composition fixture, per Contract Composition
 * Protocol (CCP) v1.0.0 §9 Rule 1 (singular formal substitution).
 *
 * Two pure chain-roots that compose the SAME inner function (`inner`)
 * into the SAME outer function (`outer`) at DIFFERENT formal
 * positions. Without position-aware composition the C lifter would
 * synthesise identical wire-format envelopes for both chains and the
 * composed CIDs would collide. With the resolver wired (composition.c
 * `pk_c_compose_resolve_formal_idx_in_args`) the chains differ in
 * step[1].formalIdx and therefore in their composed CID.
 *
 * Required purity: every helper here MUST stay tagged with an empty
 * effect set by effects.c, or composition refuses and the assertion
 * in tests/integration.sh fails noisily. That means:
 *   - no member access (no MemberRefExpr / ArraySubscriptExpr)
 *   - no assignment to a global / out-parameter
 *   - no calls into the Io / Panic allowlists
 *   - no function-pointer dispatch (no UnresolvedCall)
 *   - no inline asm
 *   - no non-void pointer casts
 *
 * inner takes one int and is the leaf in both chains. outer takes
 * two ints and is the composition site that exposes the
 * position-divergence. chain_pos0 and chain_pos1 are the roots; they
 * differ only in WHERE inner(...) appears in the call to outer.
 */

int inner(int a) {
    return a + 1;
}

int outer(int a, int b) {
    return a + b;
}

int chain_pos0(int a, int b) {
    return outer(inner(a), b);
}

int chain_pos1(int a, int b) {
    return outer(a, inner(b));
}
