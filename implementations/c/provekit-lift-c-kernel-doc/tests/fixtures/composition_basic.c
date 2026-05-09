/*
 * Composition pass fixture, per Contract Composition Protocol (CCP)
 * v1.0.0 §4 (eager materialization) and the C lifter's composition
 * pass at src/composition.c.
 *
 * Three pure helpers chained via direct (non-indirect) calls. The
 * c-effects extractor (effects.c) tags each helper with an empty
 * effect set:
 *
 *   - no member access (no MemberRefExpr / ArraySubscriptExpr)
 *   - no assignment to a global / out-parameter
 *   - no calls into the Io / Panic allowlists
 *   - no function-pointer dispatch (no UnresolvedCall)
 *   - no inline asm
 *   - no non-void pointer casts
 *
 * Pure-chain detection in composition.c follows the call_sites graph
 * leaf-first, so the chain rooted at compose_three is
 * [double_it, add_one, compose_three]; the chain rooted at compose_two
 * is [double_it, compose_two]; double_it itself has no callees and
 * therefore yields no composed contract.
 *
 * Anything in this file MUST stay pure under the c-effects walker, or
 * the integration assertion that a composed-contract declaration
 * appears will start failing.
 *
 * Each pure helper carries a kernel-doc precondition so the C lifter's
 * composition pass derives a real FunctionContractMemento body from
 * the kernel-doc index per CCP §2 + §9 (instead of falling back to
 * the identity-shape body). The composed CID for compose_three thus
 * reflects real semantic content, not just the structural shape of
 * the chain. Removing or weakening the kernel-doc here will flip the
 * pinned compose_three CID in tests/integration.sh.
 */

/**
 * double_it - return twice x
 * @x: must be positive.
 */
int double_it(int x) {
    return x + x;
}

/**
 * add_one - return double_it(x) + 1
 * @x: must be positive.
 */
int add_one(int x) {
    return double_it(x) + 1;
}

/**
 * compose_three - return add_one(x)
 * @x: must be positive.
 */
int compose_three(int x) {
    return add_one(x);
}

/**
 * compose_two - return double_it(x)
 * @x: must be positive.
 */
int compose_two(int x) {
    return double_it(x);
}
