// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use libsugar::canonical::{json_cid, json_jcs};
use serde_json::Value as JsonValue;

const EXPECTED_FIXTURE_CID: &str = "blake3-512:0859471677c6f49a8f1add647671930db39754ad7d758a8d87202b29be2da8b86a839cfb5d27dd2d937b2b8beaec6dcf81f5aa32554e79f436afa2de693bbf39";
const EXPECTED_RETURN_CID: &str = "blake3-512:776d417c66325df1d40e3e0fd7331195e2b1d14f9c30b5984030f21aa8b6b38b3eb81ee3dddd46716003275c9960022e2273dd8efb0110bacc5719811ee18dc6";
const EXPECTED_CALL_NEW_CID: &str = "blake3-512:e6576534d74eee6b309fa55457620d4903472dcd331f0cb9c2be2a95994655ad64ef1fa56f778534f6ba5c04c055069bb109de3dae4dc45bde7dd689671b24b8";

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("proofir")
        .join("d7_v0_value_null.json")
}

fn fixture_op_cid<'a>(fixture: &'a JsonValue, name: &str) -> &'a str {
    fixture["proofir_catalog_ops"]
        .as_array()
        .expect("proofir_catalog_ops array")
        .iter()
        .find(|op| op["name"] == name)
        .and_then(|op| op["op_cid"].as_str())
        .unwrap_or_else(|| panic!("fixture has op cid for {name}"))
}

#[test]
fn value_null_lift_fixture_pins_local_op_cids_and_content_cid() {
    let text = std::fs::read_to_string(fixture_path()).expect("read D7-v0 value null fixture");
    let fixture: JsonValue = serde_json::from_str(&text).expect("parse D7-v0 fixture");

    assert_eq!(fixture_op_cid(&fixture, "return"), EXPECTED_RETURN_CID);
    assert_eq!(fixture_op_cid(&fixture, "call:new"), EXPECTED_CALL_NEW_CID);
    assert_eq!(fixture["proofir_term"]["name"], "return");
    assert_eq!(fixture["proofir_term"]["args"][0]["name"], "call:new");
    assert_eq!(
        fixture["loss_record"]
            .as_array()
            .expect("loss_record array")
            .len(),
        4,
        "loss record must stay present as the real-lift partiality witness"
    );

    let fixture_jcs = json_jcs(&fixture).expect("JCS fixture");
    let reparsed: JsonValue = serde_json::from_str(&fixture_jcs).expect("reparse JCS fixture");
    let reparsed_jcs = json_jcs(&reparsed).expect("JCS reparsed fixture");
    assert_eq!(
        fixture_jcs, reparsed_jcs,
        "fixture JCS must be byte-stable without a name catalog round trip"
    );

    let fixture_cid = json_cid(&fixture).expect("fixture CID");
    assert_eq!(fixture_cid, EXPECTED_FIXTURE_CID);
    println!("fixture_cid={fixture_cid}");
}
