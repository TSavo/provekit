// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use serde_json::{json, Value as Json};

fn make_unique_dir(suffix: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("provekit-ci-check-{stamp}-{suffix}"));
    fs::create_dir_all(&dir).expect("mkdir");
    dir
}

fn cid(ch: char) -> String {
    format!("blake3-512:{}", ch.to_string().repeat(128))
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

fn blast_radius_body() -> Json {
    json!({
        "kind": "CIBlastRadius",
        "schemaVersion": "1",
        "jobKey": "provekit/conformance/rust",
        "subjectKind": "kit",
        "subject": "rust",
        "protocolCatalogCid": cid('1'),
        "jobDefinitionCid": cid('2'),
        "commandCid": cid('3'),
        "runnerIdentityCid": cid('4'),
        "toolchainCids": [cid('5')],
        "sourceClosureCid": cid('6'),
        "lockfileCids": [cid('7')],
        "generatedInputCids": [cid('8')],
        "fixtureCids": [cid('9')],
        "relevantSpecCids": [cid('a')],
        "policyCid": cid('b'),
        "nondeterminism": {
            "network": "forbidden",
            "clock": "forbidden",
            "secrets": "forbidden",
            "randomness": "forbidden"
        },
        "inputCids": [
            cid('1'), cid('2'), cid('3'), cid('4'), cid('5'), cid('6'),
            cid('7'), cid('8'), cid('9'), cid('a'), cid('b')
        ]
    })
}

#[test]
fn ci_check_accepts_valid_blast_radius_body() {
    let dir = make_unique_dir("accept");
    let body_path = dir.join("blast-radius.json");
    let body = blast_radius_body();
    write_json(&body_path, &body);

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("ci")
        .arg("check")
        .arg("--body")
        .arg(&body_path)
        .arg("--json")
        .output()
        .expect("run provekit ci check");

    assert!(
        output.status.success(),
        "status={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let summary: Json = serde_json::from_slice(&output.stdout).expect("summary JSON");
    assert_eq!(summary["kind"], "CICheck");
    assert_eq!(summary["ok"], true);
    assert_eq!(summary["bodyKind"], "CIBlastRadius");
    assert_eq!(summary["bodyCid"], jcs_cid(&body));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn ci_check_refuses_open_input_closure() {
    let dir = make_unique_dir("refuse");
    let body_path = dir.join("blast-radius-open.json");
    let mut body = blast_radius_body();
    body["inputCids"] = json!([cid('1')]);
    write_json(&body_path, &body);

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("ci")
        .arg("check")
        .arg("--body")
        .arg(&body_path)
        .output()
        .expect("run provekit ci check");

    assert!(
        !output.status.success(),
        "open input closure should be refused\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("inputCids missing required CID"),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}
