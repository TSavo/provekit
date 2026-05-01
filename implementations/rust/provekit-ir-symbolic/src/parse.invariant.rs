// SPDX-License-Identifier: Apache-2.0
//
// .invariant.rs for provekit-ir-symbolic/src/parse.rs
//
// Public surface covered:
//   * `parse_formula(&serde_json::Value) -> Result<Rc<Formula>, ParseError>`
//   * `parse_term(&serde_json::Value) -> Result<Rc<Term>, ParseError>`
//   * `ParseError` variants (Mismatch / MissingField / ExtraKey /
//     UnknownKind / Arity / EmptyContract / InvalidJson)
//
// HISTORICAL NOTE:
//   This file holds the seven contracts originally authored in
//   `provekit-self-contracts/src/lib.rs::author_self_contracts()`.
//   They have been relocated here per the per-source-file
//   `.invariant.rs` convention (every public-API source file owns its
//   contracts). The orchestrator binary `mint-self-contracts` calls
//   `invariants()` and collects the same `ContractDecl` values that
//   were collected before; the original closed-loop bridge
//   (`parse_formula -> parse_formula_correct`) is registered separately
//   in the orchestrator.
//
// Honest scope:
//   The IR's standard algebra (=, <, >) reaches Z3 cleanly. Parser
//   correctness predicates (`roundTrips`, `isErr`, `isMalformed`) are
//   kit-defined names with no Z3 semantics; their callsite verdicts
//   resolve to undecidable, which is the protocol's HONEST outcome.
//   The operational enforcement of those invariants lives in the
//   proptest block in `provekit-self-contracts/src/lib.rs`.

use std::rc::Rc;

use provekit_ir_symbolic::{
    and_, atomic_, contract, eq, forall, gt, lt, not_, num, ContractArgs, Int,
    String_, Term,
};

fn ctor1(name: &str, arg: Rc<Term>) -> Rc<Term> {
    Rc::new(Term::Ctor {
        name: name.into(),
        args: vec![arg],
    })
}

pub fn invariants() {
    // -----------------------------------------------------------------
    // CONTRACT 1: parse_formula determinism (CLOSED LOOP).
    //
    // forall x: String. parse_formula(x) = parse_formula(x)
    //
    // This contract contains a `parse_formula` ctor reference, so the
    // bridge keyed on `parse_formula` (registered in the orchestrator)
    // makes this contract enumerate as a callsite, with the bridge
    // resolving to "parse_formula_correct" (contract 2). The verifier
    // walks load -> enumerate -> resolve (succeeds) -> instantiate ->
    // smt-emit -> Z3.
    // -----------------------------------------------------------------
    contract(
        "parse_formula_determinism",
        ContractArgs {
            post: Some(forall(String_(), |x| {
                eq(ctor1("parse_formula", x.clone()), ctor1("parse_formula", x))
            })),
            ..Default::default()
        },
    );

    // -----------------------------------------------------------------
    // CONTRACT 2: parse_formula correctness (ROUND-TRIP, BRIDGE TARGET).
    //
    // The bridge resolves to THIS contract memento. Its `pre` is what
    // the verifier substitutes the call-site arg into. Uses
    // kit-defined `roundTrips` atomic; Z3 returns "undecidable" — the
    // honest outcome documented in the report. The IR expresses the
    // intent; a future Tier-1 (per-prop fact) layer would discharge it.
    // -----------------------------------------------------------------
    contract(
        "parse_formula_correct",
        ContractArgs {
            pre: Some(forall(String_(), |x| atomic_("roundTrips", vec![x]))),
            ..Default::default()
        },
    );

    // -----------------------------------------------------------------
    // CONTRACT 3: parse rejects malformed input.
    //
    // For any input failing the closed-object policy, the parser
    // returns an error. Expressed with kit-defined `isMalformed` and
    // `isErr` atomic predicates.
    // -----------------------------------------------------------------
    contract(
        "parse_rejects_malformed",
        ContractArgs {
            post: Some(forall(String_(), |x| {
                let malformed = atomic_("isMalformed", vec![x.clone()]);
                let parse_err = atomic_("isErr", vec![ctor1("parse_formula", x)]);
                provekit_ir_symbolic::implies(malformed, parse_err)
            })),
            ..Default::default()
        },
    );

    // -----------------------------------------------------------------
    // CONTRACT 4: BLAKE3-512 output length (cross-crate reach).
    //
    // BLAKE3-512 produces a 64-byte digest, emitted as 128 lowercase hex
    // chars under the "blake3-512:" tag, so the full self-identifying
    // CID has length exactly 139. Expressed with `len` as the
    // kit-defined ctor returning Int; result `=` 139.
    // -----------------------------------------------------------------
    contract(
        "compute_cid_length",
        ContractArgs {
            post: Some(forall(String_(), |x| {
                eq(ctor1("len", ctor1("compute_cid", x)), num(139))
            })),
            ..Default::default()
        },
    );

    // -----------------------------------------------------------------
    // CONTRACT 5: arity claim about parse_formula's `not` operand.
    //
    // A valid `not` formula has exactly one operand:
    //   forall n: Int. n = 1.
    // Falsifiable (n = 0, n = 7, ...): Z3 should return SAT, the
    // verifier maps to "unsatisfied". That's the negative case
    // landing through the pipeline cleanly.
    //
    // Avoid `>=`/`<=`/`!=` predicates: the JCS encoder mishandles
    // non-ASCII bytes (round-trip mangles UTF-8) which breaks Rule 2
    // envelope-CID re-derivation in the verifier. ASCII-only.
    // -----------------------------------------------------------------
    contract(
        "not_arity_eq_one",
        ContractArgs {
            post: Some(forall(Int(), |n| eq(n, num(1)))),
            ..Default::default()
        },
    );

    // -----------------------------------------------------------------
    // CONTRACT 6: implies-arity claim — disjunction over n=2.
    //
    // forall n: Int. NOT (n < 2) AND NOT (n > 2)
    // (i.e., n = 2 expressed via ASCII-only predicates < and >).
    //
    // FALSIFIABLE (any n != 2). Z3 returns SAT -> "unsatisfied".
    // -----------------------------------------------------------------
    contract(
        "implies_arity_eq_two",
        ContractArgs {
            post: Some(forall(Int(), |n| {
                and_(vec![not_(lt(n.clone(), num(2))), not_(gt(n, num(2)))])
            })),
            ..Default::default()
        },
    );

    // -----------------------------------------------------------------
    // CONTRACT 7: serialize is total (a function).
    //
    // forall f: Int. serialize(f) = serialize(f). Trivially true; Z3
    // expected to discharge.
    // -----------------------------------------------------------------
    contract(
        "serialize_is_a_function",
        ContractArgs {
            post: Some(forall(Int(), |f| {
                eq(ctor1("serialize", f.clone()), ctor1("serialize", f))
            })),
            ..Default::default()
        },
    );
}
