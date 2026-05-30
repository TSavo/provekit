// SPDX-License-Identifier: Apache-2.0

use std::fmt::Write as _;
use std::io::Cursor;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use flate2::read::GzDecoder;
use provekit_canonicalizer::Value as CValue;
use provekit_claim_envelope::{
    compute_contract_set_cid, contract_cid, Authoring, MintContractArgs,
};
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--self-test") {
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        match lift_project(&root) {
            Ok(value) => {
                println!("{}", serde_json::to_string_pretty(&value).unwrap());
                std::process::exit(0);
            }
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    }
    if !args.iter().any(|arg| arg == "--rpc") {
        eprintln!("Usage: supply-chain-npm-lifter --rpc | --self-test");
        std::process::exit(1);
    }
    run_rpc();
}

fn run_rpc() {
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(error) => {
                println!(
                    "{}",
                    json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":error.to_string()}})
                );
                continue;
            }
        };
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        match method {
            "initialize" => respond(
                id,
                json!({
                    "name": "supply-chain-npm-lifter",
                    "version": "0.1.0",
                    "protocol_version": "pep/1.7.0",
                    "capabilities": {
                        "authoring_surfaces": ["supply-chain-npm"],
                        "ir_version": "v1.1.0",
                        "emits_signed_mementos": false,
                        "identify_result_kinds": ["package-inspection-document"]
                    }
                }),
            ),
            "lift" => {
                let workspace = request
                    .pointer("/params/workspace_root")
                    .and_then(Value::as_str)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("."));
                let identify_only = request
                    .pointer("/params/options/identifyOnly")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                    || request
                        .pointer("/params/options/layer")
                        .and_then(Value::as_str)
                        == Some("identify-only");
                let result = if identify_only {
                    identify_project(&workspace)
                } else {
                    lift_project(&workspace)
                };
                match result {
                    Ok(result) => respond(id, result),
                    Err(error) => respond_error(id, 1005, &error),
                }
            }
            "shutdown" => {
                respond(id, Value::Null);
                break;
            }
            _ => respond_error(id, -32601, "unknown method"),
        }
    }
}

fn respond(id: Value, result: Value) {
    println!("{}", json!({"jsonrpc":"2.0","id":id,"result":result}));
    let _ = io::stdout().flush();
}

fn respond_error(id: Value, code: i64, message: &str) {
    println!(
        "{}",
        json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
    );
    let _ = io::stdout().flush();
}

fn identify_project(project: &Path) -> Result<Value, String> {
    let package = read_json(project, "package.json")?;
    let release_metadata = read_optional_json(project, "contracts.json")?.unwrap_or(Value::Null);
    let lifted = lift_typescript_contracts(project)?;
    let package_name = str_field(&package, "name")?;
    let package_version = str_field(&package, "version")?;
    let contract_set_cid = compute_contract_set_cid_for_ir(&lifted.ir)?;
    let previous_contract_set_cid = release_metadata
        .get("previousContractSetCid")
        .and_then(Value::as_str)
        .map(str::to_string);
    let contract_names = lifted.contract_names;

    let tarball_path = project.join("package.tgz");
    if !tarball_path.is_file() {
        return Err(format!(
            "missing npm package artifact {}",
            tarball_path.display()
        ));
    }
    let artifact_path = "package.tgz";
    let artifact_bytes = std::fs::read(&tarball_path)
        .map_err(|e| format!("read {}: {e}", tarball_path.display()))?;
    let artifact_cid = blake3_512(&artifact_bytes);
    let artifact = artifact_report(artifact_path, &artifact_bytes)?;
    let (input_closure_cid, closure) = package_input_closure_cid(project)?;
    let lockfile_cids = lockfile_cids(project)?;
    let proof_discovery = proof_discovery(project, &package)?;
    let proof_cids = proof_discovery
        .get("files")
        .and_then(Value::as_array)
        .map(|files| {
            files
                .iter()
                .filter_map(|file| file.get("contentCid").and_then(Value::as_str))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let npm = npm_semantics(&package);
    let blast_radius = npm_blast_radius(
        package_name,
        package_version,
        &input_closure_cid,
        &contract_set_cid,
        &lockfile_cids,
        &proof_cids,
    )?;
    let blast_radius_cid = json_value_cid(&blast_radius)?;
    let conventional_receipts = conventional_receipts(
        project,
        package_name,
        package_version,
        &artifact_cid,
        &artifact_bytes,
    )?;

    Ok(json!({
        "kind": "package-inspection-document",
        "ecosystem": "npm",
        "package": {
            "name": package_name,
            "version": package_version
        },
        "artifact": artifact,
        "ci": {
            "inputClosureCid": input_closure_cid,
            "closure": closure,
            "lockfileCids": lockfile_cids,
            "blastRadiusCid": blast_radius_cid,
            "blastRadius": blast_radius
        },
        "proofs": proof_discovery,
        "npm": npm,
        "release": {
            "contractSetCid": contract_set_cid,
            "previousContractSetCid": previous_contract_set_cid,
            "contracts": contract_names,
            "contractSource": lifted.source
        },
        "conventionalReceipts": conventional_receipts,
        "admission": {
            "status": "not-decided",
            "reason": "package identity, provenance, and tarball hash are not contract admission"
        },
        "diagnostics": []
    }))
}

fn lift_project(project: &Path) -> Result<Value, String> {
    let package = read_json(project, "package.json")?;
    let release_metadata = read_optional_json(project, "contracts.json")?.unwrap_or(Value::Null);
    let lifted = lift_typescript_contracts(project)?;
    validate_no_install_hook(&package)?;
    let package_name = str_field(&package, "name")?;
    let package_version = str_field(&package, "version")?;
    let contract_set_cid = compute_contract_set_cid_for_ir(&lifted.ir)?;
    let previous_contract_set_cid = release_metadata
        .get("previousContractSetCid")
        .and_then(Value::as_str)
        .map(str::to_string);
    let contract_names = lifted.contract_names;

    Ok(json!({
        "kind": "ir-document",
        "surface": "supply-chain-npm",
        "package": {
            "ecosystem": "npm",
            "name": package_name,
            "version": package_version
        },
        "release": {
            "contracts": contract_names,
            "contractSetCid": contract_set_cid,
            "previousContractSetCid": previous_contract_set_cid,
            "contractSource": lifted.source
        },
        "ir": lifted.ir,
        "authorities": authority_decls(&contract_names),
        "witnesses": witness_requirements(&contract_names),
        "implications": [],
        "diagnostics": []
    }))
}

fn authority_decls(contracts: &[String]) -> Vec<Value> {
    let mut authorities = vec![
        json!({
            "id": "safe-json.maintainer",
            "principal": "npm:maintainer/safe-json-team",
            "scopeKind": "proof",
            "scope": "safe-json npm release admission"
        }),
        json!({
            "id": "safe-json.package.identity",
            "principal": "npm:registry/package-identity",
            "scopeKind": "contract",
            "scope": "safe-json package identity",
            "parent": "safe-json.maintainer"
        }),
    ];
    authorities.extend(contracts.iter().map(|name| {
        json!({
            "id": format!("safe-json.contract.{name}"),
            "principal": format!("npm:maintainer/safe-json-team/{name}"),
            "scopeKind": "contract",
            "scope": name,
            "parent": "safe-json.maintainer"
        })
    }));
    authorities
}

fn witness_requirements(contracts: &[String]) -> Vec<Value> {
    contracts
        .iter()
        .map(|name| {
            let (surface, artifact) = if name == "package.no-install-side-effect" {
                ("package-manifest", "package.json")
            } else {
                ("javascript", "index.js")
            };
            json!({
                "id": format!("safe-json-{name}-witness"),
                "mode": "witness",
                "surface": surface,
                "attachTo": name,
                "obligation": {
                    "kind": "predicate",
                    "name": name,
                    "proofIr": {"kind": "atomic", "name": name, "args": []}
                },
                "host": {
                    "kit": surface,
                    "contextKind": "npm-package",
                    "artifact": artifact,
                    "entrypoint": "parseJson"
                },
                "bindings": [],
                "policy": {
                    "policyCid": "builtin:supply-chain-rails/npm-safe-json@0.1",
                    "mode": "static-plus-contract-witness"
                }
            })
        })
        .collect()
}

fn artifact_report(path: &str, bytes: &[u8]) -> Result<Value, String> {
    let gzip = bytes.starts_with(&[0x1f, 0x8b]);
    let mut artifact = json!({
        "path": path,
        "binaryCid": blake3_512(bytes),
        "bytes": bytes.len(),
        "gzip": gzip
    });
    if path == "package.tgz" {
        artifact["format"] = json!("npm-pack-tarball");
        artifact["entries"] = json!(tar_entries(bytes)?);
    } else {
        artifact["format"] = json!("npm-package-manifest");
        artifact["entries"] = json!([]);
    }
    Ok(artifact)
}

fn tar_entries(bytes: &[u8]) -> Result<Vec<String>, String> {
    let decoder = GzDecoder::new(Cursor::new(bytes));
    let mut archive = tar::Archive::new(decoder);
    let entries = archive
        .entries()
        .map_err(|e| format!("read npm package tar entries: {e}"))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("read npm package tar entry: {e}"))?;
        let path = entry
            .path()
            .map_err(|e| format!("read npm package tar entry path: {e}"))?;
        names.push(path.to_string_lossy().to_string());
    }
    names.sort();
    Ok(names)
}

fn conventional_receipts(
    project: &Path,
    package_name: &str,
    package_version: &str,
    artifact_cid: &str,
    artifact_bytes: &[u8],
) -> Result<Value, String> {
    let artifact_sha256 = sha256_hex(artifact_bytes);
    let slsa = verify_slsa_vsa(project, package_name, package_version, artifact_cid)?;
    let in_toto = verify_in_toto_pipeline(project, &artifact_sha256)?;
    Ok(json!({
        "packageIdentity": {
            "verdict": "green",
            "ecosystem": "npm",
            "name": package_name,
            "version": package_version
        },
        "maintainerSignature": {
            "verdict": "green",
            "source": "in-toto functionary key and SLSA VSA verifier key",
            "doesNotDischarge": "preserved behavioral contracts"
        },
        "registryMetadata": {
            "verdict": "green",
            "registry": "fixture:npm",
            "doesNotDischarge": "runtime behavior"
        },
        "tarballHash": {
            "verdict": "green",
            "algorithm": "blake3-512",
            "digest": artifact_cid,
            "sha256": artifact_sha256
        },
        "slsaVerificationSummary": slsa,
        "inTotoPipeline": in_toto,
        "sbomContents": {
            "verdict": "green",
            "source": "npm pack entries",
            "contents": [
                "package/package.json",
                "package/index.js",
                "package/contracts.ts",
                "package/contracts.json"
            ],
            "doesNotDischarge": "preserved behavioral contract witnesses"
        }
    }))
}

fn verify_slsa_vsa(
    project: &Path,
    package_name: &str,
    package_version: &str,
    artifact_cid: &str,
) -> Result<Value, String> {
    let digest = artifact_cid
        .strip_prefix("blake3-512:")
        .ok_or_else(|| format!("artifact CID is not blake3-512: {artifact_cid}"))?;
    let vsa_rel = "attestations/slsa/vsa.jsonl";
    let pub_rel = "attestations/slsa/vsa.pub";
    let vsa_path = project.join(vsa_rel);
    let pub_path = project.join(pub_rel);
    let verifier_id = "https://provekit.dev/menagerie/supply-chain-rails/slsa-vsa-verifier/v0.1";
    let resource_uri = format!("pkg:npm/{package_name}@{package_version}");
    let slsa_verifier = find_executable("slsa-verifier", &["$HOME/go/bin/slsa-verifier"])?;
    let args = vec![
        "verify-vsa".to_string(),
        "--subject-digest".to_string(),
        format!("blake3-512:{digest}"),
        "--attestation-path".to_string(),
        vsa_path.display().to_string(),
        "--verifier-id".to_string(),
        verifier_id.to_string(),
        "--resource-uri".to_string(),
        resource_uri.clone(),
        "--verified-level".to_string(),
        "SLSA_BUILD_LEVEL_2".to_string(),
        "--public-key-path".to_string(),
        pub_path.display().to_string(),
        "--print-attestation".to_string(),
    ];
    let output = Command::new(&slsa_verifier)
        .args(&args)
        .env("PATH", augmented_path())
        .output()
        .map_err(|e| format!("spawn slsa-verifier: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "slsa-verifier failed\ncommand: {}\nstdout:\n{}\nstderr:\n{}",
            command_display(&slsa_verifier, &args),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let attestation: Value = serde_json::from_slice(&output.stdout).map_err(|e| {
        format!(
            "parse slsa-verifier --print-attestation output: {e}\nstdout:\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    })?;
    let statement_cid = json_value_cid(&attestation)?;
    Ok(json!({
        "verdict": "green",
        "tool": "slsa-verifier",
        "command": command_vector("slsa-verifier", &args),
        "attestationPath": vsa_rel,
        "publicKeyPath": pub_rel,
        "predicateType": attestation.get("predicateType").cloned().unwrap_or(Value::Null),
        "statementCid": statement_cid,
        "subjectDigest": format!("blake3-512:{digest}"),
        "resourceUri": resource_uri,
        "verifiedLevels": attestation.pointer("/predicate/verifiedLevels").cloned().unwrap_or_else(|| json!([])),
        "doesNotDischarge": "runtime.no-env-secret-read"
    }))
}

fn verify_in_toto_pipeline(project: &Path, artifact_sha256: &str) -> Result<Value, String> {
    let dir_rel = "attestations/in-toto";
    let layout_rel = "attestations/in-toto/root.layout";
    let pub_rel = "attestations/in-toto/layout.pub";
    let dir = project.join(dir_rel);
    let layout = project.join(layout_rel);
    let pubkey = project.join(pub_rel);
    let in_toto_verify = find_executable("in-toto-verify", &["$HOME/.local/bin/in-toto-verify"])?;
    let args = vec![
        "--layout".to_string(),
        layout.display().to_string(),
        "--verification-keys".to_string(),
        pubkey.display().to_string(),
        "--link-dir".to_string(),
        dir.display().to_string(),
    ];
    let output = Command::new(&in_toto_verify)
        .args(&args)
        .env("PATH", augmented_path())
        .output()
        .map_err(|e| format!("spawn in-toto-verify: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "in-toto-verify failed\ncommand: {}\nstdout:\n{}\nstderr:\n{}",
            command_display(&in_toto_verify, &args),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let link_rel = find_in_toto_link(project, dir_rel)?;
    let link = read_json(project, &link_rel)?;
    let product_sha256 = link
        .pointer("/signed/products/package.tgz/sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{link_rel} missing /signed/products/package.tgz/sha256"))?;
    let product_digest_matches_artifact = product_sha256 == artifact_sha256;
    if !product_digest_matches_artifact {
        return Err(format!(
            "in-toto product digest mismatch: link has {product_sha256}, package.tgz has {artifact_sha256}"
        ));
    }
    Ok(json!({
        "verdict": "green",
        "tool": "in-toto-verify",
        "command": command_vector("in-toto-verify", &args),
        "layoutPath": layout_rel,
        "verificationKeyPath": pub_rel,
        "linkPath": link_rel,
        "step": "safe-json-pack",
        "product": "package.tgz",
        "productSha256": product_sha256,
        "productDigestMatchesArtifact": product_digest_matches_artifact,
        "doesNotDischarge": "runtime.no-env-secret-read"
    }))
}

fn find_in_toto_link(project: &Path, dir_rel: &str) -> Result<String, String> {
    let dir = project.join(dir_rel);
    let mut links = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| format!("read {}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("read in-toto dir entry: {e}"))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.starts_with("safe-json-pack.") && name.ends_with(".link") {
            links.push(format!("{dir_rel}/{name}"));
        }
    }
    links.sort();
    match links.len() {
        1 => Ok(links.remove(0)),
        0 => Err(format!("no safe-json-pack link found in {}", dir.display())),
        _ => Err(format!(
            "multiple safe-json-pack links found in {}",
            dir.display()
        )),
    }
}

fn find_executable(name: &str, fallbacks: &[&str]) -> Result<String, String> {
    if let Some(path) = find_on_path(name) {
        return Ok(path.display().to_string());
    }
    for fallback in fallbacks {
        let expanded = expand_home(fallback);
        if expanded.exists() {
            return Ok(expanded.display().to_string());
        }
    }
    Err(format!(
        "required external receipt tool `{name}` not found on PATH; run menagerie/supply-chain-rails/authenticated-betrayal/tools/refresh-native-receipts.mjs after installing the native tools"
    ))
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("$HOME/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

fn augmented_path() -> String {
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(&home).join("go/bin"));
        dirs.push(PathBuf::from(home).join(".local/bin"));
    }
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    std::env::join_paths(dirs)
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

fn command_vector(display_name: &str, args: &[String]) -> Vec<String> {
    let mut out = vec![display_name.to_string()];
    out.extend(args.iter().cloned());
    out
}

fn command_display(program: &str, args: &[String]) -> String {
    let mut parts = vec![program.to_string()];
    parts.extend(args.iter().cloned());
    parts.join(" ")
}

fn read_json(project: &Path, rel: &str) -> Result<Value, String> {
    let path = project.join(rel);
    let text =
        std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))
}

fn read_optional_json(project: &Path, rel: &str) -> Result<Option<Value>, String> {
    let path = project.join(rel);
    if !path.exists() {
        return Ok(None);
    }
    let text =
        std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| format!("parse {}: {e}", path.display()))
}

fn str_field<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing string field `{field}`"))
}

struct LiftedContracts {
    contract_names: Vec<String>,
    ir: Vec<Value>,
    source: Value,
}

fn lift_typescript_contracts(project: &Path) -> Result<LiftedContracts, String> {
    let report = run_typescript_contract_lifter(project)?;
    let contracts = report
        .get("contracts")
        .and_then(Value::as_array)
        .ok_or_else(|| "TypeScript contract report missing contracts array".to_string())?;
    if contracts.is_empty() {
        return Err(format!(
            "TypeScript lifter found no contracts in {}",
            project.display()
        ));
    }

    let mut contract_names = Vec::new();
    let mut ir = Vec::new();
    for contract in contracts {
        let name = contract
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "lifted TypeScript contract missing name".to_string())?
            .to_string();
        let out_binding = contract
            .get("outBinding")
            .and_then(Value::as_str)
            .unwrap_or("out")
            .to_string();
        let mut decl = serde_json::Map::new();
        decl.insert("kind".to_string(), Value::String("contract".to_string()));
        decl.insert("name".to_string(), Value::String(name.clone()));
        decl.insert(
            "authority".to_string(),
            Value::String(format!("safe-json.contract.{name}")),
        );
        decl.insert("outBinding".to_string(), Value::String(out_binding));
        for slot in ["pre", "post", "inv"] {
            if let Some(value) = contract.get(slot) {
                decl.insert(slot.to_string(), value.clone());
            }
        }
        if !decl.contains_key("pre") && !decl.contains_key("post") && !decl.contains_key("inv") {
            return Err(format!(
                "lifted TypeScript contract `{name}` has no pre/post/inv formula"
            ));
        }
        decl.insert(
            "source".to_string(),
            json!({
                "language": "typescript",
                "lifter": report.get("lifter").and_then(Value::as_str).unwrap_or("provekit-lift-ts"),
                "adapter": contract.get("adapter").and_then(Value::as_str).unwrap_or("unknown"),
                "sourcePath": contract.get("sourcePath").and_then(Value::as_str).unwrap_or(""),
                "sourceName": contract.get("sourceName").and_then(Value::as_str).unwrap_or("")
            }),
        );
        contract_names.push(name);
        ir.push(Value::Object(decl));
    }
    contract_names.sort();
    contract_names.dedup();
    ir.sort_by(|a, b| {
        a.get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .cmp(b.get("name").and_then(Value::as_str).unwrap_or(""))
    });
    Ok(LiftedContracts {
        contract_names,
        ir,
        source: json!({
            "language": "typescript",
            "lifter": report.get("lifter").and_then(Value::as_str).unwrap_or("provekit-lift-ts"),
            "filesScanned": report.get("filesScanned").and_then(Value::as_u64).unwrap_or(0),
            "adapters": report.get("adapterReports").cloned().unwrap_or_else(|| json!([]))
        }),
    })
}

fn run_typescript_contract_lifter(project: &Path) -> Result<Value, String> {
    let kit_rpc_dir = std::env::var_os("PROVEKIT_SUPPLY_CHAIN_KIT_RPC_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from("menagerie/supply-chain-rails/authenticated-betrayal/kit-rpc")
        });
    let repo_root = std::env::var_os("PROVEKIT_SUPPLY_CHAIN_REPO_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| find_repo_root().unwrap_or_else(|| PathBuf::from(".")));
    let script = kit_rpc_dir.join("supply-chain-ts-contracts.ts");
    let output = Command::new("pnpm")
        .args(["exec", "tsx"])
        .arg(&script)
        .arg(project)
        .current_dir(&repo_root)
        .output()
        .map_err(|e| format!("spawn TypeScript contract lifter: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "TypeScript contract lifter failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|e| {
        format!(
            "parse TypeScript contract report: {e}\nstdout:\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn find_repo_root() -> Option<PathBuf> {
    let mut cur = std::env::current_dir().ok()?;
    loop {
        if cur.join("package.json").exists() && cur.join("implementations").exists() {
            return Some(cur);
        }
        if !cur.pop() {
            return None;
        }
    }
}

fn compute_contract_set_cid_for_ir(ir: &[Value]) -> Result<String, String> {
    let mut contract_cids = Vec::new();
    for decl in ir {
        if decl.get("kind").and_then(Value::as_str) != Some("contract") {
            continue;
        }
        let name = decl
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "contract declaration missing name".to_string())?;
        let pre = decl.get("pre").map(json_to_cvalue);
        let post = decl.get("post").map(json_to_cvalue);
        let inv = decl.get("inv").map(json_to_cvalue);
        if pre.is_none() && post.is_none() && inv.is_none() {
            continue;
        }
        let args = MintContractArgs {
            contract_name: name.to_string(),
            library: None,
            pre,
            post,
            inv,
            out_binding: decl
                .get("outBinding")
                .and_then(Value::as_str)
                .unwrap_or("out")
                .to_string(),
            produced_by: "provekit-cli".to_string(),
            produced_at: "2026-05-03T18:00:00Z".to_string(),
            input_cids: Vec::new(),
            authoring: Authoring::Lift {
                lifter: "ir-document".to_string(),
                evidence: "minted from ir-document RPC response".to_string(),
                source_cid: None,
            },
            signer_seed: [0x42u8; 32],
            formals: Vec::new(),
            formal_sorts: Vec::new(),
            emit_empty_formals: false,
        };
        contract_cids.push(contract_cid(&args));
    }
    Ok(compute_contract_set_cid(contract_cids))
}

fn json_to_cvalue(j: &Value) -> Arc<CValue> {
    match j {
        Value::Null => CValue::null(),
        Value::Bool(b) => CValue::boolean(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                CValue::integer(i)
            } else if let Some(u) = n.as_u64() {
                CValue::integer(u as i64)
            } else if let Some(f) = n.as_f64() {
                CValue::integer(f as i64)
            } else {
                CValue::integer(0)
            }
        }
        Value::String(s) => CValue::string(s.clone()),
        Value::Array(items) => {
            let values = items.iter().map(json_to_cvalue).collect::<Vec<_>>();
            CValue::array(values)
        }
        Value::Object(map) => {
            let entries = map
                .iter()
                .map(|(key, value)| (key.clone(), json_to_cvalue(value)))
                .collect::<Vec<_>>();
            CValue::object(entries)
        }
    }
}

fn package_input_closure_cid(project: &Path) -> Result<(String, Vec<String>), String> {
    let mut bytes = Vec::new();
    let mut closure = Vec::new();
    let mut rels = vec![
        "package.json".to_string(),
        "index.js".to_string(),
        "contracts.ts".to_string(),
        "contracts.json".to_string(),
        "package.tgz".to_string(),
        "package-lock.json".to_string(),
        "npm-shrinkwrap.json".to_string(),
    ];
    rels.extend(proof_file_rels(project)?);
    rels.extend(attestation_file_rels(project)?);
    rels.sort();
    rels.dedup();
    for rel in rels {
        let path = project.join(&rel);
        if !path.exists() {
            continue;
        }
        closure.push(rel.clone());
        append_len_prefixed(&mut bytes, rel.as_bytes());
        let file_bytes =
            std::fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        append_len_prefixed(&mut bytes, &file_bytes);
    }
    Ok((blake3_512(&bytes), closure))
}

fn append_len_prefixed(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    out.extend_from_slice(bytes);
}

fn attestation_file_rels(project: &Path) -> Result<Vec<String>, String> {
    let root = project.join("attestations");
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut rels = Vec::new();
    collect_attestation_file_rels(project, &root, &mut rels)?;
    rels.sort();
    Ok(rels)
}

fn collect_attestation_file_rels(
    project: &Path,
    dir: &Path,
    rels: &mut Vec<String>,
) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("read attestation dir entry: {e}"))?;
        let path = entry.path();
        if path.is_dir() {
            collect_attestation_file_rels(project, &path, rels)?;
            continue;
        }
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) == Some("key") {
            continue;
        }
        let rel = path
            .strip_prefix(project)
            .map_err(|e| format!("relativize {}: {e}", path.display()))?
            .to_string_lossy()
            .replace('\\', "/");
        rels.push(rel);
    }
    Ok(())
}

fn npm_semantics(package: &Value) -> Value {
    let main = package
        .get("main")
        .and_then(Value::as_str)
        .unwrap_or("index.js");
    let scripts = package
        .get("scripts")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let lifecycle = ["preinstall", "install", "postinstall", "prepare"]
        .into_iter()
        .map(|hook| {
            let command = scripts.get(hook).and_then(Value::as_str).unwrap_or("");
            json!({
                "hook": hook,
                "present": !command.is_empty(),
                "command": command
            })
        })
        .collect::<Vec<_>>();
    let has_install_side_effects = lifecycle
        .iter()
        .any(|hook| hook.get("present").and_then(Value::as_bool) == Some(true));
    let published_files = package
        .get("files")
        .and_then(Value::as_array)
        .map(|files| files.iter().filter_map(Value::as_str).collect::<Vec<_>>())
        .unwrap_or_default();
    let proof_included_by_files = published_files
        .iter()
        .any(|entry| *entry == "*.proof" || entry.ends_with(".proof"));
    json!({
        "entrypoint": main,
        "lifecycleScripts": lifecycle,
        "hasInstallSideEffects": has_install_side_effects,
        "publishedFiles": published_files,
        "proofIncludedByFiles": proof_included_by_files,
        "dependencyNames": object_keys(package.get("dependencies")),
        "devDependencyNames": object_keys(package.get("devDependencies")),
        "semanticReads": [
            "package.name",
            "package.version",
            "package.main",
            "package.files",
            "package.scripts.{preinstall,install,postinstall,prepare}",
            "package.provekit.proofHash"
        ]
    })
}

fn object_keys(value: Option<&Value>) -> Vec<String> {
    let mut keys = value
        .and_then(Value::as_object)
        .map(|object| object.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    keys.sort();
    keys
}

fn proof_discovery(project: &Path, package: &Value) -> Result<Value, String> {
    let manifest_hint = package
        .pointer("/provekit/proofHash")
        .or_else(|| package.pointer("/provekit/proofCid"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let files = proof_file_rels(project)?
        .into_iter()
        .map(|rel| {
            let path = project.join(&rel);
            let bytes =
                std::fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
            let content_cid = blake3_512(&bytes);
            let filename_cid = rel.strip_suffix(".proof").unwrap_or(&rel).to_string();
            Ok(json!({
                "path": rel,
                "filenameCid": filename_cid,
                "contentCid": content_cid,
                "bytes": bytes.len(),
                "filenameMatchesContent": filename_cid == content_cid
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let proof_file_cids = files
        .iter()
        .filter_map(|file| file.get("contentCid").and_then(Value::as_str))
        .map(str::to_string)
        .collect::<Vec<_>>();
    let hint_matches_shipped_file = !manifest_hint.is_empty()
        && files.iter().any(|file| {
            file.get("filenameCid").and_then(Value::as_str) == Some(manifest_hint)
                || file.get("contentCid").and_then(Value::as_str) == Some(manifest_hint)
        });
    let status = if !files.is_empty() {
        "discovered"
    } else if !manifest_hint.is_empty() {
        "hint-only"
    } else {
        "absent"
    };
    Ok(json!({
        "status": status,
        "manifestHint": if manifest_hint.is_empty() { Value::Null } else { Value::String(manifest_hint.to_string()) },
        "proofFileCids": proof_file_cids,
        "files": files,
        "hintMatchesShippedFile": hint_matches_shipped_file
    }))
}

fn proof_file_rels(project: &Path) -> Result<Vec<String>, String> {
    let mut rels = Vec::new();
    for entry in
        std::fs::read_dir(project).map_err(|e| format!("read {}: {e}", project.display()))?
    {
        let entry = entry.map_err(|e| format!("read dir entry: {e}"))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("proof") {
            if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                rels.push(name.to_string());
            }
        }
    }
    rels.sort();
    Ok(rels)
}

fn lockfile_cids(project: &Path) -> Result<Vec<String>, String> {
    let mut cids = Vec::new();
    for rel in ["package-lock.json", "npm-shrinkwrap.json"] {
        let path = project.join(rel);
        if path.exists() {
            let bytes =
                std::fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
            cids.push(blake3_512(&bytes));
        }
    }
    cids.sort();
    Ok(cids)
}

fn npm_blast_radius(
    package_name: &str,
    package_version: &str,
    input_closure_cid: &str,
    contract_set_cid: &str,
    lockfile_cids: &[String],
    proof_cids: &[String],
) -> Result<Value, String> {
    let policy_cid = json_value_cid(&json!({
        "kind": "NpmPackageInspectionPolicy",
        "schemaVersion": "1",
        "requiresExactInputClosure": true,
        "requiresProofDiscovery": false,
        "requiresLifecycleScriptInspection": true
    }))?;
    let mut input_cids = vec![input_closure_cid.to_string(), contract_set_cid.to_string()];
    input_cids.extend(lockfile_cids.iter().cloned());
    input_cids.extend(proof_cids.iter().cloned());
    input_cids.sort();
    input_cids.dedup();
    Ok(json!({
        "kind": "NpmPackageBlastRadius",
        "schemaVersion": "1",
        "jobKey": format!("npm/package-inspect/{package_name}"),
        "subjectKind": "npm-package",
        "subject": format!("{package_name}@{package_version}"),
        "sourceClosureCid": input_closure_cid,
        "lockfileCids": lockfile_cids,
        "proofCids": proof_cids,
        "contractSetCid": contract_set_cid,
        "policyCid": policy_cid,
        "nondeterminism": {
            "network": "forbidden",
            "clock": "forbidden",
            "secrets": "forbidden",
            "randomness": "forbidden"
        },
        "inputCids": input_cids
    }))
}

fn json_value_cid(value: &Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|e| format!("serialize JSON for CID: {e}"))?;
    Ok(blake3_512(&bytes))
}

fn blake3_512(bytes: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(bytes);
    let mut output = [0u8; 64];
    hasher.finalize_xof().fill(&mut output);
    let mut hex = String::with_capacity(128);
    for byte in output {
        let _ = write!(&mut hex, "{byte:02x}");
    }
    format!("blake3-512:{hex}")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

fn validate_no_install_hook(package: &Value) -> Result<(), String> {
    let Some(scripts) = package.get("scripts").and_then(Value::as_object) else {
        return Ok(());
    };
    for hook in ["preinstall", "install", "postinstall", "prepare"] {
        if scripts.contains_key(hook) {
            return Err(format!(
                "package.no-install-side-effect refused: package.json contains `{hook}` script"
            ));
        }
    }
    Ok(())
}
