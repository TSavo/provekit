// SPDX-License-Identifier: Apache-2.0
//
// Voltron pool assembly: provekit prove must ask configured kits for the
// dependency .proof files they resolve through their own package managers, then
// union those files into the verifier pool without teaching the substrate cargo,
// npm, classpath, sys.path, or any other platform graph.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use provekit_canonicalizer::{blake3_512_of, encode_jcs};
use provekit_proof_envelope::{
    build_proof_envelope, ed25519_pubkey_string, Ed25519Seed, ProofEnvelopeInput,
};
use provekit_verifier::{Runner, RunnerConfig};
use serde_json::{json, Value as Json};

fn unique_dir(suffix: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("provekit-dep-proof-{stamp}-{suffix}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

fn int_sort() -> Json {
    json!({"kind": "primitive", "name": "Int"})
}

fn int_const(n: i64) -> Json {
    json!({"kind": "const", "value": n, "sort": int_sort()})
}

fn var(name: &str) -> Json {
    json!({"kind": "var", "name": name})
}

fn flat_member(mut env: Json) -> (String, Vec<u8>) {
    if let Json::Object(map) = &mut env {
        map.remove("cid");
        map.remove("producerSignature");
    }
    let canonical = json_to_canonical_jcs(&env);
    let cid = blake3_512_of(canonical.as_bytes());
    (cid, canonical.into_bytes())
}

fn json_to_canonical_jcs(j: &Json) -> String {
    fn to_cv(j: &Json) -> std::sync::Arc<provekit_canonicalizer::Value> {
        use provekit_canonicalizer::Value as CV;
        match j {
            Json::Null => CV::null(),
            Json::Bool(b) => CV::boolean(*b),
            Json::Number(n) => CV::integer(n.as_i64().unwrap_or(0)),
            Json::String(s) => CV::string(s.clone()),
            Json::Array(items) => CV::array(items.iter().map(to_cv).collect()),
            Json::Object(map) => CV::object(
                map.iter()
                    .map(|(k, v)| (k.clone(), to_cv(v)))
                    .collect::<Vec<_>>(),
            ),
        }
    }
    encode_jcs(&to_cv(j))
}

fn write_proof(dir: &Path, name: &str, members: BTreeMap<String, Vec<u8>>) -> String {
    fs::create_dir_all(dir).expect("mkdir proof dir");
    let signer_seed: Ed25519Seed = [0x51u8; 32];
    let signer_pubkey = ed25519_pubkey_string(&signer_seed);
    let signer_cid = blake3_512_of(signer_pubkey.as_bytes());
    let built = build_proof_envelope(&ProofEnvelopeInput {
        name: name.to_string(),
        version: "1.0.0".into(),
        binary_cid: None,
        metadata: None,
        members,
        signer_cid,
        signer_seed,
        declared_at: "2026-05-27T00:00:00.000Z".into(),
    });
    let hex = built.cid.strip_prefix("blake3-512:").unwrap();
    fs::write(dir.join(format!("{hex}.proof")), &built.bytes).expect("write proof");
    built.cid
}

fn publish_vendor_positive_contract(vendor_dir: &Path) -> (String, String, PathBuf) {
    let target_env = json!({
        "evidence": {
            "kind": "contract",
            "body": {
                "contractName": "must_be_positive",
                "formals": ["x"],
                "formalSorts": [int_sort()],
                "pre": {
                    "kind": "atomic",
                    "name": ">=",
                    "args": [var("x"), int_const(0)]
                }
            }
        }
    });
    let (target_cid, target_bytes) = flat_member(target_env);
    let mut members = BTreeMap::new();
    members.insert(target_cid.clone(), target_bytes);
    let bundle_cid = write_proof(vendor_dir, "@vendor/must-be-positive", members);
    let proof_path = fs::read_dir(vendor_dir)
        .expect("read vendor proofs")
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().and_then(|s| s.to_str()) == Some("proof"))
        .expect("vendor proof exists");
    (target_cid, bundle_cid, proof_path)
}

fn publish_user_bridge(project_dir: &Path, target_cid: &str, target_bundle_cid: &str) {
    let source_env = json!({
        "evidence": {
            "kind": "contract",
            "body": {
                "contractName": "user_calls_vendor",
                "inv": {
                    "kind": "atomic",
                    "name": "observed",
                    "args": [{
                        "kind": "ctor",
                        "name": "must_be_positive",
                        "args": [int_const(-1)]
                    }]
                }
            }
        }
    });
    let (source_cid, source_bytes) = flat_member(source_env);
    let bridge_env = json!({
        "evidence": {
            "kind": "bridge",
            "body": {
                "sourceSymbol": "must_be_positive",
                "sourceLayer": "rust",
                "targetContractCid": target_cid,
                "targetProofCid": target_bundle_cid,
                "targetLayer": "rust-kit"
            }
        }
    });
    let (bridge_cid, bridge_bytes) = flat_member(bridge_env);
    let mut members = BTreeMap::new();
    members.insert(source_cid, source_bytes);
    members.insert(bridge_cid, bridge_bytes);
    write_proof(
        &project_dir.join(".provekit"),
        "@user/local-bridge",
        members,
    );
}

fn install_dependency_proof_stub(project_dir: &Path, proof_path: &Path) {
    let bin = project_dir.join("resolve-deps-stub.sh");
    fs::write(
        &bin,
        format!(
            "#!/bin/sh\nwhile IFS= read -r line; do\n  case \"$line\" in\n    *resolve_dependency_proofs*) echo '{{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{{\"proof_paths\":[\"{}\"]}}}}' ;;\n    *shutdown*) echo '{{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":null}}'; exit 0 ;;\n    *) echo '{{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{{}}}}' ;;\n  esac\ndone\n",
            proof_path.display()
        ),
    )
    .expect("write stub");
    let manifest_dir = project_dir.join(".provekit").join("realize").join("rust");
    fs::create_dir_all(&manifest_dir).expect("mkdir manifest");
    fs::write(
        manifest_dir.join("manifest.toml"),
        format!(
            "name = \"rust-dependency-proof-stub\"\nlibrary_tag = \"test\"\ncommand = [\"/bin/sh\", \"{}\"]\nworking_dir = \".\"\n",
            bin.display()
        ),
    )
    .expect("write manifest");
    provekit_cli::kit_dispatch::reset_kit_dispatch_registry_cache_for_tests();
}

fn z3_available() -> bool {
    std::process::Command::new("z3")
        .arg("-version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[test]
fn dependency_rpc_union_makes_vendor_contract_reachable() {
    let root = unique_dir("reachable");
    let project = root.join("user");
    let vendor = root.join("vendor");
    fs::create_dir_all(project.join(".provekit")).expect("mkdir project");
    let (target_cid, bundle_cid, proof_path) = publish_vendor_positive_contract(&vendor);
    publish_user_bridge(&project, &target_cid, &bundle_cid);
    install_dependency_proof_stub(&project, &proof_path);

    let dependency_proofs = provekit_cli::kit_dispatch::dependency_proof_paths_via_rpc(&project)
        .expect("resolve dependency proofs");
    assert_eq!(dependency_proofs, vec![proof_path.clone()]);

    let runner = Runner::new(RunnerConfig {
        project_root: project.clone(),
        extra_proof_files: dependency_proofs,
        ..Default::default()
    });
    let (pool, _callsites) = runner.run_load_and_enumerate();
    assert!(
        pool.mementos.get(&target_cid).is_some(),
        "vendor contract {target_cid} must be present after dependency proof assembly"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn voltron_pool_refuses_cross_dependency_violation() {
    if !z3_available() {
        eprintln!("SKIP voltron_pool_refuses_cross_dependency_violation: z3 not on PATH");
        return;
    }

    let root = unique_dir("e2e");
    let project = root.join("user");
    let vendor = root.join("vendor");
    fs::create_dir_all(project.join(".provekit")).expect("mkdir project");
    let (target_cid, bundle_cid, proof_path) = publish_vendor_positive_contract(&vendor);
    publish_user_bridge(&project, &target_cid, &bundle_cid);
    install_dependency_proof_stub(&project, &proof_path);

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("prove")
        .arg(&project)
        .arg("--z3")
        .arg("z3")
        .arg("--quiet")
        .output()
        .expect("run provekit prove");
    assert_eq!(
        output.status.code(),
        Some(1),
        "cross-dependency bridge to must_be_positive(-1) must be refused\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = fs::remove_dir_all(root);
}
