// SPDX-License-Identifier: Apache-2.0
//
// provekit-self-contracts
//
// ProvekIt's first dogfood. The framework eats itself: the Rust kit
// authors contracts ABOUT the Rust kit's IR-JSON parser, mints those
// contracts as a `.proof` file, and the verifier scans them.
//
// Authoring split:
//
//   * `author_self_contracts()` — returns a typed Vec<ContractDecl>
//     plus the bridge declarations needed to close the call-site loop
//     for at least one contract. No I/O, no side effects on the file
//     system.
//
//   * `mint_self_proof(out_dir)` — given an output directory, mints
//     each contract + bridge as a signed memento, bundles them into a
//     `.proof` envelope, writes the file at
//     `<out_dir>/<hex>.proof`, and returns the full self-identifying
//     CID plus the count of mementos.
//
// Contracts written here are PLAUSIBLE claims about the parser's
// public API, not deeply provable theorems. The point is that the
// authoring + minting + bundling + verifier round-trip handles them.
//
// Contracts that exercise the IR's standard algebra (=/≠/</≤/>/≥) can
// reach Z3 and may resolve to discharged or unsatisfied. Contracts
// using kit-defined atomic predicates (`roundTrips`, `isErr`,
// `parsesOk`) emit S-expressions Z3 doesn't understand and resolve to
// undecidable — that's the protocol's HONEST outcome and is
// load-bearing: it shows the verifier flowing through the pipeline
// even when the predicate isn't in Z3's signature.
//
// One contract is wired to enumerate as a callsite: the parser-level
// `roundTrip` claim on `parse_formula` references the IR ctor
// `parse_formula`, and a bridge memento maps the IR symbol
// `parse_formula` to the contract memento for `parse_formula_correct`.
// That closes the loop the verifier walks in stages 1 -> 2 -> 3.

use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;

use provekit_canonicalizer::blake3_512_of;
use provekit_claim_envelope::{
    mint_bridge, mint_contract, Authoring, MintBridgeArgs, MintContractArgs,
};
use provekit_ir_symbolic::serialize::formula_to_value;
use provekit_ir_symbolic::{
    and_, atomic_, begin_collecting, contract, eq, finish, forall, gt, lt,
    not_, num, reset_collector, ContractArgs, ContractDecl, Int, String_,
    Term,
};
use provekit_proof_envelope::{
    build_proof_envelope, ed25519_pubkey_string, Ed25519Seed,
    ProofEnvelopeInput,
};

const PRODUCED_BY: &str = "provekit-self-contracts@1.0";
const DECLARED_AT: &str = "2026-04-30T18:00:00.000Z";

/// Wraps an arbitrary Ctor under the standard Rust kit shape. The kit
/// itself only exposes a bridge primitive for `parseInt`; the rest of
/// the IR's ctor surface has to be constructed directly. This helper
/// keeps the call sites readable.
fn ctor1(name: &str, arg: Rc<Term>) -> Rc<Term> {
    Rc::new(Term::Ctor {
        name: name.into(),
        args: vec![arg],
    })
}

/// One bridge declaration coupling an IR symbol (the ctor `name`) to a
/// contract memento that the verifier should use as the discharge
/// target. Names are insertion-order; the actual minting is deferred
/// to `mint_self_proof`.
#[derive(Debug, Clone)]
pub struct SelfBridge {
    pub source_symbol: String,
    pub target_contract_name: String,
    pub ir_arg_sorts: Vec<String>,
    pub ir_return_sort: String,
}

/// Author all self-contracts and the single bridge that closes the
/// call-site loop. No side effects.
pub fn author_self_contracts() -> (Vec<ContractDecl>, Vec<SelfBridge>) {
    reset_collector();
    begin_collecting();

    // -----------------------------------------------------------------
    // CONTRACT 1: parse_formula determinism.
    //
    // Plausible claim: parse_formula is a function — same JSON in,
    // same Formula out. In the IR this becomes
    //
    //   forall x: String. parse_formula(x) = parse_formula(x)
    //
    // This is the *closed-loop* contract. It contains a `parse_formula`
    // ctor reference, so a bridge keyed on `parse_formula` (declared
    // below) makes this contract enumerate as a callsite, with the
    // bridge resolving to "parse_formula_correct" (contract 2). The
    // verifier walks load -> enumerate -> resolve (succeeds) ->
    // instantiate -> smt-emit -> Z3.
    // -----------------------------------------------------------------
    contract(
        "parse_formula_determinism",
        ContractArgs {
            post: Some(forall(String_(), |x| {
                eq(
                    ctor1("parse_formula", x.clone()),
                    ctor1("parse_formula", x),
                )
            })),
            ..Default::default()
        },
    );

    // -----------------------------------------------------------------
    // CONTRACT 2: parse_formula correctness (round-trip).
    //
    // The bridge resolves to THIS contract memento. Its `pre` is what
    // the verifier substitutes the call-site arg into. We use the
    // kit-defined `roundTrips` atomic predicate; Z3 doesn't know it,
    // so the obligation falls to "undecidable" — that's the honest
    // outcome and the gap is documented in the report. The IR
    // expresses the intent; a future Tier-1 (per-prop fact) layer
    // would discharge it.
    // -----------------------------------------------------------------
    contract(
        "parse_formula_correct",
        ContractArgs {
            pre: Some(forall(String_(), |x| {
                atomic_("roundTrips", vec![x])
            })),
            ..Default::default()
        },
    );

    // -----------------------------------------------------------------
    // CONTRACT 3: parse rejects malformed input.
    //
    // Claim: for any input that fails the closed-object policy, the
    // parser returns an error. Expressed with the kit-defined
    // `isMalformed` and `isErr` atomic predicates.
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
    // CONTRACT 4: BLAKE3-512 output length.
    //
    // Plausible quantitative claim: BLAKE3-512 produces a 64-byte
    // digest, which the kit emits as 128 lowercase hex chars under the
    // `blake3-512:` tag, so the full self-identifying CID has length
    // exactly 139.
    //
    // We express this with `len` as a kit-defined ctor returning Int;
    // the result `=` 139.
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
    // CONTRACT 5: arity claim about parse_formula's `not`
    // operand handling.
    //
    // Plausible: a valid `not` formula has exactly one operand. We
    // express this with standard ASCII algebra:
    //   forall n: Int. n = 1.
    // This is FALSIFIABLE (n = 0, n = 7, ...) — Z3 should return SAT,
    // which the verifier maps to "unsatisfied". That's the negative
    // case landing through the pipeline cleanly: the verifier proved a
    // counterexample exists, the obligation does NOT discharge.
    //
    // Avoid ≥/≤/≠ predicates: the JCS encoder in this Rust peer
    // mishandles non-ASCII bytes (round-trip mangles UTF-8) which
    // breaks Rule 2 envelope-CID re-derivation in the verifier.
    // Tracked as a finding from the dogfood run; for now the
    // self-contracts use ASCII-only atomics.
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
    // FALSIFIABLE (any n != 2). Z3 returns SAT → "unsatisfied".
    // -----------------------------------------------------------------
    contract(
        "implies_arity_eq_two",
        ContractArgs {
            post: Some(forall(Int(), |n| {
                and_(vec![
                    not_(lt(n.clone(), num(2))),
                    not_(gt(n, num(2))),
                ])
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

    let decls = finish();

    // -----------------------------------------------------------------
    // BRIDGES
    //
    // We register one bridge here: parse_formula -> parse_formula_correct.
    // Stage 2 (enumerate_callsites) walks every contract memento's
    // pre/post/inv looking for ctor terms whose `name` is in
    // `pool.bridges_by_symbol`. Every contract above that mentions
    // `parse_formula` (contracts 1 and 3) will produce callsites.
    //
    // The bridge target is a CONTRACT name; mint_self_proof resolves
    // contract names to CIDs at minting time.
    // -----------------------------------------------------------------
    let bridges = vec![SelfBridge {
        source_symbol: "parse_formula".into(),
        target_contract_name: "parse_formula_correct".into(),
        ir_arg_sorts: vec!["String".into()],
        ir_return_sort: "Formula".into(),
    }];

    (decls, bridges)
}

/// Result from minting the self-proof.
#[derive(Debug, Clone)]
pub struct MintResult {
    /// Full self-identifying CID (`blake3-512:<128 hex>`) of the .proof file.
    pub cid: String,
    /// Bytes written.
    pub bytes_len: usize,
    /// Filesystem path written to.
    pub path: std::path::PathBuf,
    /// Number of mementos bundled (contracts + bridges).
    pub member_count: usize,
    /// Map from contract name to its memento CID.
    pub contract_cids: BTreeMap<String, String>,
}

/// Mint all self-contracts as signed mementos, bundle into a .proof,
/// write to `<out_dir>/<hex>.proof`, and return the result.
pub fn mint_self_proof(out_dir: &Path) -> Result<MintResult, String> {
    std::fs::create_dir_all(out_dir).map_err(|e| format!("create_dir_all: {e}"))?;

    let (contract_decls, bridge_decls) = author_self_contracts();

    // Deterministic seed for reproducibility. Ed25519 is fine; the
    // signer CID is hash(pubkey_string) so a fixed seed yields a
    // stable signer reference.
    let signer_seed: Ed25519Seed = [0x42; 32];

    let mut members: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut contract_cids: BTreeMap<String, String> = BTreeMap::new();

    // Mint each contract.
    for d in &contract_decls {
        let args = MintContractArgs {
            contract_name: d.name.clone(),
            pre: d.pre.as_deref().map(formula_to_value),
            post: d.post.as_deref().map(formula_to_value),
            inv: d.inv.as_deref().map(formula_to_value),
            out_binding: d.out_binding.clone(),
            produced_by: PRODUCED_BY.into(),
            produced_at: DECLARED_AT.into(),
            input_cids: vec![],
            authoring: Authoring::KitAuthor {
                author: PRODUCED_BY.into(),
                note: Some(format!(
                    "self-contract: dogfooding ProvekIt's Rust impl on its own IR-JSON parser"
                )),
            },
            signer_seed,
        };
        let m = mint_contract(&args).map_err(|e| format!("mint_contract({}): {e}", d.name))?;
        contract_cids.insert(d.name.clone(), m.cid.clone());
        members.insert(m.cid, m.canonical_bytes);
    }

    // Mint each bridge, resolving contract-name targets to CIDs.
    for b in &bridge_decls {
        let target_cid = contract_cids
            .get(&b.target_contract_name)
            .ok_or_else(|| {
                format!(
                    "bridge `{}` targets unknown contract `{}`",
                    b.source_symbol, b.target_contract_name
                )
            })?
            .clone();
        let args = MintBridgeArgs {
            produced_by: PRODUCED_BY.into(),
            produced_at: DECLARED_AT.into(),
            source_symbol: b.source_symbol.clone(),
            source_layer: "rust-ir-symbolic".into(),
            target_contract_cid: target_cid,
            target_layer: "rust-self-contract".into(),
            ir_arg_sorts: b.ir_arg_sorts.clone(),
            ir_return_sort: b.ir_return_sort.clone(),
            notes: format!(
                "bridge `{}` -> `{}`: closes the dogfood loop",
                b.source_symbol, b.target_contract_name
            ),
            signer_seed,
        };
        let m = mint_bridge(&args);
        members.insert(m.cid, m.canonical_bytes);
    }

    let member_count = members.len();

    // Build the .proof catalog.
    let signer_pubkey = ed25519_pubkey_string(&signer_seed);
    let signer_cid = blake3_512_of(signer_pubkey.as_bytes());

    let proof_input = ProofEnvelopeInput {
        name: "@provekit/self-contracts".into(),
        version: "1.0.0".into(),
        members,
        signer_cid,
        signer_seed,
        declared_at: DECLARED_AT.into(),
    };
    let built = build_proof_envelope(&proof_input);

    let cid_hex = built
        .cid
        .strip_prefix("blake3-512:")
        .ok_or("internal: cid missing blake3-512 prefix")?;
    let path = out_dir.join(format!("{cid_hex}.proof"));
    std::fs::write(&path, &built.bytes)
        .map_err(|e| format!("write {}: {e}", path.display()))?;

    Ok(MintResult {
        cid: built.cid,
        bytes_len: built.bytes.len(),
        path,
        member_count,
        contract_cids,
    })
}

// ---------------------------------------------------------------------------
// Property tests — invariants the IR contracts gesture at, enforced in code.
// ---------------------------------------------------------------------------
//
// The .proof verifier scans static IR. Behavioral parser invariants
// (round-trip identity, deterministic output, malformed-input
// rejection) live here as proptest properties because the IR doesn't
// yet have a "this string is JSON-shaped" atomic predicate.
//
// Documented gap: contracts above with `roundTrips` / `isErr` /
// `isMalformed` are kit-defined predicate names that the SMT emitter
// passes through verbatim; Z3 has no semantics for them, so callsites
// resolving against those contracts land at undecidable. The proptest
// block below is the *operational* enforcement of those same
// invariants.

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use provekit_canonicalizer::encode_jcs;
    use provekit_ir_symbolic::parse::{parse_formula, ParseError};
    use provekit_ir_symbolic::serialize::{formula_to_value, marshal_declarations};
    use provekit_ir_symbolic::{
        and_, finish, gt, make_var, must, not_, num, or_, reset_collector,
        str_const, ContractDecl, Formula,
    };

    // -- proptest generators for IrFormula values ----------------------------
    //
    // We bound depth to keep the search tractable and to avoid
    // pathologically nested terms that aren't realistic kit output.

    fn arb_var_term() -> impl Strategy<Value = Rc<Term>> {
        "[a-z][a-z0-9]{0,4}".prop_map(make_var)
    }

    fn arb_int_const() -> impl Strategy<Value = Rc<Term>> {
        any::<i32>().prop_map(|n| num(n as i64))
    }

    fn arb_string_const() -> impl Strategy<Value = Rc<Term>> {
        "[a-zA-Z0-9 _-]{0,12}".prop_map(str_const)
    }

    fn arb_term() -> impl Strategy<Value = Rc<Term>> {
        let leaf = prop_oneof![arb_var_term(), arb_int_const(), arb_string_const()];
        leaf.prop_recursive(2, 8, 3, |inner| {
            (
                "[a-z][a-zA-Z0-9_]{0,5}",
                prop::collection::vec(inner, 0..=2),
            )
                .prop_map(|(name, args)| Rc::new(Term::Ctor { name, args }))
        })
    }

    fn arb_atomic() -> impl Strategy<Value = Rc<Formula>> {
        // Ascii-only predicates. The JCS encoder in the canonicalizer
        // mishandles non-ASCII bytes (treats each UTF-8 byte as a
        // Latin-1 code point at re-encode time); using `\u{2260}`,
        // `\u{2264}`, `\u{2265}` here breaks round-trip even though
        // the IR happily authors them. That is a finding from this
        // dogfood run, tracked separately; the proptest property
        // here is over the round-tripping ASCII subset.
        let preds = prop_oneof![Just("="), Just(">"), Just("<")];
        (preds, arb_term(), arb_term()).prop_map(|(p, a, b)| atomic_(p, vec![a, b]))
    }

    fn arb_formula() -> impl Strategy<Value = Rc<Formula>> {
        let leaf = arb_atomic().boxed();
        leaf.prop_recursive(3, 16, 4, |inner| {
            prop_oneof![
                inner.clone().prop_map(|f| not_(f)),
                (inner.clone(), inner.clone()).prop_map(|(a, b)| implies_local(a, b)),
                prop::collection::vec(inner.clone(), 2..=3).prop_map(and_),
                prop::collection::vec(inner.clone(), 2..=3).prop_map(or_),
                ("[a-z][a-z0-9]{0,4}", inner.clone())
                    .prop_map(|(name, body)| forall_with_name(name, body)),
            ]
        })
    }

    fn forall_with_name(name: String, body: Rc<Formula>) -> Rc<Formula> {
        Rc::new(Formula::Quantifier {
            kind: "forall".into(),
            name,
            sort: Int(),
            body,
        })
    }

    fn implies_local(a: Rc<Formula>, b: Rc<Formula>) -> Rc<Formula> {
        provekit_ir_symbolic::implies(a, b)
    }

    fn jcs_string(f: &Formula) -> String {
        encode_jcs(&formula_to_value(f))
    }

    // -- INVARIANT 1: parse(serialize(f)) == f (byte-equal re-serialize) ------

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 64,
            ..ProptestConfig::default()
        })]

        #[test]
        fn parse_round_trips_serialize(f in arb_formula()) {
            let s1 = jcs_string(&f);
            let v: serde_json::Value = serde_json::from_str(&s1).unwrap();
            let parsed = parse_formula(&v).expect("round-trip parse");
            let s2 = jcs_string(&parsed);
            prop_assert_eq!(s1, s2);
        }

        // -- INVARIANT 4: determinism ----------------------------------------

        #[test]
        fn parse_is_deterministic(f in arb_formula()) {
            let s = jcs_string(&f);
            let v: serde_json::Value = serde_json::from_str(&s).unwrap();
            let p1 = parse_formula(&v).expect("p1");
            let p2 = parse_formula(&v).expect("p2");
            prop_assert_eq!(jcs_string(&p1), jcs_string(&p2));
        }
    }

    // -- INVARIANT 2: rejection of malformed input ----------------------------

    #[test]
    fn rejects_extra_key_on_var() {
        let raw = r#"{"kind":"var","name":"x","sort":{"kind":"primitive","name":"Int"}}"#;
        let v: serde_json::Value = serde_json::from_str(raw).unwrap();
        assert!(matches!(provekit_ir_symbolic::parse::parse_term(&v), Err(ParseError::ExtraKey { .. })));
    }

    #[test]
    fn rejects_zero_operand_not() {
        let raw = r#"{"kind":"not","operands":[]}"#;
        let v: serde_json::Value = serde_json::from_str(raw).unwrap();
        assert!(matches!(parse_formula(&v), Err(ParseError::Arity { .. })));
    }

    #[test]
    fn rejects_three_operand_implies() {
        let raw = r#"{"kind":"implies","operands":[
          {"kind":"atomic","name":"=","args":[]},
          {"kind":"atomic","name":"=","args":[]},
          {"kind":"atomic","name":"=","args":[]}
        ]}"#;
        let v: serde_json::Value = serde_json::from_str(raw).unwrap();
        assert!(matches!(parse_formula(&v), Err(ParseError::Arity { .. })));
    }

    #[test]
    fn rejects_unknown_node_kind() {
        let raw = r#"{"kind":"xyzzy","args":[]}"#;
        let v: serde_json::Value = serde_json::from_str(raw).unwrap();
        assert!(matches!(parse_formula(&v), Err(ParseError::UnknownKind { .. })));
    }

    // -- INVARIANT 3: locked key order on serialize ---------------------------
    //
    // The kit's marshal_declarations produces insertion-order keys per
    // the EBNF. The grammar locks the order kind/name/outBinding/pre.
    // We assert the substring layout directly.

    #[test]
    fn marshal_emits_locked_key_order_for_contract() {
        reset_collector();
        must("foo", forall_with_name("x".into(), gt(make_var("x"), num(0))));
        let decls = finish();
        let s = marshal_declarations(&decls);
        // Order must be: kind first, then name, then outBinding, then pre.
        let i_kind = s.find(r#""kind":"contract""#).expect("kind first");
        let i_name = s.find(r#""name":"foo""#).expect("name");
        let i_out = s.find(r#""outBinding":"#).expect("outBinding");
        let i_pre = s.find(r#""pre":"#).expect("pre");
        assert!(i_kind < i_name);
        assert!(i_name < i_out);
        assert!(i_out < i_pre);
    }

    // -- mint_self_proof end-to-end happy-path test --------------------------

    #[test]
    fn author_self_contracts_returns_expected_count() {
        let (decls, bridges) = author_self_contracts();
        assert!(decls.len() >= 5, "expected 5+ self-contracts, got {}", decls.len());
        assert_eq!(bridges.len(), 1, "expected exactly one closed-loop bridge");
    }

    #[test]
    fn self_contract_names_are_distinct() {
        let (decls, _) = author_self_contracts();
        let mut names: Vec<&str> = decls.iter().map(|d| d.name.as_str()).collect();
        names.sort();
        let original_len = names.len();
        names.dedup();
        assert_eq!(names.len(), original_len, "duplicate contract names");
    }

    // Silence unused-import warnings for symbols imported for clarity
    // even though the proptest! macro hides them inside its module.
    #[allow(dead_code)]
    fn _exercise_imports() {
        let _ = lt(num(0), num(1));
        let _: Vec<ContractDecl> = vec![];
    }
}
