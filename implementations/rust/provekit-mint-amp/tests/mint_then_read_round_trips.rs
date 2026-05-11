mod common;

use provekit_mint_amp::{
    mint_algorithm, mint_binding, mint_effect_signature, mint_equation, mint_language_morphism,
    mint_language_signature, mint_sort, AlgorithmSpec, BindingSpec, EffectSignatureSpec,
    EquationSpec, LanguageMorphismSpec, LanguageSignatureSpec, SortSpec,
};
use serde_json::json;

#[test]
fn mint_each_kind_then_read_payload_by_cid() {
    let (_dir, catalog) = common::test_catalog("roundtrip").expect("catalog");
    let signer = common::signer();

    let minted = [
        mint_sort(
            SortSpec::from_path(&common::fixture("sort_c_int.spec.json")).expect("sort spec"),
            &signer,
            &catalog,
        )
        .expect("mint sort"),
        mint_equation(
            EquationSpec::from_path(&common::fixture("equation_c_branch_identity.spec.json"))
                .expect("equation spec"),
            &signer,
            &catalog,
        )
        .expect("mint equation"),
        mint_algorithm(
            AlgorithmSpec::from_path(&common::fixture("algorithm_branch_on_nonzero.spec.json"))
                .expect("algorithm spec"),
            &signer,
            &catalog,
        )
        .expect("mint algorithm"),
        mint_binding(
            BindingSpec::from_path(&common::fixture("binding_c_branch_on_nonzero.spec.json"))
                .expect("binding spec"),
            &signer,
            &catalog,
        )
        .expect("mint binding"),
        mint_effect_signature(
            EffectSignatureSpec::from_path(&common::fixture(
                "effect_signature_c_control.spec.json",
            ))
            .expect("effect signature spec"),
            &signer,
            &catalog,
        )
        .expect("mint effect signature"),
        mint_language_signature(
            LanguageSignatureSpec::from_path(&common::fixture(
                "language_signature_c_c11.spec.json",
            ))
            .expect("language signature spec"),
            &signer,
            &catalog,
        )
        .expect("mint language signature"),
        mint_language_morphism(
            LanguageMorphismSpec::from_path(&common::fixture("morphism_c_to_llvm_ir.spec.json"))
                .expect("morphism spec"),
            &signer,
            &catalog,
        )
        .expect("mint morphism"),
    ];

    for memento in minted {
        let stored = catalog.read_by_cid(&memento.cid).expect("read by cid");
        assert_eq!(stored, memento.payload);
        assert!(memento.path.exists(), "memento path should exist");
    }
}

#[test]
fn language_signature_mint_preserves_declared_arity_shapes() {
    let (_dir, catalog) = common::test_catalog("arity-shape").expect("catalog");
    let signer = common::signer();
    let spec = LanguageSignatureSpec::from_value(json!({
        "kind": "language_signature",
        "fn_name": "c:c11",
        "sorts": [],
        "operations": [],
        "equations": [],
        "effect_signatures": [],
        "arity_shapes": {
            "c11:bop_add": {"kind": "set"},
            "c11:bop_logand": {
                "kind": "named",
                "slots": [
                    {"name": "left"},
                    {"name": "right"}
                ]
            },
            "c11:sizeof_expr": {
                "kind": "named",
                "slots": [{"name": "operand", "evaluation": "unevaluated"}]
            },
            "c11:sizeof_type": {
                "kind": "named",
                "slots": [{"name": "operand", "evaluation": "unevaluated", "slot_sort": "type"}]
            },
            "c11:generic-selection": {
                "kind": "named",
                "slots": [
                    {"name": "controlling", "evaluation": "unevaluated"},
                    {"name": "branches", "shape": {"kind": "set"}}
                ]
            },
            "c11:offsetof": {
                "kind": "named",
                "slots": [
                    {"name": "type", "evaluation": "unevaluated", "slot_sort": "type"},
                    {"name": "designator", "evaluation": "unevaluated", "slot_sort": "identifier"}
                ]
            },
            "c11:seq": {"kind": "positional", "arity": 2}
        }
    }));

    let minted = mint_language_signature(spec, &signer, &catalog).expect("mint signature");
    assert_eq!(
        minted.payload["post"]["arity_shapes"],
        json!({
            "c11:bop_add": {"kind": "set"},
            "c11:bop_logand": {
                "kind": "named",
                "slots": [
                    {"name": "left"},
                    {"name": "right"}
                ]
            },
            "c11:sizeof_expr": {
                "kind": "named",
                "slots": [{"name": "operand", "evaluation": "unevaluated"}]
            },
            "c11:sizeof_type": {
                "kind": "named",
                "slots": [{"name": "operand", "evaluation": "unevaluated", "slot_sort": "type"}]
            },
            "c11:generic-selection": {
                "kind": "named",
                "slots": [
                    {"name": "controlling", "evaluation": "unevaluated"},
                    {"name": "branches", "shape": {"kind": "set"}}
                ]
            },
            "c11:offsetof": {
                "kind": "named",
                "slots": [
                    {"name": "type", "evaluation": "unevaluated", "slot_sort": "type"},
                    {"name": "designator", "evaluation": "unevaluated", "slot_sort": "identifier"}
                ]
            },
            "c11:seq": {"kind": "positional", "arity": 2}
        })
    );
}
