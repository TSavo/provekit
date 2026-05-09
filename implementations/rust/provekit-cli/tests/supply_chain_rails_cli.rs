// SPDX-License-Identifier: Apache-2.0

use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use provekit_canonicalizer::blake3_512_of;
use serde_json::{json, Value};

fn tmp_dir(name: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("provekit-scr-cli-{stamp}-{name}"));
    fs::create_dir_all(&dir).expect("mkdir temp dir");
    dir
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("mkdir parent");
    }
    fs::write(path, text).expect("write file");
}

fn write_executable(path: &Path, text: &str) {
    write(path, text);
    #[cfg(unix)]
    {
        let mut perms = fs::metadata(path)
            .unwrap_or_else(|e| panic!("stat {}: {e}", path.display()))
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)
            .unwrap_or_else(|e| panic!("chmod {}: {e}", path.display()));
    }
}

fn write_json(path: &Path, value: &Value) {
    let mut text = serde_json::to_string_pretty(value).expect("serialize json");
    text.push('\n');
    write(path, &text);
}

fn run_provekit(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_provekit"))
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("spawn provekit {}: {e}", args.join(" ")))
}

fn parse_stdout(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "parse stdout JSON: {e}\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn make_npm_package(root: &Path, name: &str, version: &str, tarball: &str) {
    write_json(
        &root.join("package.json"),
        &json!({
            "name": name,
            "version": version,
            "description": "Small JSON boundary utility with explicit ProvekIt contracts",
            "main": "index.js",
            "scripts": {"test": "node test.js"}
        }),
    );
    write(
        &root.join("index.js"),
        "exports.parseJson = (input) => JSON.parse(input);\n",
    );
    write(&root.join("package.tgz"), tarball);
}

#[test]
fn package_inspect_reports_npm_identity_and_binary_cid() {
    let dir = tmp_dir("package-inspect");
    make_npm_package(&dir, "safe-json", "1.4.2", "safe tarball bytes");
    let manifest_dir = dir.join(".provekit/lift/npm-test-inspector");
    fs::create_dir_all(&manifest_dir).expect("create manifest dir");
    write(
        &dir.join(".provekit/config.toml"),
        "[authoring.lift]\nsurface = \"npm-test-inspector\"\n",
    );
    let binary_cid = blake3_512_of("safe tarball bytes".as_bytes());
    let plugin = dir.join("npm-test-inspector.sh");
    write_executable(
        &plugin,
        &format!(
            r#"#!/usr/bin/env bash
set -euo pipefail
while IFS= read -r line; do
  if [[ "$line" == *'"method":"initialize"'* ]]; then
    printf '%s\n' '{{"jsonrpc":"2.0","id":1,"result":{{"name":"npm-test-inspector","version":"0.1.0","protocol_version":"provekit-lift/1","capabilities":{{"authoring_surfaces":["npm-test-inspector"],"ir_version":"v1.1.0","emits_signed_mementos":false,"identify_result_kinds":["package-inspection-document"]}}}}}}'
  elif [[ "$line" == *'"method":"lift"'* ]]; then
    printf '%s\n' '{{"jsonrpc":"2.0","id":2,"result":{{"kind":"package-inspection-document","ecosystem":"npm","package":{{"name":"safe-json","version":"1.4.2"}},"artifact":{{"path":"package.tgz","binaryCid":"{binary_cid}","bytes":18}},"conventionalReceipts":{{"packageIdentity":"green"}},"admission":{{"status":"not-decided","reason":"package identity is not contract admission"}},"diagnostics":[]}}}}'
  elif [[ "$line" == *'"method":"shutdown"'* ]]; then
    printf '%s\n' '{{"jsonrpc":"2.0","id":3,"result":null}}'
    exit 0
  fi
done
"#
        ),
    );
    write(
        &manifest_dir.join("manifest.toml"),
        &format!(
            "name = \"npm-test-inspector\"\ncommand = [\"{}\"]\n",
            plugin.display()
        ),
    );

    let output = run_provekit(&[
        "package",
        "inspect",
        dir.to_str().unwrap(),
        "--json",
        "--quiet",
    ]);

    assert!(
        output.status.success(),
        "package inspect failed\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_stdout(&output);
    assert_eq!(json["kind"], "package-inspection-document");
    assert_eq!(json["package"]["name"], "safe-json");
    assert_eq!(json["package"]["version"], "1.4.2");
    assert_eq!(json["artifact"]["binaryCid"], binary_cid);
    assert_eq!(json["conventionalReceipts"]["packageIdentity"], "green");
}

#[test]
fn package_inspect_delegates_to_configured_lifter() {
    let dir = tmp_dir("delegated-package-inspect");
    let project = dir.join("custom-release");
    let manifest_dir = project.join(".provekit/lift/custom-supply");
    fs::create_dir_all(&manifest_dir).expect("create manifest dir");
    write(
        &project.join(".provekit/config.toml"),
        "[authoring.lift]\nsurface = \"custom-supply\"\n",
    );
    let plugin = dir.join("custom-supply-lifter.sh");
    write_executable(
        &plugin,
        r#"#!/usr/bin/env bash
set -euo pipefail
while IFS= read -r line; do
  if [[ "$line" == *'"method":"initialize"'* ]]; then
    printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"name":"custom-supply","version":"0.1.0","protocol_version":"provekit-lift/1","capabilities":{"authoring_surfaces":["custom-supply"],"ir_version":"v1.1.0","emits_signed_mementos":false,"identify_result_kinds":["package-inspection-document"]}}}'
  elif [[ "$line" == *'"method":"lift"'* ]]; then
    if [[ "$line" != *'"layer":"identify-only"'* ]]; then
      printf '%s\n' '{"jsonrpc":"2.0","id":2,"error":{"code":1006,"message":"expected identify-only package inspection"}}'
    else
      printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"kind":"package-inspection-document","ecosystem":"custom-supply","package":{"name":"sensor-firmware","version":"2026.05"},"artifact":{"path":"firmware.img","binaryCid":"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","bytes":128},"ci":{"inputClosureCid":"blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","closure":["firmware.img","contract-set.json"]},"conventionalReceipts":{"vendorSignature":"green"},"admission":{"status":"not-decided","reason":"delegated inspector only names supply-chain rails"},"delegatedBy":"custom-supply-lifter","diagnostics":[]}}'
    fi
  elif [[ "$line" == *'"method":"shutdown"'* ]]; then
    printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":null}'
    exit 0
  fi
done
"#,
    );
    write(
        &manifest_dir.join("manifest.toml"),
        &format!(
            "name = \"custom-supply\"\ncommand = [\"{}\"]\n",
            plugin.display()
        ),
    );

    let output = run_provekit(&[
        "package",
        "inspect",
        project.to_str().unwrap(),
        "--json",
        "--quiet",
    ]);

    assert!(
        output.status.success(),
        "delegated package inspect failed\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_stdout(&output);
    assert_eq!(json["kind"], "package-inspection-document");
    assert_eq!(json["ecosystem"], "custom-supply");
    assert_eq!(json["delegatedBy"], "custom-supply-lifter");
    assert_eq!(json["package"]["name"], "sensor-firmware");
}

#[test]
fn version_check_extension_rejects_removed_contract() {
    let dir = tmp_dir("version-extension");
    let previous = dir.join("safe-json-1.4.1.json");
    let candidate = dir.join("safe-json-1.4.2-weakened.json");
    write_json(
        &previous,
        &json!({
            "package": {"name": "safe-json", "version": "1.4.1"},
            "contractSetCid": "blake3-512:previous",
            "contracts": [
                "parse.deterministic",
                "runtime.no-env-secret-read"
            ]
        }),
    );
    write_json(
        &candidate,
        &json!({
            "package": {"name": "safe-json", "version": "1.4.2"},
            "previousContractSetCid": "blake3-512:previous",
            "contractSetCid": "blake3-512:candidate",
            "contracts": ["parse.deterministic"]
        }),
    );

    let output = run_provekit(&[
        "version",
        "check-extension",
        "--previous",
        previous.to_str().unwrap(),
        "--candidate",
        candidate.to_str().unwrap(),
        "--json",
        "--quiet",
    ]);

    assert!(
        !output.status.success(),
        "weakened contract set should be rejected\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_stdout(&output);
    assert_eq!(json["ok"], false);
    assert_eq!(json["verdict"], "rejected");
    assert_eq!(
        json["missingContracts"],
        json!(["runtime.no-env-secret-read"])
    );
    assert_eq!(json["rule"], "oldSet subset newSet");
}

#[test]
fn version_check_extension_rejects_cross_package_reuse() {
    let dir = tmp_dir("version-cross-package");
    let previous = dir.join("safe-json-1.4.1.json");
    let candidate = dir.join("unsafe-json-1.4.2.json");
    write_json(
        &previous,
        &json!({
            "ecosystem": "npm",
            "package": {"name": "safe-json", "version": "1.4.1"},
            "contractSetCid": "blake3-512:previous",
            "contracts": ["parse.deterministic"]
        }),
    );
    write_json(
        &candidate,
        &json!({
            "ecosystem": "npm",
            "package": {"name": "unsafe-json", "version": "1.4.2"},
            "previousContractSetCid": "blake3-512:previous",
            "contractSetCid": "blake3-512:candidate",
            "contracts": ["parse.deterministic"]
        }),
    );

    let output = run_provekit(&[
        "version",
        "check-extension",
        "--previous",
        previous.to_str().unwrap(),
        "--candidate",
        candidate.to_str().unwrap(),
        "--json",
        "--quiet",
    ]);

    assert!(
        !output.status.success(),
        "cross-package candidate should be rejected\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_stdout(&output);
    assert_eq!(json["ok"], false);
    assert_eq!(json["identityOk"], false);
}

#[test]
fn version_check_extension_rejects_missing_candidate_contract_set_cid() {
    let dir = tmp_dir("version-missing-candidate-cid");
    let previous = dir.join("safe-json-1.4.1.json");
    let candidate = dir.join("safe-json-1.4.2.json");
    write_json(
        &previous,
        &json!({
            "package": {"name": "safe-json", "version": "1.4.1"},
            "contractSetCid": "blake3-512:previous",
            "contracts": ["parse.deterministic"]
        }),
    );
    write_json(
        &candidate,
        &json!({
            "package": {"name": "safe-json", "version": "1.4.2"},
            "previousContractSetCid": "blake3-512:previous",
            "contracts": ["parse.deterministic"]
        }),
    );

    let output = run_provekit(&[
        "version",
        "check-extension",
        "--previous",
        previous.to_str().unwrap(),
        "--candidate",
        candidate.to_str().unwrap(),
        "--json",
        "--quiet",
    ]);

    assert!(
        !output.status.success(),
        "candidate missing contractSetCid should be rejected\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_stdout(&output);
    assert_eq!(json["ok"], false);
    assert_eq!(json["candidateContractSetCidPresent"], false);
}

#[test]
fn verify_artifact_rejects_binary_cid_mismatch() {
    let dir = tmp_dir("binary-verify");
    let artifact = dir.join("package.tgz");
    let proof = dir.join("release.json");
    write(&artifact, "poisoned bytes");
    write_json(
        &proof,
        &json!({
            "kind": "PackageReleaseReceipt",
            "package": {"name": "safe-json", "version": "1.4.2"},
            "binaryCid": blake3_512_of("clean bytes".as_bytes()),
            "policyCid": "builtin:supply-chain-rails/npm"
        }),
    );

    let output = run_provekit(&[
        "verify",
        "--artifact",
        artifact.to_str().unwrap(),
        "--proof",
        proof.to_str().unwrap(),
        "--json",
        "--quiet",
    ]);

    assert!(
        !output.status.success(),
        "binary mismatch should be rejected\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_stdout(&output);
    assert_eq!(json["ok"], false);
    assert_eq!(json["verdict"], "rejected");
    assert_eq!(json["reason"], "binaryCid mismatch");
    assert_eq!(
        json["observedBinaryCid"],
        blake3_512_of("poisoned bytes".as_bytes())
    );
}

#[test]
fn verify_artifact_and_policy_runs_both_rails() {
    let dir = tmp_dir("binary-policy-verify");
    let artifact = dir.join("package.tgz");
    let proof = dir.join("release.json");
    let policy = dir.join("policy.json");
    write(&artifact, "poisoned bytes");
    write_json(
        &proof,
        &json!({
            "kind": "PackageReleaseReceipt",
            "binaryCid": blake3_512_of("clean bytes".as_bytes()),
            "policyCid": "builtin:supply-chain-rails/npm"
        }),
    );
    write_json(
        &policy,
        &json!({
            "policyCid": "builtin:supply-chain-rails/npm"
        }),
    );

    let output = run_provekit(&[
        "verify",
        "--artifact",
        artifact.to_str().unwrap(),
        "--proof",
        proof.to_str().unwrap(),
        "--policy",
        policy.to_str().unwrap(),
        "--json",
        "--quiet",
    ]);

    assert!(
        !output.status.success(),
        "artifact mismatch must still reject when policy matches\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_stdout(&output);
    assert_eq!(json["ok"], false);
    assert_eq!(json["policy"]["ok"], true);
    assert_eq!(json["artifact"]["ok"], false);
    assert_eq!(json["artifact"]["reason"], "binaryCid mismatch");
}
