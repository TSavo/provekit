// SPDX-License-Identifier: Apache-2.0
//
// provekit-self-contracts
//
// ProvekIt's first-class dogfood. The framework eats itself: every
// public-API source file in the workspace ships a sibling
// `<name>.invariant.rs` file authoring contracts about its own surface
// using the kit's own primitives. This crate is the BUILD ORCHESTRATOR:
// it walks every `.invariant.rs` file in the workspace, runs each
// `invariants()` function in turn, collects the IR, mints each as a
// signed contract memento, registers a closed-loop bridge for
// `parse_formula -> parse_formula_correct`, and bundles the lot into a
// single `provekit-self-contracts.proof` envelope.
//
// File layout:
//   * `src/lib.rs` — the orchestrator (this file). Pulls in every
//     `.invariant.rs` via `#[path]` so they compile as plain Rust
//     modules without polluting their host crate's dep graph.
//   * `src/bin/mint-self-contracts.rs` — the runnable binary. Calls
//     into `mint_self_proof()` here and prints the resulting CID;
//     also runs the verifier against the produced .proof and prints
//     the report.
//
// Why `#[path]`? Because `provekit-ir-symbolic` already depends on
// `provekit-canonicalizer` / `-claim-envelope` / `-proof-envelope`. If
// those crates' .invariant.rs files lived in their own host modules,
// they would need a back-edge to `-ir-symbolic` (for the kit's
// authoring API), which would close a cycle. The orchestrator hosts
// the includes; the .invariant.rs files physically live next to the
// source they describe and are free of any host-crate coupling beyond
// the kit's surface.

use std::collections::BTreeMap;
use std::path::Path;

use provekit_canonicalizer::blake3_512_of;
use provekit_claim_envelope::{
    mint_bridge, mint_contract, Authoring, MintBridgeArgs, MintContractArgs,
};
use provekit_ir_symbolic::serialize::formula_to_value;
use provekit_ir_symbolic::{
    begin_collecting, finish, reset_collector, ContractDecl,
};
use provekit_proof_envelope::{
    build_proof_envelope, ed25519_pubkey_string, Ed25519Seed,
    ProofEnvelopeInput,
};

const PRODUCED_BY: &str = "provekit-self-contracts@1.0";
const DECLARED_AT: &str = "2026-04-30T18:00:00.000Z";

// --- Per-source-file .invariant.rs modules ---------------------------------
//
// Each include compiles the named .invariant.rs file as a private
// module of this crate. The module exposes a `pub fn invariants()` that
// uses the kit's process-local `CONTRACT_COLLECTOR` thread-local; the
// orchestrator drains the collector after each call.

#[path = "../../provekit-canonicalizer/src/jcs.invariant.rs"]
mod jcs_invariants;

#[path = "../../provekit-canonicalizer/src/hash.invariant.rs"]
mod hash_invariants;

#[path = "../../provekit-proof-envelope/src/cbor.invariant.rs"]
mod cbor_invariants;

#[path = "../../provekit-proof-envelope/src/sign.invariant.rs"]
mod sign_invariants;

#[path = "../../provekit-proof-envelope/src/proof.invariant.rs"]
mod proof_invariants;

#[path = "../../provekit-claim-envelope/src/lib.invariant.rs"]
mod claim_envelope_invariants;

#[path = "../../provekit-ir-symbolic/src/parse.invariant.rs"]
mod parse_invariants;

#[path = "../../provekit-ir-symbolic/src/serialize.invariant.rs"]
mod serialize_invariants;

#[path = "../../provekit-ir-symbolic/src/invariants.rs"]
mod kit_invariants;

#[path = "../../provekit-verifier/src/load_all_proofs.invariant.rs"]
mod load_all_proofs_invariants;

#[path = "../../provekit-verifier/src/enumerate_callsites.invariant.rs"]
mod enumerate_callsites_invariants;

#[path = "../../provekit-verifier/src/resolve_target.invariant.rs"]
mod resolve_target_invariants;

#[path = "../../provekit-verifier/src/instantiate.invariant.rs"]
mod instantiate_invariants;

#[path = "../../provekit-verifier/src/smt_emitter.invariant.rs"]
mod smt_emitter_invariants;

// --- Orchestrator types ----------------------------------------------------

/// Source-file label tagging which module of contracts we're walking.
/// Used in the mint result for traceability (every contract knows the
/// host file it was authored in).
#[derive(Debug, Clone)]
pub struct InvariantSource {
    pub label: &'static str,
    pub path: &'static str,
}

/// One bridge declaration coupling an IR symbol (a ctor `name`) to a
/// contract memento that the verifier should use as the discharge
/// target. Names are insertion-order; minting is deferred to
/// `mint_self_proof`.
#[derive(Debug, Clone)]
pub struct SelfBridge {
    pub source_symbol: String,
    pub target_contract_name: String,
    pub ir_arg_sorts: Vec<String>,
    pub ir_return_sort: String,
}

/// One source file's authored contracts plus the source label.
#[derive(Debug, Clone)]
pub struct AuthoredSlab {
    pub source: InvariantSource,
    pub contracts: Vec<ContractDecl>,
}

/// All authored contracts across every `.invariant.rs` file in the
/// workspace, plus the closed-loop bridge declarations. No I/O.
pub fn author_all_invariants() -> (Vec<AuthoredSlab>, Vec<SelfBridge>) {
    // Each slab does its own reset so contract names from prior files
    // can't leak. The collector is process-local.
    let slabs = vec![
        run_one_slab(
            InvariantSource {
                label: "jcs",
                path: "provekit-canonicalizer/src/jcs.invariant.rs",
            },
            jcs_invariants::invariants,
        ),
        run_one_slab(
            InvariantSource {
                label: "hash",
                path: "provekit-canonicalizer/src/hash.invariant.rs",
            },
            hash_invariants::invariants,
        ),
        run_one_slab(
            InvariantSource {
                label: "cbor",
                path: "provekit-proof-envelope/src/cbor.invariant.rs",
            },
            cbor_invariants::invariants,
        ),
        run_one_slab(
            InvariantSource {
                label: "sign",
                path: "provekit-proof-envelope/src/sign.invariant.rs",
            },
            sign_invariants::invariants,
        ),
        run_one_slab(
            InvariantSource {
                label: "proof",
                path: "provekit-proof-envelope/src/proof.invariant.rs",
            },
            proof_invariants::invariants,
        ),
        run_one_slab(
            InvariantSource {
                label: "claim-envelope",
                path: "provekit-claim-envelope/src/lib.invariant.rs",
            },
            claim_envelope_invariants::invariants,
        ),
        run_one_slab(
            InvariantSource {
                label: "parse",
                path: "provekit-ir-symbolic/src/parse.invariant.rs",
            },
            parse_invariants::invariants,
        ),
        run_one_slab(
            InvariantSource {
                label: "serialize",
                path: "provekit-ir-symbolic/src/serialize.invariant.rs",
            },
            serialize_invariants::invariants,
        ),
        run_one_slab(
            InvariantSource {
                label: "kit-invariants",
                path: "provekit-ir-symbolic/src/invariants.rs",
            },
            kit_invariants::invariants,
        ),
        run_one_slab(
            InvariantSource {
                label: "load_all_proofs",
                path: "provekit-verifier/src/load_all_proofs.invariant.rs",
            },
            load_all_proofs_invariants::invariants,
        ),
        run_one_slab(
            InvariantSource {
                label: "enumerate_callsites",
                path: "provekit-verifier/src/enumerate_callsites.invariant.rs",
            },
            enumerate_callsites_invariants::invariants,
        ),
        run_one_slab(
            InvariantSource {
                label: "resolve_target",
                path: "provekit-verifier/src/resolve_target.invariant.rs",
            },
            resolve_target_invariants::invariants,
        ),
        run_one_slab(
            InvariantSource {
                label: "instantiate",
                path: "provekit-verifier/src/instantiate.invariant.rs",
            },
            instantiate_invariants::invariants,
        ),
        run_one_slab(
            InvariantSource {
                label: "smt_emitter",
                path: "provekit-verifier/src/smt_emitter.invariant.rs",
            },
            smt_emitter_invariants::invariants,
        ),
    ];

    // The single closed-loop bridge from the original dogfood. The
    // parse.invariant.rs file authors `parse_formula_determinism`
    // (which contains a `parse_formula` ctor) and
    // `parse_formula_correct` (the bridge target with a `roundTrips`
    // pre). The bridge below makes the verifier walk:
    //   load -> enumerate -> resolve -> instantiate -> smt-emit -> Z3.
    let bridges = vec![SelfBridge {
        source_symbol: "parse_formula".into(),
        target_contract_name: "parse_formula_correct".into(),
        ir_arg_sorts: vec!["String".into()],
        ir_return_sort: "Formula".into(),
    }];

    (slabs, bridges)
}

fn run_one_slab(
    source: InvariantSource,
    f: fn(),
) -> AuthoredSlab {
    reset_collector();
    begin_collecting();
    f();
    let contracts = finish();
    AuthoredSlab { source, contracts }
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
    /// Per-source-file count of contracts authored, for the report.
    pub per_source_counts: Vec<(String, usize)>,
    /// Total contracts authored (sum of per_source_counts).
    pub total_contracts: usize,
}

/// Mint all `.invariant.rs` contracts as signed mementos, register the
/// closed-loop bridge memento, bundle into a `.proof`, write to
/// `<out_dir>/<hex>.proof`, and return the result.
pub fn mint_self_proof(out_dir: &Path) -> Result<MintResult, String> {
    std::fs::create_dir_all(out_dir).map_err(|e| format!("create_dir_all: {e}"))?;

    let (slabs, bridge_decls) = author_all_invariants();

    // Deterministic seed for reproducibility. Ed25519 is fine; the
    // signer CID is hash(pubkey_string) so a fixed seed yields a
    // stable signer reference.
    let signer_seed: Ed25519Seed = [0x42; 32];

    let mut members: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut contract_cids: BTreeMap<String, String> = BTreeMap::new();
    let mut per_source_counts: Vec<(String, usize)> = Vec::new();
    let mut total_contracts: usize = 0;

    for slab in &slabs {
        per_source_counts.push((slab.source.label.into(), slab.contracts.len()));
        total_contracts += slab.contracts.len();
        for d in &slab.contracts {
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
                        "self-contract from {}",
                        slab.source.path
                    )),
                },
                signer_seed,
            };
            let m = mint_contract(&args)
                .map_err(|e| format!("mint_contract({}): {e}", d.name))?;
            // Detect duplicate names ACROSS slabs and fail loud.
            if contract_cids.contains_key(&d.name) {
                return Err(format!(
                    "duplicate contract name `{}` across .invariant.rs files",
                    d.name
                ));
            }
            contract_cids.insert(d.name.clone(), m.cid.clone());
            members.insert(m.cid, m.canonical_bytes);
        }
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
        binary_cid: None,
        metadata: None,
        members,
        signer_cid,
        signer_seed,
        declared_at: DECLARED_AT.into(),
    };
    let built = build_proof_envelope(&proof_input);

    // The bundled filename IS the CID (per protocol/specs/2026-04-30-proof-file-format.md).
    // Convention is `<full-cid>.proof`, where `<full-cid>` includes the
    // `blake3-512:` prefix. The verifier tolerates the bare-hex form,
    // but every C++/Go/TS publisher emits the prefixed form, so we
    // match.
    if !built.cid.starts_with("blake3-512:") {
        return Err("internal: cid missing blake3-512 prefix".into());
    }
    let path = out_dir.join(format!("{cid}.proof", cid = built.cid));
    std::fs::write(&path, &built.bytes)
        .map_err(|e| format!("write {}: {e}", path.display()))?;

    Ok(MintResult {
        cid: built.cid,
        bytes_len: built.bytes.len(),
        path,
        member_count,
        contract_cids,
        per_source_counts,
        total_contracts,
    })
}

// ---------------------------------------------------------------------------
// Property tests — invariants the IR contracts gesture at, enforced in code.
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
    use super::*;
    use proptest::prelude::*;
    use provekit_canonicalizer::encode_jcs;
    use provekit_ir_symbolic::parse::{parse_formula, ParseError};
    use provekit_ir_symbolic::serialize::{formula_to_value, marshal_declarations};
    use provekit_ir_symbolic::{
        and_, atomic_, finish, gt, lt, make_var, must, not_, num, or_,
        reset_collector, str_const, ContractDecl, Formula, Int, Term,
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

    #[test]
    fn marshal_emits_locked_key_order_for_contract() {
        reset_collector();
        must("foo", forall_with_name("x".into(), gt(make_var("x"), num(0))));
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

    // -- author_all_invariants happy path ------------------------------------

    #[test]
    fn author_all_invariants_yields_30_plus_contracts() {
        let (slabs, bridges) = author_all_invariants();
        let total: usize = slabs.iter().map(|s| s.contracts.len()).sum();
        assert!(
            total >= 30,
            "expected 30+ contracts across all .invariant.rs files, got {total}"
        );
        assert_eq!(bridges.len(), 1, "expected exactly one closed-loop bridge");
    }

    #[test]
    fn invariant_module_names_are_distinct_per_file() {
        // Within each slab the contract names must be distinct.
        let (slabs, _) = author_all_invariants();
        for slab in &slabs {
            let mut names: Vec<&str> =
                slab.contracts.iter().map(|d| d.name.as_str()).collect();
            names.sort();
            let original_len = names.len();
            names.dedup();
            assert_eq!(
                names.len(),
                original_len,
                "duplicate contract names within `{}`",
                slab.source.label
            );
        }
    }

    // -- Determinism: minting twice yields the SAME CID ----------------------

    #[test]
    fn mint_self_proof_is_deterministic() {
        let dir1 = tempdir();
        let dir2 = tempdir();
        let m1 = mint_self_proof(&dir1).expect("mint 1");
        let m2 = mint_self_proof(&dir2).expect("mint 2");
        assert_eq!(m1.cid, m2.cid, "CID must be byte-deterministic across runs");
        assert_eq!(m1.member_count, m2.member_count);
        let _ = std::fs::remove_dir_all(&dir1);
        let _ = std::fs::remove_dir_all(&dir2);
    }

    fn tempdir() -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!("provekit-self-test-{nanos}-{}", std::process::id()));
        p
    }

    // Silence unused-import warnings for symbols imported for clarity.
    #[allow(dead_code)]
    fn _exercise_imports() {
        let _ = lt(num(0), num(1));
        let _: Vec<ContractDecl> = vec![];
    }
}
