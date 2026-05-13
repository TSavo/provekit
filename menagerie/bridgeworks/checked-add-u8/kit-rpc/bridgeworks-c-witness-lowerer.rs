// SPDX-License-Identifier: Apache-2.0

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

const OBLIGATION: &str = "checked_add_u8.postcondition";
const PRODUCED_AT: &str = "2026-05-08T00:00:00Z";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if !args.iter().any(|arg| arg == "--rpc") {
        eprintln!("Usage: bridgeworks-c-witness-lowerer --rpc");
        std::process::exit(1);
    }
    run_rpc();
}

fn run_rpc() {
    let stdin = io::stdin();
    let mut workspace_root = PathBuf::from(".");
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
            "initialize" => {
                if let Some(root) = request
                    .pointer("/params/workspace_root")
                    .and_then(Value::as_str)
                {
                    workspace_root = PathBuf::from(root);
                }
                respond(
                    id,
                    json!({
                        "name": "bridgeworks-c-witness-lowerer",
                        "version": "0.1.0",
                        "protocol_version": "provekit-orp/1",
                        "capabilities": {
                            "kits": ["c"],
                            "modes": ["witness"],
                            "obligationKinds": ["predicate"],
                            "predicates": [OBLIGATION],
                            "witnessTargets": ["native-c-exhaustive-u8"]
                        }
                    }),
                );
            }
            "realize" => {
                let root = request
                    .pointer("/params/workspace_root")
                    .and_then(Value::as_str)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| workspace_root.clone());
                let plan = request
                    .pointer("/params/plan")
                    .cloned()
                    .unwrap_or(Value::Null);
                match realize_witness(&root, &plan) {
                    Ok(result) => respond(id, result),
                    Err(error) => respond_error(id, 1009, &error),
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

fn realize_witness(workspace_root: &Path, plan: &Value) -> Result<Value, String> {
    let obligation = plan
        .pointer("/obligation/name")
        .and_then(Value::as_str)
        .ok_or_else(|| "RealizerPlan missing obligation.name".to_string())?;
    if obligation != OBLIGATION {
        return Ok(refusal(
            "UNSUPPORTED_PREDICATE",
            &format!("predicate {obligation} is not supported by this C witness lowerer"),
            None,
            None,
        ));
    }
    let artifact_rel = plan
        .pointer("/host/artifact")
        .and_then(Value::as_str)
        .ok_or_else(|| "RealizerPlan missing host.artifact".to_string())?;
    let entrypoint = plan
        .pointer("/host/entrypoint")
        .and_then(Value::as_str)
        .unwrap_or("checked_add_u8");
    if entrypoint != "checked_add_u8" {
        return Ok(refusal(
            "UNSUPPORTED_ENTRYPOINT",
            &format!("entrypoint {entrypoint} is not supported"),
            None,
            None,
        ));
    }

    let artifact = workspace_root.join(artifact_rel);
    let source =
        std::fs::read(&artifact).map_err(|e| format!("read {}: {e}", artifact.display()))?;
    let artifact_cid = blake3_512(&source);
    let run = run_exhaustive_harness(workspace_root, artifact_rel)?;
    if let Some(counterexample) = run.counterexample {
        let message = format!(
            "counterexample: a={} b={}\nexpected: overflow={} value={}\nobserved: overflow={} value={}",
            counterexample["a"].as_u64().unwrap_or(0),
            counterexample["b"].as_u64().unwrap_or(0),
            bool_word(counterexample["expectedOverflow"].as_bool().unwrap_or(false)),
            counterexample["expectedValue"].as_u64().unwrap_or(0),
            bool_word(counterexample["observedOverflow"].as_bool().unwrap_or(false)),
            counterexample["observedValue"].as_u64().unwrap_or(0),
        );
        return Ok(refusal(
            "COUNTEREXAMPLE",
            &message,
            Some(counterexample),
            Some(json!({
                "kind": "c-source",
                "role": "exhaustive-value-witness",
                "cid": run.witness_artifact_cid,
                "source": run.witness_source,
            })),
        ));
    }

    let policy_cid = plan
        .get("policyCid")
        .and_then(Value::as_str)
        .unwrap_or("builtin:bridgeworks.checked-add-u8.exhaustive-u8");
    let claim_body = json!({
        "kind": "TruthDischargeBodyClaim",
        "schemaVersion": "1",
        "claimKind": "checked-add-u8-postcondition-witness",
        "proposition": "c-entrypoint-satisfies-checked-add-u8-postcondition",
        "subjectCids": {
            "artifactCid": artifact_cid,
        },
        "obligation": {
            "kind": "predicate",
            "name": OBLIGATION,
        },
        "host": {
            "kit": "c",
            "artifact": artifact_rel,
            "entrypoint": entrypoint,
        },
        "verifierCid": "builtin:bridgeworks-c-exhaustive-u8-witnesser@0.1.0",
        "policyCid": policy_cid,
        "inputCids": [artifact_cid],
    });
    let evidence = json!({
        "kind": "CValueWitness",
        "schemaVersion": "1",
        "language": "c",
        "artifactCid": artifact_cid,
        "entrypoint": entrypoint,
        "witnessArtifact": {
            "kind": "c-source",
            "role": "exhaustive-value-witness",
            "cid": run.witness_artifact_cid,
            "source": run.witness_source,
        },
        "casesChecked": run.cases_checked,
        "overflowWitness": {
            "a": 1,
            "b": 255,
            "observed": {"overflow": true, "value": 0},
            "expected": {"overflow": true, "value": 0}
        },
        "domain": {
            "a": "uint8_t",
            "b": "uint8_t"
        }
    });
    let evidence_cid = jcs_cid(&evidence);
    Ok(json!({
        "output": {
            "kind": "RealizerOutput",
            "schemaVersion": "1",
            "mode": "attest",
            "status": "witnessed",
            "obligationCid": jcs_cid(&plan["obligation"]),
            "evidenceCid": evidence_cid,
            "observedArtifactCids": [artifact_cid],
            "realizer": {
                "name": "bridgeworks-c-witness-lowerer",
                "version": "0.1.0",
                "kit": "c"
            },
            "diagnostics": []
        },
        "claimKind": "checked-add-u8-postcondition-witness",
        "claimBody": claim_body,
        "evidence": evidence,
        "evidenceCid": evidence_cid,
        "verifierCid": "builtin:bridgeworks-c-exhaustive-u8-witnesser@0.1.0",
        "policyCid": policy_cid,
        "inputCids": [artifact_cid],
        "producedAt": PRODUCED_AT
    }))
}

fn refusal(
    reason_code: &str,
    message: &str,
    counterexample: Option<Value>,
    witness_artifact: Option<Value>,
) -> Value {
    json!({
        "output": {
            "kind": "RealizerOutput",
            "schemaVersion": "1",
            "mode": "attest",
            "status": "rejected",
            "reasonCode": reason_code,
            "message": message,
            "counterexample": counterexample,
            "witnessArtifact": witness_artifact,
            "realizer": {
                "name": "bridgeworks-c-witness-lowerer",
                "version": "0.1.0",
                "kit": "c"
            },
            "diagnostics": []
        }
    })
}

struct HarnessRun {
    cases_checked: u64,
    witness_artifact_cid: String,
    witness_source: String,
    counterexample: Option<Value>,
}

fn run_exhaustive_harness(workspace_root: &Path, artifact_rel: &str) -> Result<HarnessRun, String> {
    let dir = temp_dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    let harness = dir.join("checked_add_witness.c");
    let bin = dir.join("checked_add_witness");
    let source = format!(
        r#"#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>

#include {}

int main(void) {{
    unsigned long cases = 0;
    for (unsigned int a = 0; a < 256; a++) {{
        for (unsigned int b = 0; b < 256; b++) {{
            checked_add_u8_result observed = checked_add_u8((uint8_t)a, (uint8_t)b);
            uint16_t wide = (uint16_t)a + (uint16_t)b;
            bool expected_overflow = wide >= 256;
            uint8_t expected_value = expected_overflow ? 0 : (uint8_t)wide;
            cases++;
            if (observed.overflow != expected_overflow || observed.value != expected_value) {{
                printf("{{\"status\":\"counterexample\",\"a\":%u,\"b\":%u,\"expectedOverflow\":%s,\"expectedValue\":%u,\"observedOverflow\":%s,\"observedValue\":%u}}\n",
                    a,
                    b,
                    expected_overflow ? "true" : "false",
                    expected_value,
                    observed.overflow ? "true" : "false",
                    observed.value);
                return 42;
            }}
        }}
    }}
    printf("{{\"status\":\"witnessed\",\"casesChecked\":%lu}}\n", cases);
    return 0;
}}
"#,
        c_string_literal(artifact_rel)
    );
    let witness_artifact_cid = blake3_512(source.as_bytes());
    std::fs::write(&harness, &source).map_err(|e| format!("write {}: {e}", harness.display()))?;
    let compile = Command::new("cc")
        .arg("-std=c11")
        .arg("-Wall")
        .arg("-Wextra")
        .arg("-I")
        .arg(workspace_root)
        .arg(&harness)
        .arg("-o")
        .arg(&bin)
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("spawn cc: {e}"))?;
    if !compile.status.success() {
        let stderr = String::from_utf8_lossy(&compile.stderr);
        let _ = std::fs::remove_dir_all(&dir);
        return Err(format!("compile C witness harness failed:\n{stderr}"));
    }
    let output = Command::new(&bin)
        .output()
        .map_err(|e| format!("run C witness harness: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| "C witness harness produced no output".to_string())?;
    let value: Value =
        serde_json::from_str(line).map_err(|e| format!("parse C witness output: {e}: {line}"))?;
    let _ = std::fs::remove_dir_all(&dir);
    if value.get("status").and_then(Value::as_str) == Some("counterexample") {
        return Ok(HarnessRun {
            cases_checked: 0,
            witness_artifact_cid,
            witness_source: source,
            counterexample: Some(value),
        });
    }
    if !output.status.success() {
        return Err(format!(
            "C witness harness exited {}\nstdout:\n{stdout}",
            output.status
        ));
    }
    Ok(HarnessRun {
        cases_checked: value
            .get("casesChecked")
            .and_then(Value::as_u64)
            .unwrap_or(65_536),
        witness_artifact_cid,
        witness_source: source,
        counterexample: None,
    })
}

fn bool_word(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

fn temp_dir() -> Result<PathBuf, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("system clock before UNIX_EPOCH: {e}"))?
        .as_nanos();
    Ok(std::env::temp_dir().join(format!(
        "provekit-bridgeworks-c-witness-{}-{now}",
        std::process::id()
    )))
}

fn c_string_literal(path: impl AsRef<str>) -> String {
    let mut out = String::from("\"");
    for ch in path.as_ref().chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn blake3_512(bytes: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(bytes);
    let mut out = [0u8; 64];
    hasher.finalize_xof().fill(&mut out);
    format!("blake3-512:{}", hex_lower(&out))
}

fn jcs_cid(value: &Value) -> String {
    blake3_512(&jcs(value).into_bytes())
}

fn jcs(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(v) => v.to_string(),
        Value::Number(v) => v.to_string(),
        Value::String(v) => serde_json::to_string(v).expect("string serializes"),
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(jcs).collect();
            format!("[{}]", parts.join(","))
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys
                .into_iter()
                .map(|key| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(key).expect("key serializes"),
                        jcs(&map[key])
                    )
                })
                .collect();
            format!("{{{}}}", parts.join(","))
        }
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}
