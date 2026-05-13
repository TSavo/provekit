// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use serde_json::{json, Value as Json};
use std::sync::Arc;

fn make_unique_dir(suffix: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("provekit-protocol-evolve-{stamp}-{suffix}"));
    fs::create_dir_all(&dir).expect("mkdir");
    dir
}

fn to_cvalue(v: &Json) -> Arc<CValue> {
    match v {
        Json::Null => CValue::null(),
        Json::Bool(b) => CValue::boolean(*b),
        Json::Number(n) => CValue::integer(n.as_i64().expect("integer JSON number")),
        Json::String(s) => CValue::string(s.clone()),
        Json::Array(items) => CValue::array(items.iter().map(to_cvalue).collect()),
        Json::Object(map) => CValue::object(map.iter().map(|(k, v)| (k.clone(), to_cvalue(v)))),
    }
}

fn jcs_cid(v: &Json) -> String {
    let jcs = encode_jcs(&to_cvalue(v));
    blake3_512_of(jcs.as_bytes())
}

fn write_json(path: &Path, value: &Json) {
    let mut bytes = serde_json::to_string_pretty(value).expect("serialize");
    bytes.push('\n');
    fs::write(path, bytes).expect("write json");
}

#[test]
fn protocol_evolve_emits_body_and_witness_for_extension_only_patch() {
    let dir = make_unique_dir("extension-only");
    let from_catalog = dir.join("from.json");
    let to_catalog = dir.join("to.json");
    let out_dir = dir.join("out");
    let policy = dir.join("policy.json");
    let verifier = dir.join("verifier.json");
    let spec = dir.join("extra-protocol.md");

    fs::write(&spec, "# Extra Protocol\n\nDraft extension.\n").expect("write spec");
    let spec_cid = blake3_512_of(&fs::read(&spec).expect("read spec"));

    let from = json!({
        "kind": "catalog",
        "name": "provekit-protocol",
        "version": "v1.0.0-test",
        "algorithms": {
            "hash": ["blake3-512"],
            "signature": ["ed25519"],
            "pubkey": ["ed25519"]
        },
        "properties": {
            "base-protocol": "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        },
        "declaredAt": "2026-05-07T00:00:00Z"
    });
    let to = json!({
        "kind": "catalog",
        "name": "provekit-protocol",
        "version": "v1.0.1-test",
        "algorithms": {
            "hash": ["blake3-512"],
            "signature": ["ed25519"],
            "pubkey": ["ed25519"]
        },
        "properties": {
            "base-protocol": "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "extra-protocol": spec_cid
        },
        "declaredAt": "2026-05-07T00:00:00Z"
    });
    let policy_json = json!({
        "kind": "ProtocolEvolutionPolicy",
        "schemaVersion": "1",
        "name": "test-policy"
    });
    let verifier_json = json!({
        "kind": "ProtocolEvolutionVerifier",
        "schemaVersion": "1",
        "name": "test-verifier"
    });

    write_json(&from_catalog, &from);
    write_json(&to_catalog, &to);
    write_json(&policy, &policy_json);
    write_json(&verifier, &verifier_json);

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("protocol")
        .arg("evolve")
        .arg("--from")
        .arg(&from_catalog)
        .arg("--to")
        .arg(&to_catalog)
        .arg("--changed-spec")
        .arg(format!("extra-protocol={}", spec.display()))
        .arg("--policy")
        .arg(&policy)
        .arg("--verifier")
        .arg(&verifier)
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--json")
        .output()
        .expect("run provekit protocol evolve");

    assert!(
        output.status.success(),
        "status={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let summary: Json = serde_json::from_slice(&output.stdout).expect("summary JSON");
    assert_eq!(summary["kind"], "ProtocolEvolutionSummary");
    assert_eq!(summary["fromCatalogCid"], jcs_cid(&from));
    assert_eq!(summary["toCatalogCid"], jcs_cid(&to));
    assert_eq!(summary["changeClass"], "extension-only");

    let body_path = out_dir.join("protocol-evolution.body.json");
    let witness_path = out_dir.join("protocol-evolution.witness.json");
    assert!(body_path.exists(), "body file should be written");
    assert!(witness_path.exists(), "witness file should be written");

    let body: Json =
        serde_json::from_slice(&fs::read(&body_path).expect("read body")).expect("body JSON");
    assert_eq!(body["kind"], "ProtocolEvolutionBodyClaim");
    assert_eq!(body["fromVersionLabel"], "v1.0.0-test");
    assert_eq!(body["toVersionLabel"], "v1.0.1-test");
    assert_eq!(
        body["changeSet"]["added"][0]["propertyKey"],
        "extra-protocol"
    );
    assert_eq!(body["changeSet"]["added"][0]["toCid"], spec_cid);
    assert_eq!(body["compatibility"]["migrationRequired"], false);

    let witness: Json = serde_json::from_slice(&fs::read(&witness_path).expect("read witness"))
        .expect("witness JSON");
    assert_eq!(witness["kind"], "TruthDischargeWitness");
    assert_eq!(witness["claimKind"], "protocol-evolution");
    assert_eq!(witness["result"], true);
    assert_eq!(witness["claimBodyCid"], summary["bodyCid"]);
    assert_eq!(witness["claimBodyCid"], jcs_cid(&body));

    let check_output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("protocol")
        .arg("check-evolution")
        .arg("--body")
        .arg(&body_path)
        .arg("--from")
        .arg(&from_catalog)
        .arg("--to")
        .arg(&to_catalog)
        .arg("--policy")
        .arg(&policy)
        .arg("--verifier")
        .arg(&verifier)
        .arg("--catalog-diff")
        .arg(out_dir.join("catalog-diff.json"))
        .arg("--json")
        .output()
        .expect("run provekit protocol check-evolution");

    assert!(
        check_output.status.success(),
        "status={:?}\nstdout={}\nstderr={}",
        check_output.status.code(),
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );
    let check_summary: Json =
        serde_json::from_slice(&check_output.stdout).expect("check summary JSON");
    assert_eq!(check_summary["kind"], "ProtocolEvolutionCheck");
    assert_eq!(check_summary["ok"], true);
    assert_eq!(check_summary["bodyCid"], jcs_cid(&body));
    assert_eq!(check_summary["fromCatalogCid"], jcs_cid(&from));
    assert_eq!(check_summary["toCatalogCid"], jcs_cid(&to));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn protocol_evolve_refuses_extension_only_core_substrate_delta() {
    let dir = make_unique_dir("extension-only-core-delta");
    let from_catalog = dir.join("from.json");
    let to_catalog = dir.join("to.json");
    let out_dir = dir.join("out");
    let policy = dir.join("policy.json");
    let verifier = dir.join("verifier.json");
    let spec = dir.join("proof-file-format.md");

    fs::write(&spec, "# Proof File Format\n\nChanged core format.\n").expect("write spec");
    let spec_cid = blake3_512_of(&fs::read(&spec).expect("read spec"));

    let from = json!({
        "kind": "catalog",
        "name": "provekit-protocol",
        "version": "v1.0.0-test",
        "algorithms": {
            "hash": ["blake3-512"],
            "signature": ["ed25519"],
            "pubkey": ["ed25519"]
        },
        "properties": {
            "proof-file-format": "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        },
        "declaredAt": "2026-05-07T00:00:00Z"
    });
    let to = json!({
        "kind": "catalog",
        "name": "provekit-protocol",
        "version": "v1.0.1-test",
        "algorithms": {
            "hash": ["blake3-512"],
            "signature": ["ed25519"],
            "pubkey": ["ed25519"]
        },
        "properties": {
            "proof-file-format": spec_cid
        },
        "declaredAt": "2026-05-07T00:00:00Z"
    });
    let policy_json = json!({
        "kind": "ProtocolEvolutionPolicy",
        "schemaVersion": "1",
        "name": "test-policy"
    });
    let verifier_json = json!({
        "kind": "ProtocolEvolutionVerifier",
        "schemaVersion": "1",
        "name": "test-verifier"
    });

    write_json(&from_catalog, &from);
    write_json(&to_catalog, &to);
    write_json(&policy, &policy_json);
    write_json(&verifier, &verifier_json);

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("protocol")
        .arg("evolve")
        .arg("--from")
        .arg(&from_catalog)
        .arg("--to")
        .arg(&to_catalog)
        .arg("--changed-spec")
        .arg(format!("proof-file-format={}", spec.display()))
        .arg("--policy")
        .arg(&policy)
        .arg("--verifier")
        .arg(&verifier)
        .arg("--change-class")
        .arg("extension-only")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--json")
        .output()
        .expect("run provekit protocol evolve");

    assert!(
        !output.status.success(),
        "extension-only core substrate delta should be refused\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("extension-only cannot change core substrate property `proof-file-format`"),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}
