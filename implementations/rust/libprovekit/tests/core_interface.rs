// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use libprovekit::compose::{
    build_value, cid_of_value, compose_function_contracts, jcs_bytes_of_value, EffectSet,
    FunctionContractMemento, Locus,
};
use libprovekit::core::{
    address, compose, link, prove, transform, verify, ArityShape, AritySlot, CKit, Cid,
    DomainClaim, DomainKind, FunctionContractDomain, HashMapCatalog, HashMapInputCatalog, Input,
    InputCatalog, Kit, LanguageSignature, LiftPluginKit, Path, PathAlgebra, PathDocument,
    PathError, Refutation, SlotSort, Term, Truth, Verdict, Witness,
};
use provekit_canonicalizer::Value;
use provekit_ir_types::{IrFormula, IrTerm, Sort};
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
        verdict: Verdict::Unresolved,
        attestation: None,
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
    };
    let mint_step = PathAlgebra {
        name: "mint".to_string(),
        kit: "provekit-mint".to_string(),
        inputs: vec![address(&Input::Spec(serde_json::json!({
            "outDir": "/repo/out"
        })))],
        depends_on: vec!["lift".to_string()],
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
            },
            PathAlgebra {
                name: "mint".to_string(),
                kit: "provekit-mint".to_string(),
                inputs: vec![mint_input_cid.clone()],
                depends_on: vec!["lift".to_string()],
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
    assert!(
        !encoded.contains("\"term\""),
        "path documents should not embed language terms: {encoded}"
    );
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
fn path_derives_order_from_dependencies() {
    let lift_step = PathAlgebra {
        name: "lift".to_string(),
        kit: "lift-plugin:rust".to_string(),
        inputs: vec![address(&Input::Spec(
            serde_json::json!({"surface": "rust"}),
        ))],
        depends_on: vec![],
    };
    let mint_step = PathAlgebra {
        name: "mint".to_string(),
        kit: "provekit-mint".to_string(),
        inputs: vec![address(&Input::Spec(serde_json::json!({"outDir": "out"})))],
        depends_on: vec!["lift".to_string()],
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
    };
    let c_proof = PathAlgebra {
        name: "c-proof".to_string(),
        kit: "lift-plugin:c".to_string(),
        inputs: vec![address(&Input::Spec(serde_json::json!({
            "proofFile": "c.proof",
            "contractCid": shared_contract.as_str()
        })))],
        depends_on: vec![],
    };
    let link = PathAlgebra {
        name: "link".to_string(),
        kit: "provekit-link".to_string(),
        inputs: vec![address(&Input::Spec(serde_json::json!({
            "sharedContractCid": shared_contract.as_str()
        })))],
        depends_on: vec!["ts-proof".to_string(), "c-proof".to_string()],
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
