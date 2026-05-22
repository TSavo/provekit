// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use libprovekit::compose::{
    build_value, cid_of_value, jcs_bytes_of_value, EffectSet, FunctionContractMemento, Locus,
};
use libprovekit::core::lower_plugin::realize_spec_from_named_term;
use libprovekit::core::{
    address, concept_bind_result_cid, named_term_document_from_bind_payload, BindKit, BindOptions,
    Cid, DomainClaim, DomainKind, Input, Kit, LowerKit, RealizeRequest, RealizeTransport,
    RealizedSource, Term, Verdict,
};
use provekit_ir_types::{IrFormula, Sort};
use serde_json::{json, Value};

fn primitive_sort(name: &str) -> Sort {
    Sort::Primitive {
        name: name.to_string(),
    }
}

fn valid_cid(fill: char) -> String {
    format!("blake3-512:{}", fill.to_string().repeat(128))
}

fn parse_cid(fill: char) -> Cid {
    Cid::parse(valid_cid(fill)).expect("valid CID")
}

fn minimal_contract(fn_name: &str) -> FunctionContractMemento {
    let formals = vec!["x".to_string()];
    let formal_sorts = vec![primitive_sort("int")];
    let return_sort = primitive_sort("int");
    let pre = IrFormula::Atomic {
        name: "true".to_string(),
        args: vec![],
    };
    let post = IrFormula::Atomic {
        name: "true".to_string(),
        args: vec![],
    };
    let effects = EffectSet::empty();
    let locus = Locus::unknown();
    let value = build_value(
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
        canonical_bytes: jcs_bytes_of_value(&value),
        cid: cid_of_value(&value),
        auto_minted_mementos: vec![],
        concept_hint: None,
    }
}

fn bind_input_value() -> Value {
    json!({
        "kind": "ir-document",
        "sourceLanguage": "rust",
        "workspaceRoot": "/tmp/provekit-lower-claim-bind-result-test",
        "ir": [{
            "kind": "bind-lift-entry",
            "file": "src/lib.rs",
            "fn_name": "deposit",
            "source_function_name": "deposit",
            "fn_line": 14,
            "concept_annotation": "deposit-then-balance",
            "param_names": ["balance", "amount"],
            "param_types": ["i64", "i64"],
            "return_type": "i64",
            "operand_bindings": [
                {"position": [0, 0], "symbol": "balance"},
                {"position": [0, 1], "symbol": "amount"}
            ],
            "term_shape": {
                "kind": "body",
                "stmts": [
                    {"kind": "let"},
                    {"kind": "bin", "op": "+"}
                ]
            },
            "term_shape_cid": valid_cid('a'),
            "witnesses": [{
                "role": "post",
                "predicate_text": "out == balance + amount",
                "source_kind": "annotation"
            }]
        }]
    })
}

fn erased_bind_input_value() -> Value {
    json!({
        "kind": "ir-document",
        "workspaceRoot": "/tmp/provekit-lower-claim-bind-result-test",
        "ir": [{
            "kind": "bind-lift-entry",
            "file": "src/lib.rs",
            "fn_name": "wrap_identity",
            "fn_line": 7,
            "concept_annotation": "identity",
            "param_names": ["x"],
            "term_shape": {
                "kind": "body",
                "stmts": [
                    {"kind": "exit"}
                ]
            },
            "term_shape_cid": valid_cid('d'),
            "witnesses": []
        }]
    })
}

#[derive(Clone, Default)]
struct CapturingTransport {
    requests: Arc<Mutex<Vec<RealizeRequest>>>,
}

impl CapturingTransport {
    fn requests(&self) -> Vec<RealizeRequest> {
        self.requests.lock().expect("requests lock").clone()
    }
}

impl RealizeTransport for CapturingTransport {
    fn dispatch_realize(
        &self,
        _workspace_root: &Path,
        _target_lang: &str,
        _library_tag: Option<&str>,
        request: &RealizeRequest,
    ) -> Result<RealizedSource, String> {
        self.requests
            .lock()
            .expect("requests lock")
            .push(request.clone());
        Ok(RealizedSource {
            extension: "txt".to_string(),
            source: format!("realized {}", request.function),
            is_stub: false,
            emitted_artifact_cid: Some(valid_cid('b')),
            observed_loss_record: json!({}),
            used_sugars: vec![],
            observation_wrapper_emission_record: None,
            ..Default::default()
        })
    }
}

fn lower_with_capture(
    claim: DomainClaim,
    target: &str,
) -> (
    Result<DomainClaim, libprovekit::core::KitError>,
    CapturingTransport,
) {
    let transport = CapturingTransport::default();
    let lower = LowerKit::new(
        PathBuf::from("/tmp/provekit-lower-claim-bind-result-test"),
        target.to_string(),
        None,
        transport.clone(),
    );
    let result = lower.transform(&Input::Claim(claim));
    (result, transport)
}

fn bind_claim() -> DomainClaim {
    let term_value = bind_input_value();
    let input_term = Term::Const {
        value: term_value,
        sort: primitive_sort("LiftPluginResponse"),
    };
    BindKit::new(BindOptions {
        lang: "rust".to_string(),
        exam_manifest: None,
    })
    .transform(&Input::Term(input_term))
    .expect("bind kit transforms term input")
}

fn erased_bind_claim() -> DomainClaim {
    let term_value = erased_bind_input_value();
    let input_term = Term::Const {
        value: term_value,
        sort: primitive_sort("LiftPluginResponse"),
    };
    BindKit::default()
        .transform(&Input::Term(input_term))
        .expect("bind kit transforms erased term input")
}

#[test]
fn bind_result_claim_lower_uses_named_term_realize_request() {
    let claim = bind_claim();
    let payload = claim.payload.as_ref().expect("bind claim payload");
    let named =
        named_term_document_from_bind_payload(payload).expect("bind payload recovers named terms");
    let expected_spec =
        realize_spec_from_named_term(&named.terms[0]).expect("named term spec builds");

    let (result, transport) = lower_with_capture(claim, "python");

    result.expect("lower claim succeeds");
    let requests = transport.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.function, "deposit-then-balance");
    assert_eq!(request.params, vec!["balance", "amount"]);
    assert_eq!(request.param_types, vec!["i64", "i64"]);
    assert_eq!(request.return_type, "i64");
    assert_eq!(request.concept_name, "concept:deposit-then-balance");
    assert_eq!(
        request.named_term_tree,
        expected_spec.get("namedTermTree").cloned()
    );
    assert_eq!(request.term_shape, expected_spec.get("termShape").cloned());
    let request_json = serde_json::to_value(request).expect("request serializes");
    assert_eq!(
        request_json["operand_bindings"],
        json!([
            {"position": [0, 0], "symbol": "balance"},
            {"position": [0, 1], "symbol": "amount"},
        ])
    );
    assert_eq!(request_json["source_function_name"], json!("deposit"));
}

#[test]
fn bind_result_claim_lower_reconstructs_erased_signature_defaults() {
    let claim = erased_bind_claim();
    let payload = claim.payload.as_ref().expect("bind claim payload");
    let named =
        named_term_document_from_bind_payload(payload).expect("bind payload recovers named terms");
    assert_eq!(named.terms[0].param_types, Vec::<String>::new());
    assert_eq!(named.terms[0].return_type, "()");

    let (result, transport) = lower_with_capture(claim, "rust");

    result.expect("lower claim succeeds");
    let requests = transport.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.function, "identity");
    assert_eq!(request.params, vec!["x"]);
    assert_eq!(request.param_types, vec!["int"]);
    assert_eq!(request.return_type, "int");
    assert_eq!(request.concept_name, "concept:identity");
}

#[test]
fn term_const_claim_fast_path_is_preserved() {
    let spec = json!({
        "kind": "RealizeRequest",
        "function": "const_path",
        "params": ["x"],
        "paramTypes": ["int"],
        "returnType": "int",
        "conceptName": "concept:const-path",
        "termShapeCid": parse_cid('c')
    });
    let claim = DomainClaim {
        domain: DomainKind::Other("prior".to_string()),
        contract: minimal_contract("ignored_contract"),
        artifacts: vec![],
        from: vec![parse_cid('c')],
        premises: vec![],
        to: parse_cid('c'),
        witness: None,
        payload: Some(Term::Const {
            value: spec,
            sort: primitive_sort("LowerSpec"),
        }),
        verdict: Verdict::Unresolved,
        attestation: None,
    };

    let (result, transport) = lower_with_capture(claim, "python");

    result.expect("lower claim succeeds");
    let requests = transport.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].function, "const_path");
    assert_eq!(requests[0].concept_name, "concept:const-path");
}

#[test]
fn non_bind_result_op_claim_uses_existing_fallback() {
    let payload = Term::Op {
        op_cid: parse_cid('d'),
        name: "concept:not-bind-result".to_string(),
        args: vec![],
    };
    let claim = DomainClaim {
        domain: DomainKind::Other("other-op".to_string()),
        contract: minimal_contract("fallback_op"),
        artifacts: vec![],
        from: vec![parse_cid('d')],
        premises: vec![],
        to: address(&payload),
        witness: None,
        payload: Some(payload),
        verdict: Verdict::Unresolved,
        attestation: None,
    };

    let (result, transport) = lower_with_capture(claim, "python");

    result.expect("lower claim succeeds");
    let requests = transport.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].function, "fallback_op");
    assert_eq!(requests[0].params, vec!["x"]);
    assert_eq!(requests[0].param_types, vec!["int"]);
    assert_eq!(requests[0].return_type, "int");
    assert_eq!(requests[0].named_term_tree, None);
    assert_eq!(requests[0].term_shape, None);
}

#[test]
fn malformed_bind_result_wrapper_refuses_before_transport() {
    let payload = Term::Op {
        op_cid: concept_bind_result_cid(),
        name: "concept:bind-result".to_string(),
        args: vec![Term::Const {
            value: bind_input_value(),
            sort: primitive_sort("LiftPluginResponse"),
        }],
    };
    let claim = DomainClaim {
        domain: DomainKind::Other("bind".to_string()),
        contract: minimal_contract("bind::default::bind-result-op-tree"),
        artifacts: vec![],
        from: vec![parse_cid('e')],
        premises: vec![],
        to: address(&payload),
        witness: None,
        payload: Some(payload),
        verdict: Verdict::Unresolved,
        attestation: None,
    };

    let (result, transport) = lower_with_capture(claim, "python");

    let error = result.expect_err("malformed bind-result wrapper refuses");
    assert!(
        error
            .to_string()
            .contains("bind-result wrapper expected 2 args, got 1"),
        "unexpected error: {error}"
    );
    assert!(transport.requests().is_empty());
}

#[test]
fn bind_result_request_shape_is_target_neutral_for_python_java_and_c() {
    let mut requests = Vec::new();
    for target in ["python", "java", "c"] {
        let (result, transport) = lower_with_capture(bind_claim(), target);
        result.expect("lower claim succeeds");
        let captured = transport.requests();
        assert_eq!(captured.len(), 1);
        requests.push(serde_json::to_value(&captured[0]).expect("request serializes"));
    }

    assert_eq!(requests[0], requests[1]);
    assert_eq!(requests[0], requests[2]);
}
