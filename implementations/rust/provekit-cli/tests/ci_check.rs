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

fn write_file(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("mkdir parent");
    }
    fs::write(path, text).expect("write file");
}

fn raw_cid(text: &str) -> String {
    blake3_512_of(text.as_bytes())
}

fn make_shadow_repo(suffix: &str) -> PathBuf {
    let repo = make_unique_dir(suffix);

    write_file(
        &repo.join("protocol/specs/2026-04-30-protocol-catalog.json"),
        r#"{"protocol":"provekit","version":"test"}"#,
    );
    write_file(
        &repo.join("protocol/specs/2026-05-07-content-addressed-ci-protocol.md"),
        "# Content-Addressed CI Protocol\n\nCICP test spec v1.\n",
    );
    write_file(
        &repo.join("protocol/conformance/cicp/vectors.json"),
        r#"{"vectors":[]}"#,
    );
    write_file(
        &repo.join("Makefile"),
        "prove-rust:\n\tcargo test\nprove-go:\n\tgo test ./...\n",
    );
    write_file(&repo.join(".github/workflows/ci.yml"), "name: CI\n");

    write_file(
        &repo.join("implementations/rust/Cargo.toml"),
        "[workspace]\n",
    );
    write_file(&repo.join("implementations/rust/Cargo.lock"), "# lock\n");
    write_file(
        &repo.join("implementations/rust/src/lib.rs"),
        "pub fn rust_kit() {}\n",
    );

    write_file(
        &repo.join("implementations/go/go.mod"),
        "module example.com/provekit-go\n",
    );
    write_file(&repo.join("implementations/go/go.sum"), "# sum\n");
    write_file(
        &repo.join("implementations/go/main.go"),
        "package main\nfunc main() {}\n",
    );

    repo
}

fn run_ci_shadow(repo: &Path, kit: &str) -> Json {
    let out_dir = repo.join(".provekit/ci-shadow").join(kit);
    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("ci")
        .arg("shadow")
        .arg("--repo")
        .arg(repo)
        .arg("--kit")
        .arg(kit)
        .arg("--out-dir")
        .arg(out_dir)
        .arg("--json")
        .output()
        .expect("run provekit ci shadow");

    assert!(
        output.status.success(),
        "status={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    serde_json::from_slice(&output.stdout).expect("shadow summary JSON")
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
fn ci_shadow_emits_distinct_per_kit_blast_radii_with_protocol_inputs() {
    let repo = make_shadow_repo("shadow-distinct");

    let rust = run_ci_shadow(&repo, "rust");
    let go = run_ci_shadow(&repo, "go");

    assert_eq!(rust["kind"], "CIShadow");
    assert_eq!(rust["kit"], "rust");
    assert_eq!(rust["wouldSkip"], false);
    assert_eq!(rust["blastRadius"]["jobKey"], "provekit/ci/rust");
    assert_eq!(go["blastRadius"]["jobKey"], "provekit/ci/go");

    assert_ne!(
        rust["blastRadiusCid"], go["blastRadiusCid"],
        "each kit must have its own blast-radius CID"
    );
    assert_ne!(
        rust["blastRadius"]["sourceClosureCid"], go["blastRadius"]["sourceClosureCid"],
        "source closures must stay kit-specific"
    );

    let rust_body_path = repo.join(
        rust["blastRadiusPath"]
            .as_str()
            .expect("blastRadiusPath string"),
    );
    assert!(
        rust_body_path.exists(),
        "body path exists: {rust_body_path:?}"
    );

    let body: Json =
        serde_json::from_slice(&fs::read(&rust_body_path).expect("read blast-radius body"))
            .expect("parse body");
    let relevant = body["relevantSpecCids"]
        .as_array()
        .expect("relevantSpecCids array");
    assert!(
        relevant.iter().any(|cid| cid
            == &Json::String(raw_cid(
                "# Content-Addressed CI Protocol\n\nCICP test spec v1.\n"
            ))),
        "CICP protocol spec CID must be in the kit blast radius"
    );
    assert!(
        relevant.iter().any(|cid| cid
            == &Json::String(raw_cid(r#"{"protocol":"provekit","version":"test"}"#))),
        "protocol catalog spec file CID must be in the kit blast radius"
    );

    let _ = fs::remove_dir_all(&repo);
}

#[test]
fn ci_shadow_protocol_spec_change_invalidates_same_kit_radius() {
    let repo = make_shadow_repo("shadow-protocol-change");

    let before = run_ci_shadow(&repo, "rust");
    write_file(
        &repo.join("protocol/specs/2026-05-07-content-addressed-ci-protocol.md"),
        "# Content-Addressed CI Protocol\n\nCICP test spec v2.\n",
    );
    let after = run_ci_shadow(&repo, "rust");

    assert_ne!(
        before["blastRadiusCid"], after["blastRadiusCid"],
        "protocol spec edits must invalidate kit blast-radius CIDs"
    );
    assert_ne!(
        before["blastRadius"]["relevantSpecCids"], after["blastRadius"]["relevantSpecCids"],
        "the protocol spec CID set should reflect the edited spec"
    );

    let _ = fs::remove_dir_all(&repo);
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
