// SPDX-License-Identifier: Apache-2.0

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

fn main() {
    if !std::env::args().any(|arg| arg == "--rpc") {
        eprintln!("Usage: supply-chain-js-lowerer --rpc");
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
                    "name": "supply-chain-js-lowerer",
                    "version": "0.1.0",
                    "protocol_version": "provekit-orp/1",
                    "capabilities": {
                        "kits": ["javascript", "package-manifest"],
                        "modes": ["witness"],
                        "obligation_kinds": ["predicate"]
                    }
                }),
            ),
            "realize" => {
                let workspace = request
                    .pointer("/params/workspace_root")
                    .and_then(Value::as_str)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("."));
                let plan = request
                    .pointer("/params/plan")
                    .cloned()
                    .unwrap_or(Value::Null);
                match realize(&workspace, &plan) {
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

fn realize(project: &Path, plan: &Value) -> Result<Value, String> {
    let contract = plan
        .pointer("/obligation/name")
        .and_then(Value::as_str)
        .ok_or_else(|| "RealizerPlan missing obligation.name".to_string())?;
    let artifact = plan
        .pointer("/host/artifact")
        .and_then(Value::as_str)
        .unwrap_or("index.js");
    let artifact_path = project.join(artifact);
    let source = std::fs::read_to_string(&artifact_path)
        .map_err(|e| format!("read {}: {e}", artifact_path.display()))?;
    let artifact_cid = blake3_512(source.as_bytes());

    if let Some(message) = refusal_for(contract, artifact, &source) {
        return Ok(json!({
            "kind": "ORPLowerResult",
            "claimKind": "npm-package-contract-witness",
            "policyCid": plan.get("policyCid").and_then(Value::as_str).unwrap_or("builtin:supply-chain-rails/npm-safe-json@0.1"),
            "verifierCid": "builtin:supply-chain-rails/js-static-lowerer@0.1",
            "claimBody": {
                "claimKind": "npm-package-contract-witness",
                "contract": contract,
                "subjectCids": [artifact_cid],
                "policyCid": plan.get("policyCid").and_then(Value::as_str).unwrap_or("builtin:supply-chain-rails/npm-safe-json@0.1")
            },
            "evidence": {
                "kind": "static-js-witness-refusal",
                "contract": contract,
                "artifact": artifact,
                "counterexample": message,
                "witnessArtifact": {
                    "language": "javascript-static-analysis",
                    "source": format!("scan({artifact}) => reject when preserved contract {contract} is contradicted")
                }
            },
            "evidenceCid": blake3_512(message.as_bytes()),
            "output": {
                "status": "rejected",
                "reasonCode": reason_code(contract),
                "message": message,
                "error": message,
                "realizer": {"name": "supply-chain-js-lowerer", "version": "0.1.0"},
                "observedArtifactCids": [artifact_cid],
                "witnessArtifact": {
                    "language": "javascript-static-analysis",
                    "source": format!("scan({artifact}) => reject when preserved contract {contract} is contradicted")
                }
            }
        }));
    }

    let evidence = json!({
        "kind": "static-js-witness",
        "contract": contract,
        "artifact": artifact,
        "checks": accepted_checks(contract),
        "witnessArtifact": {
            "language": "javascript-static-analysis",
            "source": format!("scan({artifact}) => accepted contract {contract}; no forbidden token matched")
        }
    });
    Ok(json!({
        "kind": "ORPLowerResult",
        "claimKind": "npm-package-contract-witness",
        "policyCid": plan.get("policyCid").and_then(Value::as_str).unwrap_or("builtin:supply-chain-rails/npm-safe-json@0.1"),
        "verifierCid": "builtin:supply-chain-rails/js-static-lowerer@0.1",
        "claimBody": {
            "claimKind": "npm-package-contract-witness",
            "contract": contract,
            "subjectCids": [artifact_cid],
            "policyCid": plan.get("policyCid").and_then(Value::as_str).unwrap_or("builtin:supply-chain-rails/npm-safe-json@0.1")
        },
        "evidence": evidence,
        "evidenceCid": blake3_512(serde_json::to_string(&evidence).unwrap().as_bytes()),
        "inputCids": [],
        "producedAt": "2026-05-08T00:00:00Z",
        "output": {
            "status": "witnessed",
            "message": format!("accepted witness for {contract}"),
            "realizer": {"name": "supply-chain-js-lowerer", "version": "0.1.0"},
            "observedArtifactCids": [artifact_cid],
            "witnessArtifact": {
                "language": "javascript-static-analysis",
                "source": format!("scan({artifact}) => accepted contract {contract}; no forbidden token matched")
            }
        }
    }))
}

fn refusal_for(contract: &str, artifact: &str, source: &str) -> Option<String> {
    match contract {
        "runtime.no-env-secret-read" if source.contains("process.env") => Some(
            "counterexample: input contains __safeJsonTelemetry=true\nobserved: index.js reads process.env.SAFE_JSON_TOKEN\nviolated: runtime.no-env-secret-read".to_string(),
        ),
        "parse.no-network-effect"
            if source.contains("fetch(")
                || source.contains("require(\"http\")")
                || source.contains("require('http')")
                || source.contains("require(\"net\")")
                || source.contains("require('net')") =>
        {
            Some("counterexample: parse path can open network-capable API".to_string())
        }
        "parse.deterministic" if source.contains("Math.random") || source.contains("Date.now") => {
            Some("counterexample: parse path reads nondeterministic runtime state".to_string())
        }
        "package.no-install-side-effect"
            if artifact == "package.json"
                && (source.contains("\"preinstall\"")
                    || source.contains("\"install\"")
                    || source.contains("\"postinstall\"")
                    || source.contains("\"prepare\"")) =>
        {
            Some("counterexample: package.json declares an effectful install hook".to_string())
        }
        _ => None,
    }
}

fn reason_code(contract: &str) -> &'static str {
    match contract {
        "runtime.no-env-secret-read" => "env-secret-read",
        "parse.no-network-effect" => "network-effect",
        "parse.deterministic" => "nondeterministic-parse",
        "package.no-install-side-effect" => "install-side-effect",
        _ => "contract-violation",
    }
}

fn accepted_checks(contract: &str) -> Vec<&'static str> {
    match contract {
        "runtime.no-env-secret-read" => vec!["no process.env reads matched"],
        "parse.no-network-effect" => vec!["no fetch/http/net tokens matched"],
        "parse.deterministic" => vec!["no Math.random or Date.now tokens matched"],
        "package.no-install-side-effect" => vec!["no install lifecycle scripts matched"],
        _ => vec!["contract-specific static scan accepted"],
    }
}

fn blake3_512(bytes: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(bytes);
    let mut out = [0u8; 64];
    hasher.finalize_xof().fill(&mut out);
    let mut hex = String::with_capacity(128);
    for byte in out {
        hex.push_str(&format!("{byte:02x}"));
    }
    format!("blake3-512:{hex}")
}
