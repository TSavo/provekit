// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::sync::{Arc, Mutex};

use libprovekit::core::{Input, Kit, LowerKit, RealizeRequest, RealizeTransport, RealizedSource};
use serde_json::{json, Value};

fn valid_cid(fill: char) -> String {
    format!("blake3-512:{}", fill.to_string().repeat(128))
}

#[derive(Clone, Default)]
struct RecursiveTransport {
    requests: Arc<Mutex<Vec<RealizeRequest>>>,
    fail_concept: Option<String>,
}

impl RecursiveTransport {
    fn refusing(concept: &str) -> Self {
        Self {
            requests: Arc::new(Mutex::new(Vec::new())),
            fail_concept: Some(concept.to_string()),
        }
    }

    fn requests(&self) -> Vec<RealizeRequest> {
        self.requests.lock().expect("requests lock").clone()
    }
}

impl RealizeTransport for RecursiveTransport {
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
        if self.fail_concept.as_deref() == Some(request.concept_name.as_str()) {
            return Err(format!(
                "missing child realization for {}",
                request.concept_name
            ));
        }
        match request.concept_name.as_str() {
            "concept:bitcoin-send" => Ok(RealizedSource {
                extension: "txt".to_string(),
                source: "return bitcoin_send(tx, amount);".to_string(),
                is_stub: false,
                emitted_artifact_cid: Some(valid_cid('b')),
                observed_loss_record: json!({
                    "child-loss": {
                        "args": [],
                        "head": "atomic",
                        "name": "child-loss"
                    }
                }),
                used_sugars: vec![],
                observation_wrapper_emission_record: None,
                ..Default::default()
            }),
            "concept:log-emit" => {
                let binding = request
                    .operand_bindings
                    .iter()
                    .find(|binding| {
                        binding.get("kind").and_then(Value::as_str)
                            == Some("recursive-child-realization")
                    })
                    .ok_or_else(|| "parent missing recursive child binding".to_string())?;
                let composition_point = binding
                    .get("composition_point")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "child binding missing composition_point".to_string())?;
                let child_source = binding
                    .get("source")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "child binding missing source".to_string())?;
                let source = if composition_point == "after-return" {
                    format!(
                        "let __provekit_result = bitcoin_send(tx, amount);\nlog_emit(__provekit_result);\nreturn __provekit_result;\n// child: {child_source}"
                    )
                } else {
                    format!("{composition_point}: {child_source}")
                };
                Ok(RealizedSource {
                    extension: "txt".to_string(),
                    source,
                    is_stub: false,
                    emitted_artifact_cid: Some(valid_cid('c')),
                    observed_loss_record: json!({
                        "parent-loss": {
                            "args": [],
                            "head": "atomic",
                            "name": "parent-loss"
                        }
                    }),
                    used_sugars: vec![],
                    observation_wrapper_emission_record: None,
                    ..Default::default()
                })
            }
            other => Err(format!("unexpected concept {other}")),
        }
    }
}

fn tree_with_point(composition_point: &str) -> Value {
    json!({
        "conceptName": "concept:log-emit",
        "operationKind": "log-emit",
        "shapeCid": valid_cid('1'),
        "compositionPoint": composition_point,
        "args": [{
            "conceptName": "concept:bitcoin-send",
            "operationKind": "bitcoin-send",
            "shapeCid": valid_cid('2'),
            "args": []
        }]
    })
}

fn recursive_spec(composition_point: &str) -> Value {
    json!({
        "kind": "RealizeRequest",
        "function": "send_with_log",
        "params": ["tx", "amount"],
        "paramTypes": ["Tx", "i64"],
        "returnType": "Receipt",
        "conceptName": "concept:log-emit",
        "namedTermTree": tree_with_point(composition_point),
        "termShapeCid": valid_cid('a')
    })
}

fn lower_with(
    spec: Value,
    transport: RecursiveTransport,
) -> (
    Result<libprovekit::core::DomainClaim, libprovekit::core::KitError>,
    RecursiveTransport,
) {
    let lower = LowerKit::new(
        "/tmp/provekit-recursive-composition-test",
        "generic-test",
        None,
        transport.clone(),
    );
    let result = lower.transform(&Input::Spec(spec));
    (result, transport)
}

#[test]
fn recursive_after_return_wraps_child_and_preserves_child_result() {
    let (result, transport) = lower_with(
        recursive_spec("after-return"),
        RecursiveTransport::default(),
    );

    let claim = result.expect("recursive lower succeeds");
    let realized = LowerKit::<RecursiveTransport>::realized_source_from_claim(&claim)
        .expect("payload decodes");
    let requests = transport.requests();
    assert_eq!(
        requests
            .iter()
            .map(|request| request.concept_name.as_str())
            .collect::<Vec<_>>(),
        vec!["concept:bitcoin-send", "concept:log-emit"]
    );
    let parent_request = requests
        .iter()
        .find(|request| request.concept_name == "concept:log-emit")
        .expect("parent request captured");
    let child_claim_cid = parent_request.operand_bindings[0]["child_claim_cid"]
        .as_str()
        .expect("child claim CID");
    assert!(realized
        .source
        .contains("let __provekit_result = bitcoin_send(tx, amount);"));
    assert!(realized.source.contains("log_emit(__provekit_result);"));
    assert!(realized.source.contains("return __provekit_result;"));
    assert!(
        claim
            .premises
            .iter()
            .any(|premise| premise.as_str() == child_claim_cid),
        "parent claim must cite child claim CID"
    );
}

#[test]
fn recursive_child_loss_records_are_aggregated_into_parent_output() {
    let (result, _) = lower_with(
        recursive_spec("after-return"),
        RecursiveTransport::default(),
    );

    let claim = result.expect("recursive lower succeeds");
    let realized = LowerKit::<RecursiveTransport>::realized_source_from_claim(&claim)
        .expect("payload decodes");

    assert!(realized.observed_loss_record.get("child-loss").is_some());
    assert!(realized.observed_loss_record.get("parent-loss").is_some());
}

#[test]
fn missing_child_realization_refuses_without_parent_dispatch() {
    let transport = RecursiveTransport::refusing("concept:bitcoin-send");

    let (result, transport) = lower_with(recursive_spec("after-return"), transport);

    let error = result.expect_err("missing child realization refuses");
    assert!(
        error
            .to_string()
            .contains("missing child realization for concept:bitcoin-send"),
        "unexpected error: {error}"
    );
    assert_eq!(
        transport
            .requests()
            .iter()
            .map(|request| request.concept_name.as_str())
            .collect::<Vec<_>>(),
        vec!["concept:bitcoin-send"],
        "parent must not be dispatched after a child refusal"
    );
}

#[test]
fn recursive_composition_accepts_declared_composition_points() {
    for point in ["before", "after-return", "after-throw", "around"] {
        let (result, transport) = lower_with(recursive_spec(point), RecursiveTransport::default());

        result.unwrap_or_else(|error| panic!("{point} must be accepted, got {error}"));
        let requests = transport.requests();
        let parent = requests
            .iter()
            .find(|request| request.concept_name == "concept:log-emit")
            .expect("parent request captured");
        let binding = parent
            .operand_bindings
            .iter()
            .find(|binding| {
                binding.get("kind").and_then(Value::as_str) == Some("recursive-child-realization")
            })
            .expect("recursive binding present");
        assert_eq!(binding["composition_point"], point);
    }
}

#[test]
fn recursive_composition_refuses_unknown_composition_point_before_transport() {
    let (result, transport) = lower_with(recursive_spec("during"), RecursiveTransport::default());

    let error = result.expect_err("unknown composition point refuses");
    assert!(
        error
            .to_string()
            .contains("unknown recursive composition point `during`"),
        "unexpected error: {error}"
    );
    assert!(
        transport.requests().is_empty(),
        "structural refusal must happen before any transport dispatch"
    );
}

#[test]
fn recursive_composition_refuses_malformed_child_tree_before_transport() {
    let mut spec = recursive_spec("after-return");
    spec["namedTermTree"]["args"] = json!([42]);

    let (result, transport) = lower_with(spec, RecursiveTransport::default());

    let error = result.expect_err("malformed child tree refuses");
    assert!(
        error
            .to_string()
            .contains("recursive child at position 0 must be an object"),
        "unexpected error: {error}"
    );
    assert!(
        transport.requests().is_empty(),
        "structural refusal must happen before any transport dispatch"
    );
}
