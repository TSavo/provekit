// SPDX-License-Identifier: Apache-2.0

use libsugar::core::RealizeRequest;
use serde_json::{json, Value};

#[test]
fn realize_request_accepts_op_cid_alongside_concept_name() {
    let request: RealizeRequest = serde_json::from_value(json!({
        "function": "materialize_add",
        "params": ["x"],
        "param_types": ["int"],
        "return_type": "int",
        "concept_name": "concept:add",
        "op_cid": "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "mode": null,
        "modes": [],
        "contract": null,
        "sugar_cids": [],
        "sugar_plugins": [],
        "family": null,
        "library_version": null
    }))
    .expect("RealizeRequest with op_cid decodes");

    assert_eq!(request.concept_name, "concept:add");
    assert_eq!(
        request.op_cid.as_deref(),
        Some("blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );

    let serialized: Value = serde_json::to_value(&request).expect("serialize request");
    assert_eq!(serialized["concept_name"], "concept:add");
    assert_eq!(
        serialized["opCid"],
        "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
}
