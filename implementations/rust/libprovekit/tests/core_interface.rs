// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use libprovekit::compose::{
    build_value, cid_of_value, compose_function_contracts, jcs_bytes_of_value, EffectSet,
    FunctionContractMemento, Locus,
};
use libprovekit::core::{
    address, compose, prove, transform, verify, Boundary, CKit, Cid, DomainClaim, DomainKind,
    FunctionContractDomain, HashMapCatalog, Input, NoopPortfolio, Refutation, Term, Truth, Verdict,
    Witness,
};
use provekit_canonicalizer::Value;
use provekit_ir_types::{IrFormula, IrTerm, Sort};

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
    DomainClaim {
        domain: DomainKind::FunctionContract,
        term: Some(Term::Var {
            name: contract.fn_name.clone(),
        }),
        contract,
        from: vec![],
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
fn transform_and_prove_build_a_contract_claim_with_stub_kits() {
    let kit = CKit::default();
    let domain = FunctionContractDomain;
    let boundary = Boundary::default();
    let input = Input::Term(Term::Var {
        name: "x".to_string(),
    });

    let claim = transform(&kit, &domain, &input, &boundary).expect("transform succeeds");
    assert_eq!(claim.domain, DomainKind::FunctionContract);
    assert!(claim.term.is_some());
    assert_eq!(claim.verdict, Verdict::Unresolved);

    let proved =
        prove(&kit, &domain, &input, &boundary, &NoopPortfolio).expect("prove skeleton succeeds");
    assert_eq!(proved.verdict, Verdict::Unknown);
    assert!(matches!(proved.witness, Some(Witness::Unknown { .. })));
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
    assert_eq!(round_tripped.0.verdict, Verdict::Proved);

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
    assert_eq!(round_tripped.0.verdict, Verdict::Refuted);
}

#[test]
fn resolve_reads_canonical_bytes_from_hash_map_catalog() {
    let claim = claim_for_contract(pure_identity_contract("cataloged", "x"));
    let mut catalog = HashMapCatalog::default();
    let cid = catalog.insert(&claim);

    let bytes = libprovekit::core::resolve(&cid, &catalog).expect("claim bytes resolve");
    assert_eq!(bytes, claim.canonical_bytes());
}
