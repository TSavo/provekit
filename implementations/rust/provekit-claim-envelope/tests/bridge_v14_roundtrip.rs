// SPDX-License-Identifier: Apache-2.0
//
// v1.4 BridgeDeclaration byte-equality round-trip parity tests.
//
// Source of truth:
//   protocol/specs/2026-05-03-bridge-target-dimensionality.md §1, §3
//   protocol/specs/2026-05-03-substrate-layers-envelope-header-body.md
//   protocol/provekit-ir.cddl  BridgeDeclarationV14
//
// What this file pins:
//
//   1. Round-trip parity (acceptance #5):
//        emit v1.4 bridge -> re-parse -> emit again -> byte-identical
//
//   2. Canonical fixture bytes for `conformance/fixtures.toml`:
//        the `bridge_decl_v1_4` entry MUST match the JCS bytes and
//        BLAKE3-512 hash this test prints / asserts.
//
//   3. Spec §1.R2 conformance: omitted metadata fields are ABSENT from
//      the JCS bytes, NOT serialized as `null`.
//
//   4. Spec §1.R1 conformance: tagged-union `target` round-trips
//      through both `Contract` and `ContractSet` variants.
//
// The fixture inputs (signer seed, CIDs, timestamps, names) are pinned
// constants so the resulting JCS bytes are reproducible across kits.

use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value};
use provekit_claim_envelope::{
    mint_bridge_v14, BridgeTargetV14, MintBridgeV14Args,
};
use provekit_proof_envelope::Ed25519Seed;
use serde_json::Value as Json;

// Inline copy of `tests/layered_shape.rs::json_to_value`. Both tests
// need to convert a `serde_json::Value` into the canonicalizer's
// `Value` to drive a JCS re-emit; the helper is private to its test
// file. We duplicate it here rather than expand the canonicalizer
// public API for a test-only need.
fn json_to_value(j: &Json) -> Arc<Value> {
    match j {
        Json::Null => Value::null(),
        Json::Bool(b) => Value::boolean(*b),
        Json::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::integer(i)
            } else if let Some(u) = n.as_u64() {
                Value::integer(u as i64)
            } else if let Some(f) = n.as_f64() {
                Value::integer(f as i64)
            } else {
                Value::integer(0)
            }
        }
        Json::String(s) => Value::string(s.clone()),
        Json::Array(items) => {
            let v: Vec<_> = items.iter().map(json_to_value).collect();
            Value::array(v)
        }
        Json::Object(map) => {
            let entries: Vec<(String, _)> = map
                .iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect();
            Arc::new(Value::Object(entries))
        }
    }
}

// All-0x42 seed produces a deterministic Ed25519 keypair across runs.
fn fixture_seed() -> Ed25519Seed {
    [0x42u8; 32]
}

const FIXTURE_NAME: &str = "rust-canonical-bridge-fixture";
const FIXTURE_SOURCE_SYMBOL: &str = "parseInt";
const FIXTURE_SOURCE_LAYER: &str = "rust-kit";
const FIXTURE_SOURCE_CONTRACT_CID: &str = "blake3-512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
const FIXTURE_TARGET_CONTRACT_CID: &str = "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111";
const FIXTURE_DECLARED_AT: &str = "2026-05-03T00:00:00.000Z";

fn canonical_fixture_args() -> MintBridgeV14Args {
    MintBridgeV14Args {
        name: FIXTURE_NAME.into(),
        source_symbol: FIXTURE_SOURCE_SYMBOL.into(),
        source_layer: FIXTURE_SOURCE_LAYER.into(),
        source_contract_cid: FIXTURE_SOURCE_CONTRACT_CID.into(),
        target: BridgeTargetV14::Contract {
            cid: FIXTURE_TARGET_CONTRACT_CID.into(),
        },
        // Spec §1.R2: witness/binary axes unknown -> OMIT.
        target_witness_cid: None,
        target_binary_cid: None,
        target_layer: Some("rust-kit".into()),
        target_contract_set_cid: None,
        produced_by: Some("provekit-canonical-reference@v1.4".into()),
        produced_at: Some(FIXTURE_DECLARED_AT.into()),
        declared_at: FIXTURE_DECLARED_AT.into(),
        signer_seed: fixture_seed(),
    }
}

#[test]
fn bridge_v14_round_trip_byte_identical() {
    // Acceptance #5: emit -> parse -> emit -> byte-identical.
    let m1 = mint_bridge_v14(&canonical_fixture_args());

    // Parse the canonical bytes back to JSON.
    let parsed: Json = serde_json::from_slice(&m1.canonical_bytes).expect("parse JCS");

    // Re-emit JCS. Going Json -> Value -> encode_jcs is the canonical
    // path the cross-language conformance tests use.
    let v = json_to_value(&parsed);
    let reemitted = encode_jcs(&v);

    let original = std::str::from_utf8(&m1.canonical_bytes).expect("utf-8");
    assert_eq!(
        reemitted, original,
        "v1.4 bridge JCS bytes MUST be byte-identical across emit/parse/emit"
    );

    // Also verify the BLAKE3-512 hash is stable across re-emission.
    let hash_first = blake3_512_of(m1.canonical_bytes.as_slice());
    let hash_second = blake3_512_of(reemitted.as_bytes());
    assert_eq!(hash_first, hash_second);
}

#[test]
fn bridge_v14_omits_none_metadata_fields_from_jcs_bytes() {
    // Spec §1.R2: omitted axes do NOT appear in the JCS bytes.
    // Not as `null`, not as placeholder strings.
    let m = mint_bridge_v14(&canonical_fixture_args());
    let bytes = std::str::from_utf8(&m.canonical_bytes).expect("utf-8");

    assert!(
        !bytes.contains("targetWitnessCid"),
        "targetWitnessCid was None; MUST be absent from JCS bytes"
    );
    assert!(
        !bytes.contains("targetBinaryCid"),
        "targetBinaryCid was None; MUST be absent from JCS bytes"
    );
    assert!(
        !bytes.contains("targetContractSetCid"),
        "targetContractSetCid was None; MUST be absent from JCS bytes"
    );
    assert!(
        !bytes.contains("null"),
        "no null literal MUST appear in v1.4 bridge JCS bytes"
    );
    assert!(
        !bytes.contains("pending-"),
        "no `pending-*` placeholder MUST appear (spec §1.R2)"
    );
    assert!(
        !bytes.contains("deferred:"),
        "no `deferred:*` placeholder MUST appear (spec §1.R2)"
    );
}

#[test]
fn bridge_v14_target_tagged_union_shape() {
    // Spec §1.R1: `target` is a JSON OBJECT with a `kind` discriminator,
    // NOT a bare string.
    let m = mint_bridge_v14(&canonical_fixture_args());
    let env: Json = serde_json::from_slice(&m.canonical_bytes).expect("parse");

    let target = env
        .pointer("/header/target")
        .expect("header.target present");
    assert!(target.is_object(), "target MUST be an object, not a string");
    assert_eq!(
        target.pointer("/kind").and_then(|v| v.as_str()),
        Some("contract")
    );
    assert!(target
        .pointer("/cid")
        .and_then(|v| v.as_str())
        .is_some());
}

#[test]
fn bridge_v14_target_contract_set_variant() {
    // Spec §1.R1: `kind: "contractSet"` is the second variant.
    let mut args = canonical_fixture_args();
    args.target = BridgeTargetV14::ContractSet {
        cid: "blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222".into(),
    };
    let m = mint_bridge_v14(&args);
    let env: Json = serde_json::from_slice(&m.canonical_bytes).expect("parse");
    assert_eq!(
        env.pointer("/header/target/kind").and_then(|v| v.as_str()),
        Some("contractSet")
    );
}

#[test]
fn bridge_v14_header_carries_seven_canonical_fields() {
    // Spec §1.R3: header carries the contract-axis claim only.
    // The seven fields are: schemaVersion, kind, name, sourceSymbol,
    // sourceLayer, sourceContractCid, target.
    let m = mint_bridge_v14(&canonical_fixture_args());
    let env: Json = serde_json::from_slice(&m.canonical_bytes).expect("parse");
    let header = env
        .pointer("/header")
        .and_then(|v| v.as_object())
        .expect("header object");

    let mut keys: Vec<&str> = header.keys().map(|k| k.as_str()).collect();
    keys.sort();
    let mut expected = vec![
        "schemaVersion",
        "kind",
        "name",
        "sourceSymbol",
        "sourceLayer",
        "sourceContractCid",
        "target",
    ];
    expected.sort();
    assert_eq!(keys, expected);

    // schemaVersion is "1" per substrate-layers spec §1.
    assert_eq!(
        header.get("schemaVersion").and_then(|v| v.as_str()),
        Some("1")
    );
    assert_eq!(header.get("kind").and_then(|v| v.as_str()), Some("bridge"));
}

#[test]
fn bridge_v14_top_level_layered_shape() {
    // Substrate-layers spec §1: every memento has exactly three
    // top-level keys: envelope, header, metadata.
    let m = mint_bridge_v14(&canonical_fixture_args());
    let env: Json = serde_json::from_slice(&m.canonical_bytes).expect("parse");
    let mut top: Vec<&str> = env
        .as_object()
        .expect("top-level object")
        .keys()
        .map(|k| k.as_str())
        .collect();
    top.sort();
    assert_eq!(top, vec!["envelope", "header", "metadata"]);

    let envelope = env
        .pointer("/envelope")
        .and_then(|v| v.as_object())
        .expect("envelope object");
    assert!(envelope.contains_key("signer"));
    assert!(envelope.contains_key("declaredAt"));
    assert!(envelope.contains_key("signature"));
}

#[test]
fn bridge_v14_canonical_fixture_bytes_pinned() {
    // PINS the conformance/fixtures.toml `bridge_decl_v1_4` entry.
    // If you edit `mint_bridge_v14` and this test fires, you have
    // changed the wire grammar. Update the fixture bytes/hash AND
    // bump the catalog if the change propagates per the protocol
    // catalog versioning rules.
    //
    // Print on failure so the new bytes/hash are visible in CI logs.
    let m = mint_bridge_v14(&canonical_fixture_args());
    let bytes = std::str::from_utf8(&m.canonical_bytes).expect("utf-8");
    let hash = blake3_512_of(m.canonical_bytes.as_slice());

    // The expected values below MUST match `conformance/fixtures.toml`
    // entry `bridge_decl_v1_4`. They are stamped here too so an
    // accidental drift surfaces inside this crate's test suite, not
    // only when the cross-language fixture loaders run.
    let expected_jcs = r#"{"envelope":{"declaredAt":"2026-05-03T00:00:00.000Z","signature":"ed25519:RMYnQheAjTz7Ydq2yr1yl2Ramj/5G4eyhIb0DH1u3HKI7+95UAZnB3hEdgz0wqc+9BSe38SVTc1CmvyK8YVIBw==","signer":"ed25519:IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI="},"header":{"kind":"bridge","name":"rust-canonical-bridge-fixture","schemaVersion":"1","sourceContractCid":"blake3-512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000","sourceLayer":"rust-kit","sourceSymbol":"parseInt","target":{"cid":"blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111","kind":"contract"}},"metadata":{"producedAt":"2026-05-03T00:00:00.000Z","producedBy":"provekit-canonical-reference@v1.4","targetLayer":"rust-kit"}}"#;
    let expected_hash = "blake3-512:660ce98742d1f7ff326c994e4f6aba4d396d7fba0914db91a142c489e6d0901a7eff0ca206ce49bfa5b71eda289a138049fa8cf6461c5ef353703a78c0966cf2";

    if bytes != expected_jcs {
        eprintln!("==== ACTUAL bridge_decl_v1_4 JCS bytes ====");
        eprintln!("{}", bytes);
        eprintln!("==== ACTUAL bridge_decl_v1_4 hash ====");
        eprintln!("{}", hash);
    }
    assert_eq!(bytes, expected_jcs, "v1.4 fixture JCS bytes drift");
    assert_eq!(hash, expected_hash, "v1.4 fixture hash drift");
}
