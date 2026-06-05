// SPDX-License-Identifier: Apache-2.0
//
// provekit-self-contracts
//
// ProvekIt's shared self-contract support for protocol-level dogfood.
//
// Rust kit self-contract minting now comes from the native source lifter
// declared in `implementations/rust/.provekit/config.toml`. This crate no
// longer slab-walks sibling `.invariant.rs` files; it keeps only the shared
// lift-plugin protocol contract slab used by conformance checks and peer kits.

use std::collections::BTreeMap;

// catalog-format spec rules as machine-enforceable contracts. Independent
// of the per-Rust-file orchestrator below; lives in its own module so
// the file/test layout matches its origin (`protocol/specs/2026-04-30-protocol-catalog-format.md`).
pub mod catalog_format;

// lift-plugin-protocol spec rules as machine-enforceable contracts.
// Source contracts that each kit's lift-plugin implementation will
// bridge to. Origin: `protocol/specs/2026-04-30-lift-plugin-protocol.md`.
pub mod lift_plugin_protocol;

use provekit_claim_envelope::{
    compute_contract_set_cid, contract_cid as compute_contract_cid, Authoring, MintContractArgs,
};
use provekit_ir_symbolic::serialize::formula_to_value;
use provekit_ir_symbolic::{begin_collecting, finish, reset_collector, ContractDecl};
use provekit_proof_envelope::Ed25519Seed;

const PRODUCED_BY: &str = "provekit-self-contracts@1.0";
const DECLARED_AT: &str = "2026-04-30T18:00:00.000Z";

// lift-plugin-protocol spec rules.
use lift_plugin_protocol as lift_plugin_protocol_invariants;

/// The standard protocol-contract slab: lift-plugin-protocol C1-C9, split
/// into the concrete contract facets authored by `lift_plugin_protocol.rs`.
///
/// This list is the stable ordering for the separate protocol contract set.
/// The set CID itself is order-independent, but keeping the order named here
/// makes missing/additive drift loud in tests and extraction tools.
pub const LIFT_PLUGIN_PROTOCOL_CONTRACT_NAMES: &[&str] = &[
    "lift_plugin_initialize_protocol_version_match",
    "lift_plugin_initialize_capabilities_authoring_surfaces_nonempty",
    "lift_plugin_initialize_capabilities_ir_version_starts_with_v",
    "lift_plugin_lift_request_surface_is_string",
    "lift_plugin_lift_request_source_paths_nonempty",
    "lift_plugin_lift_request_source_paths_each_nonempty",
    "lift_plugin_lift_request_options_layer_well_formed",
    "lift_plugin_lift_request_surface_in_capabilities",
    "lift_plugin_lift_response_kind_matches_layer",
    "lift_plugin_lift_response_ir_document_array",
    "lift_plugin_diagnostic_field_is_array",
    "lift_emits_call_edge_stream",
];

/// Accepted contractSetCid for the standard lift-plugin-protocol slab above.
///
/// This is the protocol-only trust anchor peer kits bridge to. It is
/// deliberately separate from any kit's full self-contract surface CID:
/// protocol evolution must move this pin explicitly, while ordinary Rust
/// dogfood-surface drift only moves the Rust kit pin.
pub const ACCEPTED_LIFT_PLUGIN_PROTOCOL_CONTRACT_SET_CID: &str = concat!(
    "blake3-512:",
    "2b41786f188d603fe0232c6cf64d7a1b4dfc29b07cd1f272b9241c0ed08ebd1f",
    "1e8cc4eb032d0715915115481ef272f5e454fec77216fadfbccb7098e888db81"
);

/// Source-file label tagging which module of contracts we're walking.
/// Used for traceability when deriving the protocol contract slab.
#[derive(Debug, Clone)]
pub struct InvariantSource {
    pub label: &'static str,
    pub path: &'static str,
}

fn run_one_slab(_source: InvariantSource, f: fn()) -> Vec<ContractDecl> {
    reset_collector();
    begin_collecting();
    f();
    finish()
}

/// Derive the signer-independent contract CIDs for the standard protocol
/// contract slab without minting a kit attestation.
pub fn lift_plugin_protocol_contract_cids() -> Result<BTreeMap<String, String>, String> {
    let source = InvariantSource {
        label: "lift_plugin_protocol",
        path: "provekit-self-contracts/src/lift_plugin_protocol.rs",
    };
    let contracts = run_one_slab(source.clone(), lift_plugin_protocol_invariants::invariants);
    let signer_seed: Ed25519Seed = [0x42; 32];
    let mut cids = BTreeMap::new();

    for d in &contracts {
        let args = MintContractArgs {
            evidence_term: None,
            formals: Vec::new(),
            emit_empty_formals: false,
            formal_sorts: Vec::new(),
            library: None,
            body_discharge_eligible: true,
            body_discharge_refusal_reason: None,
            panic_loci: Vec::new(),
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
                note: Some(format!("protocol contract from {}", source.path)),
            },
            signer_seed,
        };
        if cids.contains_key(&d.name) {
            return Err(format!("duplicate protocol contract name `{}`", d.name));
        }
        cids.insert(d.name.clone(), compute_contract_cid(&args));
    }

    let mut missing = Vec::new();
    for name in LIFT_PLUGIN_PROTOCOL_CONTRACT_NAMES {
        if !cids.contains_key(*name) {
            missing.push(*name);
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "lift_plugin_protocol slab missing {} expected contract(s): {}",
            missing.len(),
            missing.join(", ")
        ));
    }
    if cids.len() != LIFT_PLUGIN_PROTOCOL_CONTRACT_NAMES.len() {
        return Err(format!(
            "lift_plugin_protocol slab emitted {} contract(s); expected {}",
            cids.len(),
            LIFT_PLUGIN_PROTOCOL_CONTRACT_NAMES.len()
        ));
    }

    Ok(cids)
}

/// Derive the standard protocol contract-set CID.
pub fn lift_plugin_protocol_contract_set_cid() -> Result<String, String> {
    let cids = lift_plugin_protocol_contract_cids()?;
    Ok(compute_contract_set_cid(cids.values().cloned().collect()))
}

// ---------------------------------------------------------------------------
// Property tests: invariants the IR contracts gesture at, enforced in code.
// ---------------------------------------------------------------------------
//
// The .proof verifier scans static IR. Behavioral parser invariants
// (round-trip identity, deterministic output, malformed-input
// rejection) live here as proptest properties because the IR doesn't
// yet have "this string is JSON-shaped" / "this byte sequence equals
// the deterministic CBOR" predicates.
//
// Documented gap: contracts above with `roundTrips` / `isErr` /
// `isMalformed` and friends are kit-defined names that the SMT emitter
// passes through verbatim; Z3 has no semantics for them, so callsites
// resolving against those contracts land at undecidable. The proptest
// block below is the OPERATIONAL enforcement of those same invariants.

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use provekit_canonicalizer::encode_jcs;
    use provekit_ir_symbolic::parse::{parse_formula, ParseError};
    use provekit_ir_symbolic::serialize::{formula_to_value, marshal_declarations};
    use provekit_ir_symbolic::{
        and_, atomic_, finish, gt, lt, make_var, must, not_, num, or_, reset_collector, str_const,
        ContractDecl, Formula, Int, Term,
    };
    use std::rc::Rc;

    // -- proptest generators for IrFormula values ----------------------------

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
        assert!(matches!(
            provekit_ir_symbolic::parse::parse_term(&v),
            Err(ParseError::ExtraKey { .. })
        ));
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
        assert!(matches!(
            parse_formula(&v),
            Err(ParseError::UnknownKind { .. })
        ));
    }

    // -- INVARIANT 3: locked key order on serialize ---------------------------

    #[test]
    fn marshal_emits_locked_key_order_for_contract() {
        reset_collector();
        must(
            "foo",
            forall_with_name("x".into(), gt(make_var("x"), num(0))),
        );
        let decls = finish();
        let s = marshal_declarations(&decls);
        let i_kind = s.find(r#""kind":"contract""#).expect("kind first");
        let i_name = s.find(r#""name":"foo""#).expect("name");
        let i_out = s.find(r#""outBinding":"#).expect("outBinding");
        let i_pre = s.find(r#""pre":"#).expect("pre");
        assert!(i_kind < i_name);
        assert!(i_name < i_out);
        assert!(i_out < i_pre);
    }

    // Silence unused-import warnings for symbols imported for clarity.
    #[allow(dead_code)]
    fn _exercise_imports() {
        let _ = lt(num(0), num(1));
        let _: Vec<ContractDecl> = vec![];
    }
}
