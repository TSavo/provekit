// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use libprovekit::canonical::{json_cid, json_jcs};
use libprovekit::proofir_bridge::CatalogIndex;
use libprovekit::{proofir_resolve, proofir_unresolve};
use provekit_ir_types::Term;
use serde_json::{json, Value as JsonValue};

const EXPECTED_FIXTURE_CID: &str = "blake3-512:bcb10be48ad632abc71c406355b6d11b0191a959b523aa755ee00ad7496afa2270ce28821af4abcd5949427026fb16d8d8b38af702b1810dec3bdff810ec8f32";

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("proofir")
        .join("d7_v0_value_null.json")
}

fn resolved_sort(name: &str) -> JsonValue {
    json!({
        "args": [],
        "kind": "ctor",
        "name": name,
    })
}

fn fixture_op_cid(fixture: &JsonValue, name: &str) -> String {
    fixture["proofir_catalog_ops"]
        .as_array()
        .expect("proofir_catalog_ops array")
        .iter()
        .find(|op| op["name"] == name)
        .and_then(|op| op["op_cid"].as_str())
        .unwrap_or_else(|| panic!("fixture has op cid for {name}"))
        .to_string()
}

fn value_null_catalog(fixture: &JsonValue) -> CatalogIndex {
    let mut catalog = CatalogIndex::new();
    catalog.insert_op(
        "return",
        fixture_op_cid(fixture, "return"),
        Some(vec![resolved_sort("Expr")]),
        Some(resolved_sort("Stmt")),
    );
    catalog.insert_op(
        "call:new",
        fixture_op_cid(fixture, "call:new"),
        Some(vec![
            resolved_sort("FnContract"),
            resolved_sort("ListOfExpr"),
        ]),
        Some(resolved_sort("Expr")),
    );
    catalog
}

#[test]
fn value_null_lift_round_trips_byte_identical() {
    let text = std::fs::read_to_string(fixture_path()).expect("read D7-v0 value null fixture");
    let original_json: JsonValue = serde_json::from_str(&text).expect("parse D7-v0 fixture");
    let original_loss_record = original_json["loss_record"].clone();
    let original_term: Term =
        serde_json::from_value(original_json["proofir_term"].clone()).expect("decode ProofIR term");

    let catalog = value_null_catalog(&original_json);
    let resolved = proofir_resolve(&original_term, &catalog).expect("resolve real-lift ProofIR");
    let unresolved = proofir_unresolve(&resolved, &catalog).expect("unresolve real-lift ProofIR");

    let mut round_trip_json = original_json.clone();
    round_trip_json["proofir_term"] =
        serde_json::to_value(&unresolved).expect("encode unresolved ProofIR term");

    let original_jcs = json_jcs(&original_json).expect("JCS original fixture");
    let round_trip_jcs = json_jcs(&round_trip_json).expect("JCS round-trip fixture");

    assert_eq!(
        original_jcs.as_bytes(),
        round_trip_jcs.as_bytes(),
        "resolve then unresolve must preserve fixture JCS bytes"
    );
    assert_eq!(
        round_trip_json["loss_record"], original_loss_record,
        "loss record must be preserved verbatim"
    );

    let fixture_cid = json_cid(&original_json).expect("fixture CID");
    let round_trip_cid = json_cid(&round_trip_json).expect("round-trip CID");
    assert_eq!(fixture_cid, EXPECTED_FIXTURE_CID);
    assert_eq!(fixture_cid, round_trip_cid);
    println!("fixture_cid={fixture_cid}");
    println!("round_trip_cid={round_trip_cid}");
}
