// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use libprovekit::canonical::{json_cid, serializable_cid, serializable_jcs};
use libprovekit::compose::{
    build_value, cid_of_value, compose_function_contracts, jcs_bytes_of_value, EffectSet,
    FunctionContractMemento, Locus,
};
use libprovekit::core::{
    address, compose, execute_path, link, platform_semantics_for_lower_target, prove, transform,
    verify, ArityShape, AritySlot, CKit, Canonical, Catalog, Cid, ConformanceDeclaration, Dialect,
    DomainClaim, DomainKind, FunctionContractDomain, HashMapCatalog, HashMapInputCatalog, Input,
    InputCatalog, Kit, KitRegistry, LanguageSignature, LiftKit, LiftPluginKit, OpCoverageVerdict,
    Path, PathAlgebra, PathDocument, PathDocumentError, PathError, PlatformSemanticComparisonError,
    PlatformSemanticsDeclaration, Refutation, Side, SlotSort, Term, Truth, Verb, Verdict, Witness,
};
use provekit_canonicalizer::{blake3_512_of, Value};
use provekit_ir_types::{
    composition_refusal_compose_input_cid, composition_refusal_header_cid,
    composition_refusal_signature, CompositionRefusalEnvelope, CompositionRefusalHeader,
    CompositionRefusalMemento, CompositionRefusalMetadata, DimensionValueMemento, IrFormula,
    IrTerm, PlatformSemanticTag, Sort,
};
use serde_json::json;

fn any_sort() -> Sort {
    Sort::Primitive {
        name: "Any".to_string(),
    }
}

fn bool_true() -> IrFormula {
    IrFormula::Atomic {
        name: "true".to_string(),
        args: vec![],
    }
}

const PYTHON_PLATFORM_DIMENSIONS: &[(&str, &str)] = &[
    ("ArithmeticOverflow", "ArbitraryPrecision"),
    ("IntegerDivisionRounding", "Floor"),
    ("ShiftMode", "Arithmetic"),
    ("NullSemantics", "RaiseZeroDivisionError"),
    ("BitwiseSemantics", "TwosComplement"),
];

const PYTHON_PLATFORM_CONCEPT_OP_CIDS: &[&str] = &[
    "blake3-512:95fc70e63a5550fd2e25142f13932919c59d085654ab387789c798886b0111c61d28fe533fc98b50df70eea9428a9af8aa75372c8b1c1deb3acc1a4094790468",
    "blake3-512:b7c54558573348bb3a9297732547a8e6e9d152403d292df7426b6bb8a248f705b4b030bf2a22ba547a17d6f1bfaf8e75a6843e02e8f23a8226ebc09e2a8622af",
    "blake3-512:46cd627de058c8d4f7d087ea33f4904af65ad4b2e3cfd3aff8f44bf27db96b33c2dae39cd30f53898c233c9465ba8d2701c69e5903d48935113103b4db00fd03",
    "blake3-512:c6a13abbcafdf83edcff49d883a7c7440faadd8af896da0ad46e2bcb177ed0649d005b4ddecd4689cf565b10679219a07c784399bafe5c6174642e1b808d7839",
    "blake3-512:92340897b43965e01454b00a6a43ec54b2bf0e01213a45fa2311f730dde18adf8da97a22458c1a2a0fb23ce85ef3ad9b22e704804c74f41997aba3ba02cefe0d",
    "blake3-512:f9cdfcba8d0e223803126504a2a6ed10005fa61acb5c55b74b270bc66d963eb7648ab6763f0510760df93145c0f6670087a403417e8b3100c7142e121111807a",
    "blake3-512:c90e3c159b25e4c4c7f9c899da5aa3ee048a548719ced7360f3e514450811096b21cd5473f22d7a05df088f92210bbc916e65970b9fa1e1511c193ed969f112b",
    "blake3-512:9e96c2445bad6bb1e5a6f902ad7f733e3f4619829b9c0e232361fbf50b978c8332029212ed895762e604d1df009fce58848cda33524a697df798233eae30a14b",
    "blake3-512:d57b54bffe698ed804a4a49486b73a1a8a3e7bd84fb12babaad01ce22d8b7bcb5a35f3476324063f8de9f8090846d0d4fbeb48d78475d07e16f7925b4f264de3",
    "blake3-512:343b1f9faa98218467d810e0a2bb1b1eebeaf921c71a1bc52141f885220afff482c631c52e2157a6067640f4830f928add53ef7aa0386c6a27ee3c8bab6dc353",
    "blake3-512:5e788f0d551081f4e709e4418e01017fa9ae1c04963e7be2862fadad8a8434fafa204629fbec53e2e44624c195ac2e32c0410df25cf8ff3a4be672582f89109f",
    "blake3-512:ad958847b50cf07ddbb92d85ae488a5f983d5619e108476b42e519174cfcce883ecd637544a372b946bb45a1c22893c710bc9b08ea0569ad0e035b3babb6a409",
];

fn pure_identity_contract(fn_name: &str, formal: &str) -> FunctionContractMemento {
    let formals = vec![formal.to_string()];
    let formal_sorts = vec![any_sort()];
    let return_sort = any_sort();
    let pre = bool_true();
    let post = IrFormula::Atomic {
        name: "=".to_string(),
        args: vec![
            IrTerm::Var {
                name: "result".to_string(),
            },
            IrTerm::Var {
                name: formal.to_string(),
            },
        ],
    };
    let effects = EffectSet::empty();
    let locus = Locus::unknown();
    let value: Arc<Value> = build_value(
        fn_name,
        &formals,
        &formal_sorts,
        &return_sort,
        &pre,
        &post,
        None,
        &effects,
        &locus,
        &[],
    );
    let canonical_bytes = jcs_bytes_of_value(&value);
    let cid = cid_of_value(&value);

    FunctionContractMemento {
        fn_name: fn_name.to_string(),
        formals,
        formal_sorts,
        formal_regions: vec![],
        return_sort,
        return_region: None,
        pre,
        post,
        body_cid: None,
        effects,
        locus,
        canonical_bytes,
        cid,
        auto_minted_mementos: vec![],
        concept_hint: None,
    }
}

fn claim_for_contract(contract: FunctionContractMemento) -> DomainClaim {
    let to = Cid::try_from(contract.cid.clone()).expect("fixture cid is valid");
    let artifact = address(&format!("contract:{}", contract.cid));
    DomainClaim {
        domain: DomainKind::FunctionContract,
        contract,
        artifacts: vec![artifact],
        from: vec![],
        premises: vec![],
        to,
        witness: None,
        payload: None,
        verdict: Verdict::Unresolved,
        attestation: None,
    }
}

fn claim_fixture(name: &str, verdict: Verdict) -> DomainClaim {
    let mut claim = claim_for_contract(pure_identity_contract(name, "x"));
    claim.verdict = verdict;
    claim.witness = match verdict {
        Verdict::Proved => Some(Witness::Proof {
            tree: json!({"claim": name}),
        }),
        Verdict::Refuted => Some(Witness::Counterexample {
            model: json!({"claim": name}),
        }),
        Verdict::Unknown => Some(Witness::Unknown {
            transcript: json!({"claim": name}),
        }),
        Verdict::Unresolved => None,
    };
    claim
}

fn term_fixture(name: &str) -> Term {
    Term::Op {
        op_cid: address(&format!("op:{name}")),
        name: name.to_string(),
        args: vec![Term::Const {
            value: json!(1),
            sort: any_sort(),
        }],
    }
}

fn one_step_path(name: &str, inputs: Vec<Cid>) -> Path {
    Path {
        algebra: vec![PathAlgebra {
            name: name.to_string(),
            kit: format!("kit:{name}"),
            inputs,
            depends_on: vec![],
            verb: Verb::Transform,
        }],
    }
}

#[test]
fn canonical_addresses_are_deterministic_for_core_shapes() {
    let op_cid = address(&"identity-op");
    let term = Term::Op {
        op_cid,
        name: "identity".to_string(),
        args: vec![Term::Var {
            name: "x".to_string(),
        }],
    };
    let formula = IrFormula::Atomic {
        name: "=".to_string(),
        args: vec![
            IrTerm::Var {
                name: "x".to_string(),
            },
            IrTerm::Const {
                value: serde_json::json!(1),
                sort: any_sort(),
            },
        ],
    };
    let contract = pure_identity_contract("id", "x");
    let claim = claim_for_contract(contract.clone());

    assert_eq!(address(&term), address(&term));
    assert_eq!(address(&formula), address(&formula));
    assert_eq!(address(&contract), address(&contract));
    assert_eq!(claim.cid(), claim.cid());
    assert!(claim.cid().as_str().starts_with("blake3-512:"));
}

#[test]
fn language_signature_shape_drives_term_identity() {
    let signature_cid = address(&"c11-signature");
    let bop_add_cid = address(&"c11:bop_add");
    let bop_logand_cid = address(&"c11:bop_logand");
    let seq_cid = address(&"c11:seq");
    let sizeof_cid = address(&"c11:sizeof_expr");
    let sizeof_type_cid = address(&"c11:sizeof_type");

    let signature = LanguageSignature::new(signature_cid.clone())
        .with_operation("bop_add", bop_add_cid.clone(), ArityShape::set())
        .with_operation(
            "bop_logand",
            bop_logand_cid.clone(),
            ArityShape::named(["left", "right"]),
        )
        .with_operation("seq", seq_cid.clone(), ArityShape::positional(2))
        .with_operation(
            "sizeof_expr",
            sizeof_cid.clone(),
            ArityShape::named_slots([AritySlot::unevaluated("operand")]),
        )
        .with_operation(
            "sizeof_type",
            sizeof_type_cid.clone(),
            ArityShape::named_slots([
                AritySlot::unevaluated("operand").with_slot_sort(SlotSort::Type)
            ]),
        );

    let a = Term::Var {
        name: "a".to_string(),
    };
    let b = Term::Var {
        name: "b".to_string(),
    };

    let add_ab = Term::Op {
        op_cid: bop_add_cid.clone(),
        name: "bop_add".to_string(),
        args: vec![a.clone(), b.clone()],
    };
    let add_ba = Term::Op {
        op_cid: bop_add_cid,
        name: "bop_add".to_string(),
        args: vec![b.clone(), a.clone()],
    };
    assert_eq!(
        signature.term_cid(&add_ab).expect("shape-aware add cid"),
        signature.term_cid(&add_ba).expect("shape-aware add cid"),
        "Set-shaped C11 binary ops sort children by child CID"
    );

    let and_ab = Term::Op {
        op_cid: bop_logand_cid.clone(),
        name: "bop_logand".to_string(),
        args: vec![a.clone(), b.clone()],
    };
    let and_ba = Term::Op {
        op_cid: bop_logand_cid,
        name: "bop_logand".to_string(),
        args: vec![b.clone(), a.clone()],
    };
    assert_ne!(
        signature.term_cid(&and_ab).expect("shape-aware and cid"),
        signature.term_cid(&and_ba).expect("shape-aware and cid"),
        "Named short-circuit slots carry left/right identity"
    );

    let seq_ab = Term::Op {
        op_cid: seq_cid.clone(),
        name: "seq".to_string(),
        args: vec![a.clone(), b.clone()],
    };
    let seq_ba = Term::Op {
        op_cid: seq_cid,
        name: "seq".to_string(),
        args: vec![b.clone(), a.clone()],
    };
    assert_ne!(
        signature.term_cid(&seq_ab).expect("shape-aware seq cid"),
        signature.term_cid(&seq_ba).expect("shape-aware seq cid"),
        "Positional sequence order remains index-bearing"
    );

    let sizeof_a = Term::Op {
        op_cid: sizeof_cid.clone(),
        name: "sizeof_expr".to_string(),
        args: vec![a.clone()],
    };
    let evaluated_sizeof_signature = LanguageSignature::new(signature_cid.clone()).with_operation(
        "sizeof_expr",
        sizeof_cid,
        ArityShape::named(["operand"]),
    );
    assert_ne!(
        signature
            .term_cid(&sizeof_a)
            .expect("unevaluated slot participates in the term cid"),
        evaluated_sizeof_signature
            .term_cid(&sizeof_a)
            .expect("evaluated slot participates in the term cid"),
        "slot evaluation is catalog data, not an operational side channel"
    );

    let sizeof_type_a = Term::Op {
        op_cid: sizeof_type_cid.clone(),
        name: "sizeof_type".to_string(),
        args: vec![a.clone()],
    };
    let term_typed_sizeof_signature = LanguageSignature::new(signature_cid).with_operation(
        "sizeof_type",
        sizeof_type_cid,
        ArityShape::named_slots([AritySlot::unevaluated("operand")]),
    );
    assert_ne!(
        signature
            .term_cid(&sizeof_type_a)
            .expect("slot sort participates in the term cid"),
        term_typed_sizeof_signature
            .term_cid(&sizeof_type_a)
            .expect("default term slot participates in the term cid"),
        "slot sort is catalog data, not inferred from the handler"
    );
}

#[test]
fn core_compose_agrees_with_legacy_function_contract_composition() {
    let inner = pure_identity_contract("inner", "y");
    let outer = pure_identity_contract("outer", "x");
    let expected = compose_function_contracts(&outer, &inner, 0).expect("legacy compose works");

    let a = claim_for_contract(inner);
    let mut b = claim_for_contract(outer);
    b.from.push(a.to.clone());

    let actual = compose(&a, &b).expect("core compose works");
    assert_eq!(actual.contract.cid, expected.cid);
    assert_eq!(actual.contract.canonical_bytes, expected.canonical_bytes);
    assert_eq!(actual.contract.pre, expected.pre);
    assert_eq!(actual.contract.post, expected.post);
}

#[test]
fn core_compose_tracks_source_endpoints_and_input_claims() {
    let inner = pure_identity_contract("inner_sources", "y");
    let outer = pure_identity_contract("outer_sources", "x");

    let mut a = claim_for_contract(inner);
    let source_cid = address(&"source-endpoint");
    a.from.push(source_cid.clone());

    let mut b = claim_for_contract(outer);
    let ambient_cid = address(&"ambient-endpoint");
    b.from.push(a.to.clone());
    b.from.push(ambient_cid.clone());

    let mut expected_from = vec![source_cid, ambient_cid];
    expected_from.sort();

    let mut expected_premises = vec![a.cid(), b.cid()];
    expected_premises.sort();

    let actual = compose(&a, &b).expect("core compose works");
    assert_eq!(actual.from, expected_from);
    assert_eq!(actual.premises, expected_premises);

    let unioned = a
        .union(&b)
        .expect("domain claims union through composition");
    assert_eq!(unioned.from, actual.from);
    assert_eq!(unioned.premises, actual.premises);
    assert_eq!(unioned.to, actual.to);
}

#[test]
fn link_cross_domain_claims_by_shared_contract_cid() {
    let contract = pure_identity_contract("ffi_shared", "x");
    let shared_cid = Cid::try_from(contract.cid.clone()).expect("fixture cid is valid");

    let mut ts_claim = claim_for_contract(contract.clone());
    ts_claim.domain = DomainKind::Other("typescript".to_string());
    ts_claim.artifacts = vec![address(&"ts.proof")];
    ts_claim.from = vec![address(&"typescript-source")];

    let mut c_claim = claim_for_contract(contract);
    c_claim.domain = DomainKind::Other("c".to_string());
    c_claim.artifacts = vec![address(&"c.proof")];
    c_claim.from = vec![address(&"c-source")];

    let linked = link(&[ts_claim.clone(), c_claim.clone()])
        .expect("cross-domain claims link through shared contract cid");
    let mut expected_premises = vec![ts_claim.cid(), c_claim.cid()];
    expected_premises.sort();

    assert_eq!(
        linked.domain,
        DomainKind::Other("linked-program".to_string())
    );
    assert_eq!(linked.to, shared_cid);
    assert_eq!(linked.premises, expected_premises);
    assert_eq!(linked.artifacts.len(), 2);
    assert!(linked.artifacts.contains(&address(&"ts.proof")));
    assert!(linked.artifacts.contains(&address(&"c.proof")));
}

#[test]
fn transform_and_prove_build_a_contract_claim_with_stub_kits() {
    let kit = CKit::default();
    let input = Input::Term(Term::Var {
        name: "x".to_string(),
    });

    let claim = kit.transform(&input).expect("kit transform succeeds");
    assert_eq!(claim.domain, DomainKind::FunctionContract);
    assert!(!claim.artifacts.is_empty());
    assert_eq!(claim.from, vec![address(&input)]);
    assert_eq!(claim.verdict, Verdict::Unresolved);

    let verb_claim = transform(&kit, &input).expect("transform verb delegates to kit");
    assert_eq!(verb_claim.from, claim.from);

    let proved = kit.prove(claim).expect("kit prove accepts a domain claim");
    assert_eq!(
        prove(&kit, verb_claim)
            .expect("prove verb delegates")
            .verdict,
        Verdict::Unknown
    );
    assert_eq!(proved.verdict, Verdict::Unknown);
    assert!(matches!(proved.witness, Some(Witness::Unknown { .. })));
}

#[test]
fn path_is_cidable_language_neutral_algebra() {
    let lift_step = PathAlgebra {
        name: "lift".to_string(),
        kit: "lift-plugin:rust".to_string(),
        inputs: vec![address(&Input::Spec(serde_json::json!({
            "surface": "rust",
            "workspace_root": "/repo"
        })))],
        depends_on: vec![],
        verb: Verb::Transform,
    };
    let mint_step = PathAlgebra {
        name: "mint".to_string(),
        kit: "provekit-mint".to_string(),
        inputs: vec![address(&Input::Spec(serde_json::json!({
            "outDir": "/repo/out"
        })))],
        depends_on: vec!["lift".to_string()],
        verb: Verb::Transform,
    };

    let path = Path {
        algebra: vec![mint_step.clone(), lift_step.clone()],
    };
    let reordered = Path {
        algebra: vec![lift_step, mint_step],
    };

    assert_eq!(path.cid(), reordered.cid());
    assert_eq!(
        address(&Input::Path(Box::new(path.clone()))),
        address(&Input::Path(Box::new(reordered)))
    );
    assert_eq!(
        path.step("mint").expect("mint step").depends_on,
        vec!["lift".to_string()]
    );
}

#[test]
fn path_algebra_default_verb_is_cid_stable_when_omitted() {
    let source_cid = address(&Input::Spec(json!({"surface": "rust"})));
    let step = PathAlgebra {
        name: "lift".to_string(),
        kit: "lift-rust".to_string(),
        inputs: vec![source_cid.clone()],
        depends_on: vec![],
        verb: Verb::default(),
    };
    let without_verb = json!({
        "dependsOn": [],
        "inputs": [source_cid.as_str()],
        "kit": "lift-rust",
        "name": "lift"
    });

    let encoded = serializable_jcs(&step).expect("default verb step serializes");
    assert!(
        !encoded.contains("\"verb\""),
        "default Transform verb must stay absent from canonical JSON: {encoded}"
    );
    assert_eq!(
        serializable_cid(&step).expect("default verb step CID"),
        json_cid(&without_verb).expect("legacy no-verb step CID")
    );

    let decoded: PathAlgebra =
        serde_json::from_value(without_verb).expect("legacy no-verb step parses");
    assert_eq!(decoded.verb, Verb::Transform);
}

#[test]
fn path_algebra_prove_verb_round_trips_and_changes_cid() {
    let claim_cid = address(&Input::Spec(json!({"claim": "input"})));
    let transform_step = PathAlgebra {
        name: "prove".to_string(),
        kit: "prove-stub".to_string(),
        inputs: vec![claim_cid],
        depends_on: vec![],
        verb: Verb::Transform,
    };
    let prove_step = PathAlgebra {
        verb: Verb::Prove,
        ..transform_step.clone()
    };

    let encoded = serializable_jcs(&prove_step).expect("Prove verb step serializes");
    assert!(
        encoded.contains("\"verb\":\"Prove\""),
        "Prove verb must be present in canonical JSON: {encoded}"
    );
    let decoded: PathAlgebra = serde_json::from_str(&encoded).expect("Prove verb step round-trips");

    assert_eq!(decoded.verb, Verb::Prove);
    assert_ne!(
        serializable_cid(&prove_step).expect("Prove verb step CID"),
        serializable_cid(&transform_step).expect("Transform verb step CID")
    );
}

#[test]
fn path_document_round_trips_with_cid_checked_materialized_inputs() {
    let lift_input = Input::Spec(json!({
        "surface": "rust-self-contracts",
        "workspace_root": ".",
        "config_path": ".provekit/config.toml",
        "source_paths": ["."],
        "options": {"layer": "all"}
    }));
    let mint_input = Input::Spec(json!({
        "outDir": "target/provekit-path-doc",
        "options": {"quiet": true}
    }));
    let lift_input_cid = address(&lift_input);
    let mint_input_cid = address(&mint_input);
    let path = Path {
        algebra: vec![
            PathAlgebra {
                name: "lift".to_string(),
                kit: "lift-plugin:rust-self-contracts".to_string(),
                inputs: vec![lift_input_cid.clone()],
                depends_on: vec![],
                verb: Verb::Transform,
            },
            PathAlgebra {
                name: "mint".to_string(),
                kit: "provekit-mint".to_string(),
                inputs: vec![mint_input_cid.clone()],
                depends_on: vec!["lift".to_string()],
                verb: Verb::Transform,
            },
        ],
    };

    let document = PathDocument::from_path_and_inputs(
        path.clone(),
        vec![lift_input.clone(), mint_input.clone()],
    )
    .expect("path document accepts matching inputs");
    let encoded = serde_json::to_string_pretty(&document).expect("path document serializes");
    let decoded: PathDocument = serde_json::from_str(&encoded).expect("path document parses");
    let inputs = decoded
        .materialized_inputs()
        .expect("path document materializes checked inputs");

    assert_eq!(decoded.path.cid(), path.cid());
    assert_eq!(inputs.len(), 2);
    assert!(inputs.iter().any(|(cid, input)| {
        cid == &lift_input_cid
            && matches!(input, Input::Spec(value) if value["surface"] == "rust-self-contracts")
    }));
    assert!(inputs.iter().any(|(cid, input)| {
        cid == &mint_input_cid
            && matches!(input, Input::Spec(value) if value["outDir"] == "target/provekit-path-doc")
    }));
}

#[test]
fn path_document_rejects_materialized_input_cid_mismatch() {
    let declared = Input::Spec(json!({"surface": "declared"}));
    let actual = Input::Spec(json!({"surface": "changed"}));
    let declared_cid = address(&declared);
    let document = PathDocument {
        kind: "provekit-path/v1".to_string(),
        path: Path {
            algebra: vec![PathAlgebra {
                name: "lift".to_string(),
                kit: "lift-plugin:declared".to_string(),
                inputs: vec![declared_cid],
                depends_on: vec![],
                verb: Verb::Transform,
            }],
        },
        inputs: vec![libprovekit::core::PathInputBinding {
            cid: address(&declared),
            input: libprovekit::core::PathInputMaterial::Spec {
                value: match actual {
                    Input::Spec(value) => value,
                    _ => unreachable!(),
                },
            },
        }],
    };

    let error = document
        .materialized_inputs()
        .expect_err("changed materialized input must not satisfy declared cid")
        .to_string();
    assert!(
        error.contains("materialized as"),
        "unexpected path document error: {error}"
    );
}

#[test]
fn path_document_materializes_closed_input_universe() {
    let claim = claim_fixture("path-doc-claim", Verdict::Unresolved);
    let truth = Truth::try_from(claim_fixture("path-doc-truth", Verdict::Proved))
        .expect("proved claim builds truth input");
    let refutation = Refutation::try_from(claim_fixture("path-doc-refutation", Verdict::Refuted))
        .expect("refuted claim builds refutation input");
    let term = term_fixture("path-doc-term");
    let inner_claim = Input::Claim(claim_fixture("path-doc-inner-claim", Verdict::Unresolved));
    let inner_claim_cid = address(&inner_claim);
    let inner_path = one_step_path("inner", vec![inner_claim_cid.clone()]);
    let inputs = vec![
        Input::Source {
            dialect: Dialect::Rust,
            bytes: b"fn main() {}".to_vec(),
        },
        Input::Spec(json!({"surface": "rust", "outDir": "target/path-doc"})),
        Input::Claim(claim),
        Input::Truth(truth),
        Input::Refutation(refutation),
        Input::Term(term),
        Input::Path(Box::new(inner_path)),
        inner_claim,
    ];
    let path = one_step_path("outer", inputs.iter().map(address).collect());

    let document = PathDocument::from_path_and_inputs(path.clone(), inputs.clone())
        .expect("path document accepts every closed input variant");
    let encoded = serde_json::to_string_pretty(&document).expect("path document serializes");
    let decoded: PathDocument = serde_json::from_str(&encoded).expect("path document parses");
    let materialized = decoded
        .materialized_inputs()
        .expect("path document materializes checked inputs");

    assert_eq!(decoded.path.cid(), path.cid());
    for expected in &inputs {
        let expected_cid = address(expected);
        let (_, actual) = materialized
            .iter()
            .find(|(cid, _)| cid == &expected_cid)
            .expect("expected materialized input cid");
        assert_eq!(actual.canonical_bytes(), expected.canonical_bytes());
    }
    for kind in [
        "\"kind\": \"source\"",
        "\"kind\": \"spec\"",
        "\"kind\": \"claim\"",
        "\"kind\": \"truth\"",
        "\"kind\": \"refutation\"",
        "\"kind\": \"term\"",
        "\"kind\": \"path\"",
    ] {
        assert!(
            encoded.contains(kind),
            "missing materialized variant {kind}"
        );
    }
}

#[test]
fn path_document_rejects_materialized_cid_mismatch_for_claim_term_and_nested_path() {
    fn assert_mismatch(declared: Input, actual: Input) {
        let declared_cid = address(&declared);
        let path = one_step_path("mismatch", vec![declared_cid.clone()]);
        let mut document = PathDocument::from_path_and_inputs(path, vec![actual])
            .expect("path document accepts supported materialized input");
        document.inputs[0].cid = declared_cid;

        let error = document
            .materialized_inputs()
            .expect_err("changed materialized input must not satisfy declared cid");
        assert!(
            matches!(error, PathDocumentError::InputCidMismatch { .. }),
            "unexpected path document error: {error}"
        );
    }

    assert_mismatch(
        Input::Claim(claim_fixture("declared-claim", Verdict::Unresolved)),
        Input::Claim(claim_fixture("actual-claim", Verdict::Unresolved)),
    );
    assert_mismatch(
        Input::Term(term_fixture("declared-term")),
        Input::Term(term_fixture("actual-term")),
    );
    assert_mismatch(
        Input::Path(Box::new(one_step_path(
            "declared-inner",
            vec![address(&Input::Spec(json!({"input": "declared"})))],
        ))),
        Input::Path(Box::new(one_step_path(
            "actual-inner",
            vec![address(&Input::Spec(json!({"input": "actual"})))],
        ))),
    );
}

#[test]
fn path_document_requires_nested_path_claim_closure() {
    let inner_claim = Input::Claim(claim_fixture("missing-inner-claim", Verdict::Unresolved));
    let inner_path = Input::Path(Box::new(one_step_path(
        "inner",
        vec![address(&inner_claim)],
    )));
    let outer_path = one_step_path("outer", vec![address(&inner_path)]);
    let document = PathDocument::from_path_and_inputs(outer_path, vec![inner_path])
        .expect("path document can carry a nested path input");

    let error = document
        .materialized_inputs()
        .expect_err("nested path input claim must be materialized");
    assert!(
        matches!(error, PathDocumentError::MissingMaterializedInput { .. }),
        "unexpected path document error: {error}"
    );
}

#[test]
fn nested_path_cid_changes_when_inner_path_or_cited_claim_changes() {
    fn outer_for(inner_path: Path, claim: &Input) -> Path {
        one_step_path(
            "outer",
            vec![address(&Input::Path(Box::new(inner_path))), address(claim)],
        )
    }

    let claim_a = Input::Claim(claim_fixture("nested-claim-a", Verdict::Unresolved));
    let claim_b = Input::Claim(claim_fixture("nested-claim-b", Verdict::Unresolved));
    let inner_a = one_step_path("inner", vec![address(&claim_a)]);
    let inner_changed = one_step_path("inner-changed", vec![address(&claim_a)]);

    let outer_a = outer_for(inner_a.clone(), &claim_a);
    let outer_inner_changed = outer_for(inner_changed, &claim_a);
    let outer_claim_changed = outer_for(one_step_path("inner", vec![address(&claim_b)]), &claim_b);

    assert_ne!(
        address(&Input::Path(Box::new(outer_a.clone()))),
        address(&Input::Path(Box::new(outer_inner_changed))),
        "outer path input cid must include nested path identity"
    );
    assert_ne!(
        address(&Input::Path(Box::new(outer_a))),
        address(&Input::Path(Box::new(outer_claim_changed))),
        "outer path input cid must include cited claim identity"
    );
}

#[test]
fn path_derives_order_from_dependencies() {
    let lift_step = PathAlgebra {
        name: "lift".to_string(),
        kit: "lift-plugin:rust".to_string(),
        inputs: vec![address(&Input::Spec(
            serde_json::json!({"surface": "rust"}),
        ))],
        depends_on: vec![],
        verb: Verb::Transform,
    };
    let mint_step = PathAlgebra {
        name: "mint".to_string(),
        kit: "provekit-mint".to_string(),
        inputs: vec![address(&Input::Spec(serde_json::json!({"outDir": "out"})))],
        depends_on: vec!["lift".to_string()],
        verb: Verb::Transform,
    };
    let path = Path {
        algebra: vec![mint_step, lift_step],
    };

    let ordered_names: Vec<&str> = path
        .ordered_steps()
        .expect("path dependency order")
        .into_iter()
        .map(|step| step.name.as_str())
        .collect();
    assert_eq!(ordered_names, vec!["lift", "mint"]);

    let terminal_names: Vec<&str> = path
        .terminal_steps()
        .into_iter()
        .map(|step| step.name.as_str())
        .collect();
    assert_eq!(terminal_names, vec!["mint"]);
}

#[test]
fn input_catalog_materializes_path_step_inputs_by_cid() {
    let mut catalog = HashMapInputCatalog::default();
    let input = Input::Spec(serde_json::json!({
        "surface": "jvm-bytecode",
        "artifact": "target/classes/App.class"
    }));
    let input_cid = catalog.insert(input);
    let path = Path {
        algebra: vec![PathAlgebra {
            name: "lift-jvm".to_string(),
            kit: "lift-plugin:jvm-bytecode".to_string(),
            inputs: vec![input_cid.clone()],
            depends_on: vec![],
            verb: Verb::Transform,
        }],
    };

    let step = path.step("lift-jvm").expect("path step");
    let materialized = catalog
        .get_input(&step.inputs[0])
        .expect("input materializes from catalog");
    let Input::Spec(value) = materialized else {
        panic!("catalog returned unexpected input");
    };
    assert_eq!(value["surface"].as_str(), Some("jvm-bytecode"));
    assert_eq!(step.inputs, vec![input_cid]);
}

#[test]
fn path_rejects_invalid_dependency_graphs() {
    let step = |name: &str, depends_on: Vec<&str>| PathAlgebra {
        name: name.to_string(),
        kit: format!("kit:{name}"),
        inputs: vec![address(&format!("input:{name}"))],
        depends_on: depends_on.into_iter().map(str::to_string).collect(),
        verb: Verb::Transform,
    };

    let duplicate = Path {
        algebra: vec![step("same", vec![]), step("same", vec![])],
    };
    assert!(matches!(
        duplicate.ordered_steps(),
        Err(PathError::DuplicateStep { name }) if name == "same"
    ));

    let missing = Path {
        algebra: vec![step("mint", vec!["lift"])],
    };
    assert!(matches!(
        missing.ordered_steps(),
        Err(PathError::MissingDependency { step, dependency })
            if step == "mint" && dependency == "lift"
    ));

    let cycle = Path {
        algebra: vec![step("a", vec!["b"]), step("b", vec!["a"])],
    };
    assert!(matches!(
        cycle.ordered_steps(),
        Err(PathError::Cycle { step }) if step == "a" || step == "b"
    ));
}

#[test]
fn path_orders_cross_domain_link_after_shared_contract_proofs() {
    let shared_contract = address(&"shared-ts-c-contract");
    let ts_proof = PathAlgebra {
        name: "ts-proof".to_string(),
        kit: "lift-plugin:typescript".to_string(),
        inputs: vec![address(&Input::Spec(serde_json::json!({
            "proofFile": "ts.proof",
            "contractCid": shared_contract.as_str()
        })))],
        depends_on: vec![],
        verb: Verb::Transform,
    };
    let c_proof = PathAlgebra {
        name: "c-proof".to_string(),
        kit: "lift-plugin:c".to_string(),
        inputs: vec![address(&Input::Spec(serde_json::json!({
            "proofFile": "c.proof",
            "contractCid": shared_contract.as_str()
        })))],
        depends_on: vec![],
        verb: Verb::Transform,
    };
    let link = PathAlgebra {
        name: "link".to_string(),
        kit: "provekit-link".to_string(),
        inputs: vec![address(&Input::Spec(serde_json::json!({
            "sharedContractCid": shared_contract.as_str()
        })))],
        depends_on: vec!["ts-proof".to_string(), "c-proof".to_string()],
        verb: Verb::Transform,
    };
    let path = Path {
        algebra: vec![link, ts_proof, c_proof],
    };

    let ordered_names: Vec<&str> = path
        .ordered_steps()
        .expect("cross-domain path order")
        .into_iter()
        .map(|step| step.name.as_str())
        .collect();
    assert_eq!(ordered_names.last(), Some(&"link"));
    assert!(ordered_names[..2].contains(&"ts-proof"));
    assert!(ordered_names[..2].contains(&"c-proof"));
    assert_eq!(
        path.terminal_steps()
            .into_iter()
            .map(|step| step.name.as_str())
            .collect::<Vec<_>>(),
        vec!["link"]
    );
}

#[test]
fn verify_truth_checks_catalog_bytes_and_check_mode_witness() {
    let domain = FunctionContractDomain;
    let mut claim = claim_for_contract(pure_identity_contract("truthy", "x"));
    claim.verdict = Verdict::Proved;
    claim.witness = Some(Witness::Proof {
        tree: serde_json::json!({"checked": true}),
    });
    let truth = Truth::try_from(claim.clone()).expect("proved claim is truth");
    let mut catalog = HashMapCatalog::default();
    let cid = catalog.insert(&claim);
    assert_eq!(cid, claim.cid());

    assert!(verify(&truth, &domain, &catalog));

    let mut stale_catalog = HashMapCatalog::default();
    stale_catalog.put(cid, claim.canonical_bytes());
    claim.from.push(address(&"drifted-input"));
    let drifted_truth = Truth::try_from(claim).expect("fixture remains proved");
    assert!(!verify(&drifted_truth, &domain, &stale_catalog));
}

#[test]
fn truth_and_refutation_are_valid_inputs() {
    let mut truth_claim = claim_for_contract(pure_identity_contract("truth_input", "x"));
    truth_claim.verdict = Verdict::Proved;
    truth_claim.witness = Some(Witness::Proof {
        tree: serde_json::json!({"ok": true}),
    });
    let truth = Truth::try_from(truth_claim).expect("truth");
    let input = Input::Truth(truth.clone());
    let Input::Truth(round_tripped) = input else {
        panic!("truth input changed variant");
    };
    assert_eq!(round_tripped.claim().verdict, Verdict::Proved);
    assert!(Truth::try_from(claim_for_contract(pure_identity_contract("not_truth", "x"))).is_err());

    let mut refuted_claim = claim_for_contract(pure_identity_contract("refuted_input", "x"));
    refuted_claim.verdict = Verdict::Refuted;
    refuted_claim.witness = Some(Witness::Counterexample {
        model: serde_json::json!({"x": 0}),
    });
    let refutation = Refutation::try_from(refuted_claim).expect("refutation");
    let input = Input::Refutation(refutation.clone());
    let Input::Refutation(round_tripped) = input else {
        panic!("refutation input changed variant");
    };
    assert_eq!(round_tripped.claim().verdict, Verdict::Refuted);
    assert!(
        Refutation::try_from(claim_for_contract(pure_identity_contract(
            "not_refutation",
            "x"
        )))
        .is_err()
    );
}

#[test]
fn resolve_reads_canonical_bytes_from_hash_map_catalog() {
    let claim = claim_for_contract(pure_identity_contract("cataloged", "x"));
    let mut catalog = HashMapCatalog::default();
    let cid = catalog.insert(&claim);

    let bytes = libprovekit::core::resolve(&cid, &catalog).expect("claim bytes resolve");
    assert_eq!(bytes, claim.canonical_bytes());
}

#[test]
fn hash_map_catalog_contains_reports_membership_without_get_fetch() {
    let claim = claim_for_contract(pure_identity_contract("catalog_contains", "x"));
    let mut catalog = HashMapCatalog::default();
    let present = catalog.insert(&claim);
    let absent = address(&"missing catalog entry");

    assert!(<HashMapCatalog as Catalog>::contains(&catalog, &present));
    assert!(!<HashMapCatalog as Catalog>::contains(&catalog, &absent));

    let traits_source = include_str!("../src/core/traits.rs");
    let impl_body = traits_source
        .split("impl Catalog for HashMapCatalog {")
        .nth(1)
        .and_then(|tail| tail.split("/// Typed resolver").next())
        .expect("HashMapCatalog Catalog impl is present");
    assert!(impl_body.contains("self.entries.contains_key(cid)"));
    assert!(!impl_body.contains("self.get(cid)"));
}

#[test]
fn lift_plugin_transport_is_a_core_kit_with_legacy_response_escape_hatch() {
    let temp =
        std::env::temp_dir().join(format!("provekit-lift-plugin-kit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).expect("create temp dir");
    let script = temp.join("fake-lifter.sh");
    std::fs::write(
        &script,
        r#"#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*) echo '{"jsonrpc":"2.0","id":1,"result":{"name":"fake-lifter"}}' ;;
    *'"method":"lift"'*) echo '{"jsonrpc":"2.0","id":2,"result":{"kind":"ir-document","ir":[],"diagnostics":[]}}' ;;
    *'"method":"shutdown"'*) exit 0 ;;
  esac
done
"#,
    )
    .expect("write fake lifter");

    let kit = LiftPluginKit::new(
        "test-surface",
        vec!["sh".to_string(), script.display().to_string()],
        Some(temp.clone()),
    );
    let input = Input::Spec(serde_json::json!({
        "surface": "test-surface",
        "workspace_root": temp,
        "config_path": ".provekit/config.toml",
        "source_paths": ["."],
        "options": {"layer": "all", "identifyOnly": false}
    }));

    let session = kit
        .parse_session(&input)
        .expect("session exposes transport metadata");
    assert_eq!(
        session.response().get("kind").and_then(|v| v.as_str()),
        Some("ir-document")
    );
    assert_eq!(
        session.claim.domain,
        DomainKind::Other("lift-plugin".to_string())
    );
    assert_eq!(session.claim.from, vec![address(&input)]);
    let response_term = Term::Const {
        value: session.response().clone(),
        sort: Sort::Primitive {
            name: "LiftPluginResponse".to_string(),
        },
    };
    assert_eq!(session.claim.to, address(&response_term));
    assert_eq!(session.claim.artifacts, vec![address(&response_term)]);
    assert_eq!(
        session.claim.contract.body_cid.as_deref(),
        Some(session.claim.to.as_str())
    );
    assert_eq!(
        session.response().get("kind").and_then(|v| v.as_str()),
        Some("ir-document")
    );
    let _ = std::fs::remove_dir_all(&temp);
}

#[test]
fn lift_kit_transforms_source_through_lift_plugin_transport_and_carries_term_payload() {
    let temp =
        std::env::temp_dir().join(format!("provekit-lift-kit-source-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).expect("create temp dir");
    let script = temp.join("fake-lifter.sh");
    std::fs::write(
        &script,
        r#"#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*) echo '{"jsonrpc":"2.0","id":1,"result":{"name":"fake-rust-lifter"}}' ;;
    *'"method":"lift"'*) echo '{"jsonrpc":"2.0","id":2,"result":{"kind":"ir-document","ir":[{"kind":"bind-lift-entry","file":"src/lib.rs","fn_name":"id","fn_line":1,"param_names":["x"],"param_types":["i64"],"return_type":"i64","term_shape":{"kind":"var","name":"x"},"term_shape_cid":"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","witnesses":[]}],"diagnostics":[]}}' ;;
    *'"method":"shutdown"'*) exit 0 ;;
  esac
done
"#,
    )
    .expect("write fake lifter");

    let request = serde_json::json!({
        "surface": "rust",
        "workspace_root": temp,
        "config_path": ".provekit/config.toml",
        "source_paths": ["."],
        "options": {"layer": "all", "identifyOnly": false}
    });
    let source = Input::Source {
        dialect: libprovekit::core::Dialect::Rust,
        bytes: serde_json::to_vec(&request).expect("source request JSON"),
    };
    let kit = LiftKit::new(
        libprovekit::core::Dialect::Rust,
        "rust",
        vec!["sh".to_string(), script.display().to_string()],
        Some(temp.clone()),
    );

    let claim = kit
        .transform(&source)
        .expect("source input lifts through the transport");
    let expected_term = Term::Const {
        value: serde_json::json!({
            "kind": "ir-document",
            "ir": [{
                "kind": "bind-lift-entry",
                "file": "src/lib.rs",
                "fn_name": "id",
                "fn_line": 1,
                "param_names": ["x"],
                "param_types": ["i64"],
                "return_type": "i64",
                "term_shape": {"kind": "var", "name": "x"},
                "term_shape_cid": "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "witnesses": []
            }],
            "diagnostics": []
        }),
        sort: Sort::Primitive {
            name: "LiftPluginResponse".to_string(),
        },
    };

    assert_eq!(claim.to, address(&expected_term));
    assert_eq!(claim.artifacts, vec![address(&expected_term)]);
    assert_eq!(claim.payload.as_ref(), Some(&expected_term));
    assert_eq!(claim.from, vec![address(&source)]);
    let _ = std::fs::remove_dir_all(&temp);
}

#[test]
fn execute_path_refuses_unregistered_lift_kit_with_composition_refusal_memento() {
    let source = Input::Source {
        dialect: libprovekit::core::Dialect::Other("unknown".to_string()),
        bytes: b"fn id(x: i64) -> i64 { x }".to_vec(),
    };
    let mut inputs = HashMapInputCatalog::default();
    let source_cid = inputs.insert(source);
    let path = Input::Path(Box::new(Path {
        algebra: vec![PathAlgebra {
            name: "lift".to_string(),
            kit: "lift-unknown".to_string(),
            inputs: vec![source_cid],
            depends_on: vec![],
            verb: Verb::Transform,
        }],
    }));
    let registry = KitRegistry::default();

    let err = execute_path(&path, &registry, &inputs).expect_err("unknown lift kit refuses");
    let refusal = err
        .composition_refusal()
        .expect("path executor error carries composition refusal");
    assert_eq!(refusal.header.failure_kind, "memento-required-missing");
    assert!(refusal
        .header
        .missing_memento_requirements
        .as_ref()
        .expect("missing requirements")
        .iter()
        .any(|requirement| requirement.role.as_deref() == Some("kit-registry")));
}

#[test]
fn kit_registry_register_requires_and_exposes_conformance_declaration() {
    fn register_with_declaration(
        registry: &mut KitRegistry,
        name: &str,
        kit: impl Kit + 'static,
        conformance: ConformanceDeclaration,
    ) {
        registry.register(name, kit, conformance);
    }

    let conformance = ConformanceDeclaration::NonCarrier {
        reason: "lifts source bytes to DomainClaim; no target source produced",
    };
    let mut registry = KitRegistry::default();

    register_with_declaration(
        &mut registry,
        "lift-rust",
        LiftKit::new(Dialect::Rust, "rust", vec!["true".to_string()], None),
        conformance.clone(),
    );

    assert_eq!(registry.conformance("lift-rust"), Some(&conformance));
    assert_eq!(registry.conformance("unknown"), None);
}

#[test]
fn conformance_declaration_round_trips_through_json() {
    fn decode(encoded: String) -> ConformanceDeclaration {
        let encoded: &'static str = Box::leak(encoded.into_boxed_str());
        serde_json::from_str(encoded).expect("conformance declaration decodes")
    }

    let carrier = ConformanceDeclaration::Carrier {
        fixtures_path: PathBuf::from("implementations/rust/conformance/fixtures"),
        platform_semantics: None,
    };
    let non_carrier = ConformanceDeclaration::NonCarrier {
        reason: "lifts source bytes to DomainClaim; no target source produced",
    };

    let carrier_json = serde_json::to_string(&carrier).expect("carrier serializes");
    let non_carrier_json = serde_json::to_string(&non_carrier).expect("non-carrier serializes");

    assert_eq!(decode(carrier_json), carrier);
    assert_eq!(decode(non_carrier_json), non_carrier);
}

#[test]
fn carrier_registration_preserves_platform_semantics_declaration() {
    let wrapping = DimensionValueMemento::new(
        "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111".to_string(),
        "ArithmeticOverflowMode".to_string(),
        "Wrapping".to_string(),
        bool_true(),
    );
    let truncate = DimensionValueMemento::new(
        "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111".to_string(),
        "IntegerDivisionRoundingMode".to_string(),
        "Truncate".to_string(),
        bool_true(),
    );
    let arbitrary_precision = DimensionValueMemento::new(
        "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111".to_string(),
        "ArithmeticOverflowMode".to_string(),
        "ArbitraryPrecision".to_string(),
        bool_true(),
    );
    let first_tag = platform_tag(
        "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        vec![
            ("ArithmeticOverflowMode", wrapping.cid.clone()),
            ("IntegerDivisionRoundingMode", truncate.cid.clone()),
        ],
    );
    let second_tag = platform_tag(
        "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        vec![("ArithmeticOverflowMode", arbitrary_precision.cid.clone())],
    );
    let declaration = PlatformSemanticsDeclaration {
        tags: vec![first_tag.clone(), second_tag.clone()],
        dimension_values: vec![wrapping, truncate, arbitrary_precision],
        op_aliases: BTreeMap::new(),
    };
    let mut registry = KitRegistry::default();

    registry.register(
        "semantic-kit",
        LiftKit::new(Dialect::Rust, "rust", vec!["true".to_string()], None),
        ConformanceDeclaration::Carrier {
            fixtures_path: PathBuf::from("fixtures/semantic-kit"),
            platform_semantics: Some(declaration.clone()),
        },
    );

    let Some(ConformanceDeclaration::Carrier {
        platform_semantics: Some(retrieved),
        ..
    }) = registry.conformance("semantic-kit")
    else {
        panic!("semantic-kit carrier declaration must preserve platform semantics");
    };

    assert_eq!(retrieved, &declaration);
    let encoded = retrieved.tags[0].to_jcs_string();
    let decoded: PlatformSemanticTag = serde_json::from_str(&encoded).expect("tag decodes");
    assert_eq!(
        decoded.dimensions.keys().collect::<Vec<_>>(),
        first_tag.dimensions.keys().collect::<Vec<_>>()
    );
    assert_eq!(decoded.to_jcs_string(), encoded);
}

#[test]
fn dispatcher_returns_python_platform_semantics_declaration() {
    let declaration = platform_semantics_for_lower_target("python")
        .expect("python lower target declares platform semantics");
    assert_eq!(
        declaration,
        libprovekit::core::platform_semantics::python_realize_core::declaration()
    );
    assert_python_platform_semantics(&declaration);
}

#[test]
fn python_platform_semantics_wrappers_share_locked_dimension_values() {
    let lift = libprovekit::core::platform_semantics::python_lift_source::declaration();
    let realize = libprovekit::core::platform_semantics::python_realize_core::declaration();
    assert_eq!(lift, realize);
    assert_python_platform_semantics(&lift);

    let values = libprovekit::core::platform_semantics::python_realize_core::dimension_values();
    assert_eq!(values.len(), PYTHON_PLATFORM_DIMENSIONS.len());
    for (dimension, value_name) in PYTHON_PLATFORM_DIMENSIONS {
        let value = values
            .iter()
            .find(|candidate| {
                candidate.dimension_name == *dimension && candidate.value_name == *value_name
            })
            .unwrap_or_else(|| panic!("missing Python dimension value {dimension}:{value_name}"));
        assert_eq!(
            value.compare_to,
            IrFormula::Atomic {
                name: format!("python:{value_name}"),
                args: vec![],
            }
        );
    }
}

#[test]
fn dispatcher_returns_rust_platform_semantics_declaration() {
    let declaration =
        platform_semantics_for_lower_target("rust").expect("rust semantics are declared");

    assert_eq!(declaration.tags.len(), 21);
    assert!(declaration
        .tags
        .iter()
        .any(|tag| { tag.dimensions.contains_key("ArithmeticOverflow") }));
    assert!(declaration
        .tags
        .iter()
        .any(|tag| { tag.dimensions.contains_key("IntegerDivisionRounding") }));
    assert!(declaration
        .tags
        .iter()
        .any(|tag| tag.dimensions.contains_key("ShiftMode")));
    assert!(declaration
        .tags
        .iter()
        .any(|tag| tag.dimensions.contains_key("NullSemantics")));
    assert!(declaration
        .tags
        .iter()
        .any(|tag| tag.dimensions.contains_key("BitwiseSemantics")));
}

#[test]
fn dispatcher_still_returns_none_for_unclaimed_targets() {
    for target in ["unknown"] {
        assert_eq!(platform_semantics_for_lower_target(target), None);
    }
}

#[test]
fn dispatcher_returns_java_platform_semantics_declaration() {
    const ADD: &str = "blake3-512:95fc70e63a5550fd2e25142f13932919c59d085654ab387789c798886b0111c61d28fe533fc98b50df70eea9428a9af8aa75372c8b1c1deb3acc1a4094790468";
    const SUB: &str = "blake3-512:b7c54558573348bb3a9297732547a8e6e9d152403d292df7426b6bb8a248f705b4b030bf2a22ba547a17d6f1bfaf8e75a6843e02e8f23a8226ebc09e2a8622af";
    const MUL: &str = "blake3-512:46cd627de058c8d4f7d087ea33f4904af65ad4b2e3cfd3aff8f44bf27db96b33c2dae39cd30f53898c233c9465ba8d2701c69e5903d48935113103b4db00fd03";
    const NEG: &str = "blake3-512:ad958847b50cf07ddbb92d85ae488a5f983d5619e108476b42e519174cfcce883ecd637544a372b946bb45a1c22893c710bc9b08ea0569ad0e035b3babb6a409";
    const DIV: &str = "blake3-512:c6a13abbcafdf83edcff49d883a7c7440faadd8af896da0ad46e2bcb177ed0649d005b4ddecd4689cf565b10679219a07c784399bafe5c6174642e1b808d7839";
    const MOD: &str = "blake3-512:92340897b43965e01454b00a6a43ec54b2bf0e01213a45fa2311f730dde18adf8da97a22458c1a2a0fb23ce85ef3ad9b22e704804c74f41997aba3ba02cefe0d";
    const SHL: &str = "blake3-512:f9cdfcba8d0e223803126504a2a6ed10005fa61acb5c55b74b270bc66d963eb7648ab6763f0510760df93145c0f6670087a403417e8b3100c7142e121111807a";
    const SHR: &str = "blake3-512:c90e3c159b25e4c4c7f9c899da5aa3ee048a548719ced7360f3e514450811096b21cd5473f22d7a05df088f92210bbc916e65970b9fa1e1511c193ed969f112b";
    const USHR: &str = "blake3-512:5746cb4f8bb8d713624731661de51e851e7ca65dae10a88bae4727d1e0070525be77e9919d90939264acaf4c093b00808862e6d0d2c24ac05262ce95cd67c8ad";
    const BITNOT: &str = "blake3-512:5e788f0d551081f4e709e4418e01017fa9ae1c04963e7be2862fadad8a8434fafa204629fbec53e2e44624c195ac2e32c0410df25cf8ff3a4be672582f89109f";

    let declaration = platform_semantics_for_lower_target("java")
        .expect("java lower target must declare platform semantics");
    assert_eq!(
        declaration,
        libprovekit::core::platform_semantics::java::declaration()
    );
    let kit_cid = blake3_512_of(b"provekit-realize-java-core@0.1.0");
    let wrapping = java_dimension_value(&kit_cid, "ArithmeticOverflow", "Wrapping");
    let truncate = java_dimension_value(&kit_cid, "IntegerDivisionRounding", "Truncate");
    let arithmetic = java_dimension_value(&kit_cid, "ShiftMode", "Arithmetic");
    let logical = java_dimension_value(&kit_cid, "ShiftMode", "Logical");
    let throw_arithmetic =
        java_dimension_value(&kit_cid, "NullSemantics", "ThrowArithmeticException");
    let twos_complement = java_dimension_value(&kit_cid, "BitwiseSemantics", "TwosComplement");

    assert_eq!(declaration.tags.len(), 10);
    assert_java_tag(
        &declaration,
        ADD,
        &kit_cid,
        &[("ArithmeticOverflow", &wrapping.cid)],
    );
    assert_java_tag(
        &declaration,
        SUB,
        &kit_cid,
        &[("ArithmeticOverflow", &wrapping.cid)],
    );
    assert_java_tag(
        &declaration,
        MUL,
        &kit_cid,
        &[("ArithmeticOverflow", &wrapping.cid)],
    );
    assert_java_tag(
        &declaration,
        NEG,
        &kit_cid,
        &[("ArithmeticOverflow", &wrapping.cid)],
    );
    assert_java_tag(
        &declaration,
        DIV,
        &kit_cid,
        &[
            ("IntegerDivisionRounding", &truncate.cid),
            ("NullSemantics", &throw_arithmetic.cid),
        ],
    );
    assert_java_tag(
        &declaration,
        MOD,
        &kit_cid,
        &[
            ("IntegerDivisionRounding", &truncate.cid),
            ("NullSemantics", &throw_arithmetic.cid),
        ],
    );
    assert_java_tag(
        &declaration,
        SHL,
        &kit_cid,
        &[("BitwiseSemantics", &twos_complement.cid)],
    );
    assert_java_tag(
        &declaration,
        SHR,
        &kit_cid,
        &[
            ("BitwiseSemantics", &twos_complement.cid),
            ("ShiftMode", &arithmetic.cid),
        ],
    );
    assert_java_tag(
        &declaration,
        USHR,
        &kit_cid,
        &[
            ("BitwiseSemantics", &twos_complement.cid),
            ("ShiftMode", &logical.cid),
        ],
    );
    assert_java_tag(
        &declaration,
        BITNOT,
        &kit_cid,
        &[("BitwiseSemantics", &twos_complement.cid)],
    );
}

#[test]
fn dispatcher_returns_c_platform_semantics_declaration() {
    let declaration = platform_semantics_for_lower_target("c")
        .expect("c lower target declares platform semantics");
    assert!(
        !declaration.tags.is_empty(),
        "c declaration must include per-op platform semantic tags"
    );

    let expected_dimensions = BTreeSet::from([
        "ArithmeticOverflow".to_string(),
        "BitwiseSemantics".to_string(),
        "IntegerDivisionRounding".to_string(),
        "NullSemantics".to_string(),
        "ShiftMode".to_string(),
    ]);

    for tag in &declaration.tags {
        assert_eq!(
            tag.dimensions.keys().cloned().collect::<BTreeSet<_>>(),
            expected_dimensions
        );
        assert_eq!(tag.cid, tag.recompute_cid());
        for value_cid in tag.dimensions.values() {
            assert!(value_cid.starts_with("blake3-512:"));
            assert_eq!(value_cid.len(), "blake3-512:".len() + 128);
        }
    }

    let op_cids = declaration
        .tags
        .iter()
        .map(|tag| tag.op_cid.as_str())
        .collect::<BTreeSet<_>>();
    for required in [
        "blake3-512:95fc70e63a5550fd2e25142f13932919c59d085654ab387789c798886b0111c61d28fe533fc98b50df70eea9428a9af8aa75372c8b1c1deb3acc1a4094790468", // concept:add
        "blake3-512:c6a13abbcafdf83edcff49d883a7c7440faadd8af896da0ad46e2bcb177ed0649d005b4ddecd4689cf565b10679219a07c784399bafe5c6174642e1b808d7839", // concept:div
        "blake3-512:c90e3c159b25e4c4c7f9c899da5aa3ee048a548719ced7360f3e514450811096b21cd5473f22d7a05df088f92210bbc916e65970b9fa1e1511c193ed969f112b", // concept:shr
        "blake3-512:9e96c2445bad6bb1e5a6f902ad7f733e3f4619829b9c0e232361fbf50b978c8332029212ed895762e604d1df009fce58848cda33524a697df798233eae30a14b", // concept:bitand
        "blake3-512:93ff252a879bc061949fecdb9710a0a927b47f5104f5e628c7e0bd2477e3ea3515ebb2bc2794d9cc7c11c6ea16db511ff20a18c699bb94f7854e79b5e195f717", // concept:deref
    ] {
        assert!(op_cids.contains(required), "missing C semantics tag for {required}");
    }
}

#[test]
fn platform_semantics_compare_op_reports_open_keyed_dimension_divergence() {
    let source = platform_semantics_for_lower_target("python")
        .expect("python lower target declares platform semantics");
    let target =
        platform_semantics_for_lower_target("rust").expect("rust lower target declares semantics");
    let div = PYTHON_PLATFORM_CONCEPT_OP_CIDS[3];

    let verdict = source
        .compare_op_with(div, &target)
        .expect("no internal substrate inconsistency");
    let divergence = match verdict {
        OpCoverageVerdict::Divergent(d) => d,
        other => panic!("expected Divergent, got {other:?}"),
    };

    assert_eq!(divergence.dimension_name, "IntegerDivisionRounding");
    assert_ne!(divergence.source_value_cid, divergence.target_value_cid);
    assert_eq!(
        divergence.source_compare_to,
        IrFormula::Atomic {
            name: "python:Floor".to_string(),
            args: vec![],
        }
    );
    assert_eq!(
        divergence.target_compare_to,
        IrFormula::Atomic {
            name: "rust:Truncate".to_string(),
            args: vec![],
        }
    );
}

#[test]
fn platform_semantics_compare_op_preserves_exact_same_kit_leg() {
    let source = platform_semantics_for_lower_target("python")
        .expect("python lower target declares platform semantics");
    let div = PYTHON_PLATFORM_CONCEPT_OP_CIDS[3];

    assert_eq!(
        source
            .compare_op_with(div, &source)
            .expect("same-kit comparison has no internal inconsistency"),
        OpCoverageVerdict::Same,
    );
}

#[test]
fn platform_semantics_compare_op_reports_uncharacterizable_absent_target_op() {
    let source =
        platform_semantics_for_lower_target("rust").expect("rust lower target declares semantics");
    let target = platform_semantics_for_lower_target("java")
        .expect("java lower target declares platform semantics");
    // After the OpCoverageVerdict refactor, a rust-only op yields
    // Ok(Uncharacterizable { absent_on: Target }) rather than Err(TargetOpAbsent).
    let rust_only_op = source
        .tags
        .iter()
        .map(|tag| tag.op_cid.as_str())
        .find(|op_cid| {
            matches!(
                source.compare_op_with(op_cid, &target),
                Ok(OpCoverageVerdict::Uncharacterizable {
                    absent_on: Side::Target
                })
            )
        })
        .expect("fixture has a rust-only native op cid");

    assert!(matches!(
        source.compare_op_with(rust_only_op, &target),
        Ok(OpCoverageVerdict::Uncharacterizable {
            absent_on: Side::Target
        })
    ));
}

// ---------------------------------------------------------------------------
// OpCoverageVerdict variant tests -- three per variant (positive + discrimination + structural)
//
// All four variants: NoOpinion, Uncharacterizable, Same, Divergent.
// Helper: build a minimal PlatformSemanticsDeclaration with one tagged op.
// ---------------------------------------------------------------------------

const TEST_KIT_CID: &str = "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111";
const TEST_OP_A: &str = "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TEST_OP_B: &str = "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn test_dimension_value(value_name: &str) -> DimensionValueMemento {
    DimensionValueMemento::new(
        TEST_KIT_CID.to_string(),
        "TestDimension".to_string(),
        value_name.to_string(),
        IrFormula::Atomic {
            name: format!("test:{value_name}"),
            args: vec![],
        },
    )
}

fn single_op_declaration(op_cid: &str, value_cid: &str, value: DimensionValueMemento) -> PlatformSemanticsDeclaration {
    let tag = platform_tag(op_cid, vec![("TestDimension", value_cid.to_string())]);
    PlatformSemanticsDeclaration {
        tags: vec![tag],
        dimension_values: vec![value],
        op_aliases: BTreeMap::new(),
    }
}

// -- NoOpinion: positive --
#[test]
fn op_coverage_verdict_no_opinion_when_both_kits_absent_for_op() {
    let val_a = test_dimension_value("ValueA");
    let decl_a = single_op_declaration(TEST_OP_A, &val_a.cid.clone(), val_a);
    let val_b = test_dimension_value("ValueB");
    let decl_b = single_op_declaration(TEST_OP_B, &val_b.cid.clone(), val_b);

    let unknown_op = "blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    assert_eq!(
        decl_a.compare_op_with(unknown_op, &decl_b).expect("no internal error"),
        OpCoverageVerdict::NoOpinion,
    );
}

// -- NoOpinion: discrimination -- adding a tag to one kit flips to Uncharacterizable
#[test]
fn op_coverage_verdict_no_opinion_flips_to_uncharacterizable_when_one_kit_gains_tag() {
    let val_a = test_dimension_value("ValueA");
    let decl_with_op = single_op_declaration(TEST_OP_A, &val_a.cid.clone(), val_a);
    let decl_without_op = PlatformSemanticsDeclaration {
        tags: vec![],
        dimension_values: vec![],
        op_aliases: BTreeMap::new(),
    };

    assert_eq!(
        decl_without_op.compare_op_with(TEST_OP_A, &decl_without_op).expect("no error"),
        OpCoverageVerdict::NoOpinion,
    );
    assert!(matches!(
        decl_with_op.compare_op_with(TEST_OP_A, &decl_without_op).expect("no error"),
        OpCoverageVerdict::Uncharacterizable { .. }
    ));
}

// -- NoOpinion: structural -- verdict carries no payload (unit variant)
#[test]
fn op_coverage_verdict_no_opinion_is_unit_variant() {
    let decl_empty = PlatformSemanticsDeclaration {
        tags: vec![],
        dimension_values: vec![],
        op_aliases: BTreeMap::new(),
    };
    let verdict = decl_empty.compare_op_with(TEST_OP_A, &decl_empty).expect("no error");
    assert_eq!(verdict, OpCoverageVerdict::NoOpinion);
    assert_eq!(verdict.clone(), OpCoverageVerdict::NoOpinion);
}

// -- Uncharacterizable: positive -- source has tag, target does not
#[test]
fn op_coverage_verdict_uncharacterizable_absent_on_target() {
    let val_a = test_dimension_value("ValueA");
    let decl_source = single_op_declaration(TEST_OP_A, &val_a.cid.clone(), val_a);
    let decl_target = PlatformSemanticsDeclaration {
        tags: vec![],
        dimension_values: vec![],
        op_aliases: BTreeMap::new(),
    };
    assert_eq!(
        decl_source.compare_op_with(TEST_OP_A, &decl_target).expect("no internal error"),
        OpCoverageVerdict::Uncharacterizable { absent_on: Side::Target },
    );
}

// -- Uncharacterizable: discrimination -- adding target tag flips to Same
#[test]
fn op_coverage_verdict_uncharacterizable_flips_to_same_when_target_gains_identical_tag() {
    let val_a = test_dimension_value("ValueA");
    let decl_source = single_op_declaration(TEST_OP_A, &val_a.cid.clone(), val_a.clone());
    let decl_target_empty = PlatformSemanticsDeclaration {
        tags: vec![],
        dimension_values: vec![],
        op_aliases: BTreeMap::new(),
    };
    assert!(matches!(
        decl_source.compare_op_with(TEST_OP_A, &decl_target_empty).expect("no error"),
        OpCoverageVerdict::Uncharacterizable { absent_on: Side::Target }
    ));
    let decl_target_with_tag = single_op_declaration(TEST_OP_A, &val_a.cid.clone(), val_a);
    assert_eq!(
        decl_source.compare_op_with(TEST_OP_A, &decl_target_with_tag).expect("no error"),
        OpCoverageVerdict::Same,
    );
}

// -- Uncharacterizable: structural -- absent_on carries the correct Side
#[test]
fn op_coverage_verdict_uncharacterizable_absent_on_source_when_only_target_has_tag() {
    let val_a = test_dimension_value("ValueA");
    let decl_source_empty = PlatformSemanticsDeclaration {
        tags: vec![],
        dimension_values: vec![],
        op_aliases: BTreeMap::new(),
    };
    let decl_target = single_op_declaration(TEST_OP_A, &val_a.cid.clone(), val_a);
    let verdict = decl_source_empty.compare_op_with(TEST_OP_A, &decl_target).expect("no error");
    assert!(
        matches!(verdict, OpCoverageVerdict::Uncharacterizable { absent_on: Side::Source }),
        "expected absent_on = Source but got {verdict:?}"
    );
}

// -- Same: positive -- both kits declare identical dimension value CIDs
#[test]
fn op_coverage_verdict_same_when_both_kits_declare_identical_value() {
    let val_a = test_dimension_value("ValueA");
    let decl_a = single_op_declaration(TEST_OP_A, &val_a.cid.clone(), val_a.clone());
    let decl_b = single_op_declaration(TEST_OP_A, &val_a.cid.clone(), val_a);
    assert_eq!(
        decl_a.compare_op_with(TEST_OP_A, &decl_b).expect("no internal error"),
        OpCoverageVerdict::Same,
    );
}

// -- Same: discrimination -- swapping one kit value CID flips to Divergent
#[test]
fn op_coverage_verdict_same_flips_to_divergent_when_value_cids_differ() {
    let val_a = test_dimension_value("ValueA");
    let val_b = test_dimension_value("ValueB");
    let decl_source = single_op_declaration(TEST_OP_A, &val_a.cid.clone(), val_a.clone());
    let decl_same = single_op_declaration(TEST_OP_A, &val_a.cid.clone(), val_a);
    assert_eq!(
        decl_source.compare_op_with(TEST_OP_A, &decl_same).expect("no error"),
        OpCoverageVerdict::Same,
    );
    let decl_divergent = single_op_declaration(TEST_OP_A, &val_b.cid.clone(), val_b);
    assert!(matches!(
        decl_source.compare_op_with(TEST_OP_A, &decl_divergent).expect("no error"),
        OpCoverageVerdict::Divergent(_)
    ));
}

// -- Same: structural -- no payload, unit variant
#[test]
fn op_coverage_verdict_same_is_unit_variant() {
    let val_a = test_dimension_value("ValueA");
    let decl_a = single_op_declaration(TEST_OP_A, &val_a.cid.clone(), val_a.clone());
    let decl_b = single_op_declaration(TEST_OP_A, &val_a.cid.clone(), val_a);
    let verdict = decl_a.compare_op_with(TEST_OP_A, &decl_b).expect("no error");
    assert_eq!(verdict, OpCoverageVerdict::Same);
    assert_eq!(verdict.clone(), OpCoverageVerdict::Same);
}

// -- Divergent: positive -- both kits present with differing value CIDs
#[test]
fn op_coverage_verdict_divergent_when_both_kits_have_differing_values() {
    let val_a = test_dimension_value("ValueA");
    let val_b = test_dimension_value("ValueB");
    let decl_source = single_op_declaration(TEST_OP_A, &val_a.cid.clone(), val_a);
    let decl_target = single_op_declaration(TEST_OP_A, &val_b.cid.clone(), val_b);
    assert!(matches!(
        decl_source.compare_op_with(TEST_OP_A, &decl_target).expect("no internal error"),
        OpCoverageVerdict::Divergent(_)
    ));
}

// -- Divergent: discrimination -- making value CIDs identical flips to Same
#[test]
fn op_coverage_verdict_divergent_flips_to_same_when_value_cids_made_equal() {
    let val_a = test_dimension_value("ValueA");
    let val_b = test_dimension_value("ValueB");
    let decl_source = single_op_declaration(TEST_OP_A, &val_a.cid.clone(), val_a.clone());
    let decl_target_diff = single_op_declaration(TEST_OP_A, &val_b.cid.clone(), val_b);
    assert!(matches!(
        decl_source.compare_op_with(TEST_OP_A, &decl_target_diff).expect("no error"),
        OpCoverageVerdict::Divergent(_)
    ));
    let decl_target_same = single_op_declaration(TEST_OP_A, &val_a.cid.clone(), val_a);
    assert_eq!(
        decl_source.compare_op_with(TEST_OP_A, &decl_target_same).expect("no error"),
        OpCoverageVerdict::Same,
    );
}

// -- Divergent: structural -- payload contains source_compare_to and target_compare_to
#[test]
fn op_coverage_verdict_divergent_carries_both_compare_to_formulas() {
    let val_a = test_dimension_value("ValueA");
    let val_b = test_dimension_value("ValueB");
    let decl_source = single_op_declaration(TEST_OP_A, &val_a.cid.clone(), val_a.clone());
    let decl_target = single_op_declaration(TEST_OP_A, &val_b.cid.clone(), val_b.clone());
    let verdict = decl_source.compare_op_with(TEST_OP_A, &decl_target).expect("no error");
    match verdict {
        OpCoverageVerdict::Divergent(ref d) => {
            assert_eq!(d.dimension_name, "TestDimension");
            assert_eq!(d.source_value_cid, val_a.cid);
            assert_eq!(d.target_value_cid, val_b.cid);
            assert_eq!(
                d.source_compare_to,
                IrFormula::Atomic { name: "test:ValueA".to_string(), args: vec![] }
            );
            assert_eq!(
                d.target_compare_to,
                IrFormula::Atomic { name: "test:ValueB".to_string(), args: vec![] }
            );
        }
        other => panic!("expected Divergent but got {other:?}"),
    }
}

#[test]
fn carrier_fixture_set_requirement_covers_c_emit_compile_run_fixtures() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("libprovekit has rust parent")
        .parent()
        .expect("rust has implementations parent")
        .parent()
        .expect("implementations has repo parent")
        .to_path_buf();
    let fixtures_path = repo_root
        .join("implementations")
        .join("c")
        .join("conformance")
        .join("fixtures");
    let mut registry = KitRegistry::default();
    registry.register(
        "lower-c",
        CKit::default(),
        ConformanceDeclaration::Carrier {
            fixtures_path: fixtures_path.clone(),
            platform_semantics: None,
        },
    );

    assert_carrier_fixture_set(
        &registry,
        "lower-c",
        &[
            "hello_world",
            "recursive_factorial",
            "arithmetic_add",
            "arithmetic_multi_op",
            "control_flow_if",
            "transported_op_concept_comment",
        ],
    );
}

fn assert_carrier_fixture_set(registry: &KitRegistry, kit: &str, required: &[&str]) {
    let Some(ConformanceDeclaration::Carrier { fixtures_path, .. }) = registry.conformance(kit)
    else {
        panic!("{kit} must be registered as a carrier kit");
    };
    let present = fixture_names(fixtures_path);
    let missing = required
        .iter()
        .copied()
        .filter(|name| !present.contains(*name))
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "{kit} carrier fixtures at {} missing required fixture(s): {:?}",
        fixtures_path.display(),
        missing
    );
}

fn platform_tag(op_cid: &str, pairs: Vec<(&str, String)>) -> PlatformSemanticTag {
    let mut dimensions = BTreeMap::new();
    for (dimension, cid) in pairs {
        dimensions.insert(dimension.to_string(), cid);
    }
    PlatformSemanticTag::new(
        "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111".to_string(),
        op_cid.to_string(),
        dimensions,
    )
}

fn assert_python_platform_semantics(declaration: &PlatformSemanticsDeclaration) {
    assert_eq!(
        declaration.tags.len(),
        PYTHON_PLATFORM_CONCEPT_OP_CIDS.len()
    );
    let expected_dimensions = PYTHON_PLATFORM_DIMENSIONS
        .iter()
        .map(|(dimension, _)| dimension.to_string())
        .collect::<BTreeSet<_>>();
    let actual_op_cids = declaration
        .tags
        .iter()
        .map(|tag| {
            assert_eq!(
                tag.dimensions.keys().cloned().collect::<BTreeSet<_>>(),
                expected_dimensions
            );
            tag.op_cid.as_str()
        })
        .collect::<BTreeSet<_>>();
    let expected_op_cids = PYTHON_PLATFORM_CONCEPT_OP_CIDS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    assert_eq!(actual_op_cids, expected_op_cids);
}

fn java_dimension_value(
    kit_cid: &str,
    dimension_name: &str,
    value_name: &str,
) -> DimensionValueMemento {
    DimensionValueMemento::new(
        kit_cid.to_string(),
        dimension_name.to_string(),
        value_name.to_string(),
        IrFormula::Atomic {
            name: format!("java:{value_name}"),
            args: vec![],
        },
    )
}

fn assert_java_tag(
    declaration: &PlatformSemanticsDeclaration,
    op_cid: &str,
    kit_cid: &str,
    pairs: &[(&str, &str)],
) {
    let tag = declaration
        .tags
        .iter()
        .find(|tag| tag.op_cid == op_cid)
        .unwrap_or_else(|| panic!("missing Java platform semantic tag for {op_cid}"));
    let expected = pairs
        .iter()
        .map(|(dimension, cid)| (dimension.to_string(), cid.to_string()))
        .collect::<BTreeMap<_, _>>();

    assert_eq!(tag.kit_cid, kit_cid);
    assert_eq!(tag.dimensions, expected);
    assert_eq!(tag.cid, tag.recompute_cid());
}

fn fixture_names(path: &FsPath) -> BTreeSet<String> {
    let entries = std::fs::read_dir(path)
        .unwrap_or_else(|error| panic!("read carrier fixture dir {}: {error}", path.display()));
    entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                path.file_stem()
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
            } else {
                None
            }
        })
        .collect()
}

fn refusal_for_failure_kind(failure_kind: &str, failure_detail: &str) -> CompositionRefusalMemento {
    let atoms_cids = vec![address(&format!("atom:{failure_kind}")).to_string()];
    let effect_set_cids = Vec::new();
    let compose_input_cid =
        composition_refusal_compose_input_cid(&atoms_cids, &effect_set_cids, "1.0.0");
    let mut header = CompositionRefusalHeader {
        atoms_cids,
        blocking_effects: None,
        ccp_version: "1.0.0".to_string(),
        cid: String::new(),
        compose_input_cid,
        effect_occurrences: Some(vec![]),
        effect_set_cids,
        failure_detail: failure_detail.to_string(),
        failure_kind: failure_kind.to_string(),
        incompatible_pair: None,
        kind: "composition-refusal".to_string(),
        missing_memento_requirements: None,
        schema_version: "1".to_string(),
    };
    header.cid = composition_refusal_header_cid(&header);
    let metadata = CompositionRefusalMetadata::default();
    let signature = composition_refusal_signature(&header, &metadata);
    CompositionRefusalMemento {
        envelope: CompositionRefusalEnvelope {
            declared_at: "1970-01-01T00:00:00Z".to_string(),
            signature,
            signer: "substrate:test".to_string(),
        },
        header,
        metadata,
    }
}

#[test]
fn target_failure_kinds_round_trip_through_jcs() {
    let cases = [
        (
            "target-compile-failure",
            "compiler stderr: missing semicolon",
        ),
        (
            "target-behavior-divergence",
            "expected stdout 1; observed stdout 2",
        ),
    ];

    for (failure_kind, failure_detail) in cases {
        let refusal = refusal_for_failure_kind(failure_kind, failure_detail);
        let value = serde_json::to_value(&refusal).expect("refusal serializes");
        let jcs = libprovekit::canonical::json_jcs(&value).expect("refusal JCS");
        let decoded: CompositionRefusalMemento =
            serde_json::from_str(&jcs).expect("JCS refusal decodes");

        assert_eq!(decoded, refusal);
        assert_eq!(decoded.header.failure_kind, failure_kind);
        assert_eq!(decoded.header.failure_detail, failure_detail);
        assert_eq!(
            composition_refusal_header_cid(&decoded.header),
            decoded.header.cid
        );
    }
}
