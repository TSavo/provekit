// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use serde_json::{json, Value};
use tree_sitter::{Node, Parser};

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 2 || args[1] != "--rpc" {
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

    if contract == "runtime.no-env-secret-read" {
        return realize_runtime_no_env(contract, artifact, &source, &artifact_cid, plan);
    }

    if let Some(message) = refusal_for(contract, artifact, &source) {
        let evidence = json!({
            "kind": "static-js-witness-refusal",
            "contract": contract,
            "artifact": artifact,
            "counterexample": message,
            "witnessArtifact": {
                "language": "javascript-static-analysis",
                "source": format!("scan({artifact}) => reject when preserved contract {contract} is contradicted")
            }
        });
        let evidence_json =
            serde_json::to_string(&evidence).map_err(|e| format!("serialize evidence: {e}"))?;
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
            "evidence": evidence,
            "evidenceCid": blake3_512(evidence_json.as_bytes()),
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
    let evidence_json =
        serde_json::to_string(&evidence).map_err(|e| format!("serialize evidence: {e}"))?;
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
        "evidenceCid": blake3_512(evidence_json.as_bytes()),
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

fn realize_runtime_no_env(
    contract: &str,
    artifact: &str,
    source: &str,
    artifact_cid: &str,
    plan: &Value,
) -> Result<Value, String> {
    let analysis = analyze_runtime_no_env_secret_read(artifact, source)?;
    let policy_cid = plan
        .get("policyCid")
        .and_then(Value::as_str)
        .unwrap_or("builtin:supply-chain-rails/npm-safe-json@0.1");
    let proof_ir = plan
        .pointer("/obligation/proofIr")
        .cloned()
        .unwrap_or_else(|| json!({"kind": "atomic", "name": contract, "args": []}));
    let contract_cid = jcs_cid(&proof_ir);
    let findings = findings_json(&analysis.findings);
    let unsupported = unsupported_json(&analysis.unsupported);
    let source_spans = source_spans_json(&analysis);
    let lowerer = json!({"name": "supply-chain-js-lowerer", "version": "0.2.0"});
    let evidence = json!({
        "kind": "javascript-runtime-no-env-evidence",
        "contract": contract,
        "contractCid": contract_cid,
        "artifact": artifact,
        "artifactCid": artifact_cid,
        "lowerer": lowerer,
        "mode": "witness",
        "findings": findings,
        "unsupportedSemantics": unsupported,
        "sourceSpans": source_spans,
    });
    let evidence_cid = jcs_cid(&evidence);
    let claim_body = json!({
        "claimKind": "npm-package-contract-witness",
        "contract": contract,
        "contractCid": contract_cid,
        "subjectCids": [artifact_cid],
        "policyCid": policy_cid
    });

    let common = json!({
        "kind": "ORPLowerResult",
        "claimKind": "npm-package-contract-witness",
        "policyCid": policy_cid,
        "verifierCid": "builtin:supply-chain-rails/js-static-lowerer@0.2",
        "claimBody": claim_body,
        "evidence": evidence,
        "evidenceCid": evidence_cid,
        "inputCids": [artifact_cid, contract_cid],
        "producedAt": "2026-05-08T00:00:00Z"
    });

    if let Some(finding) = analysis.findings.first() {
        let message = format!(
            "counterexample: {} at {}:{} violates {contract}",
            finding.expression, finding.span.line_start, finding.span.column_start
        );
        return Ok(merge_output(
            common,
            json!({
                "status": "rejected",
                "reasonCode": finding.reason_code,
                "message": message,
                "error": message,
                "findings": findings,
                "unsupportedSemantics": unsupported,
                "sourceSpans": source_spans,
                "realizer": lowerer,
                "observedArtifactCids": [artifact_cid],
                "witnessArtifact": {
                    "language": "javascript-ast-analysis",
                    "source": format!("parse({artifact}) => reject when AST contradicts preserved contract {contract}")
                }
            }),
        ));
    }

    if let Some(unsupported_surface) = analysis.unsupported.first() {
        let message = format!(
            "unsupported JavaScript semantics: {} at {}:{}",
            unsupported_surface.reason_code,
            unsupported_surface.span.line_start,
            unsupported_surface.span.column_start
        );
        return Ok(merge_output(
            common,
            json!({
                "status": "rejected",
                "reasonCode": unsupported_surface.reason_code,
                "message": message,
                "error": message,
                "findings": findings,
                "unsupportedSemantics": unsupported,
                "sourceSpans": source_spans,
                "realizer": lowerer,
                "observedArtifactCids": [artifact_cid],
                "witnessArtifact": {
                    "language": "javascript-ast-analysis",
                    "source": format!("parse({artifact}) => fail closed for unsupported JavaScript while lowering {contract}")
                }
            }),
        ));
    }

    Ok(merge_output(
        common,
        json!({
            "status": "witnessed",
            "message": format!("accepted witness for {contract}"),
            "findings": findings,
            "unsupportedSemantics": unsupported,
            "sourceSpans": source_spans,
            "realizer": lowerer,
            "observedArtifactCids": [artifact_cid],
            "witnessArtifact": {
                "language": "javascript-ast-analysis",
                "source": format!("parse({artifact}) => accepted contract {contract}; no AST env reads matched")
            }
        }),
    ))
}

fn merge_output(mut base: Value, mut output: Value) -> Value {
    let evidence_cid = base
        .get("evidenceCid")
        .cloned()
        .unwrap_or_else(|| Value::Null);
    if let Some(output_obj) = output.as_object_mut() {
        output_obj.insert("evidenceCid".to_string(), evidence_cid);
    }
    if let Some(base_obj) = base.as_object_mut() {
        base_obj.insert("output".to_string(), output);
    }
    base
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceSpan {
    line_start: usize,
    column_start: usize,
    line_end: usize,
    column_end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EnvFinding {
    reason_code: &'static str,
    expression: String,
    span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UnsupportedSurface {
    reason_code: &'static str,
    expression: String,
    span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeNoEnvAnalysis {
    findings: Vec<EnvFinding>,
    unsupported: Vec<UnsupportedSurface>,
}

fn analyze_runtime_no_env_secret_read(
    _artifact: &str,
    source: &str,
) -> Result<RuntimeNoEnvAnalysis, String> {
    let mut parser = Parser::new();
    let language = tree_sitter_javascript::LANGUAGE.into();
    parser
        .set_language(&language)
        .map_err(|e| format!("initialize JavaScript parser: {e}"))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| "parse JavaScript source".to_string())?;
    let root = tree.root_node();
    let source_bytes = source.as_bytes();

    let mut aliases = BTreeSet::new();
    collect_env_aliases(root, source_bytes, &mut aliases);

    let mut analysis = RuntimeNoEnvAnalysis {
        findings: Vec::new(),
        unsupported: Vec::new(),
    };
    if root.has_error() {
        analysis.unsupported.push(UnsupportedSurface {
            reason_code: "parse-error",
            expression: "<parse-error>".to_string(),
            span: span_for(root),
        });
    }
    scan_env_reads(root, source_bytes, &aliases, &mut analysis);
    Ok(analysis)
}

fn collect_env_aliases(node: Node<'_>, source: &[u8], aliases: &mut BTreeSet<String>) {
    if node.kind() == "variable_declarator" {
        if let (Some(name), Some(value)) = (
            node.child_by_field_name("name"),
            node.child_by_field_name("value"),
        ) {
            let name_text = node_text(name, source);
            let value_text = compact_js_text(&node_text(value, source));
            if name.kind() == "identifier" && value_text == "process.env" {
                aliases.insert(name_text);
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_env_aliases(child, source, aliases);
    }
}

fn scan_env_reads(
    node: Node<'_>,
    source: &[u8],
    aliases: &BTreeSet<String>,
    analysis: &mut RuntimeNoEnvAnalysis,
) {
    let text = node_text(node, source);
    let compact = compact_js_text(&text);
    match node.kind() {
        "member_expression" => {
            if is_process_env_member(&compact)
                && !is_env_alias_initializer(node, source, aliases)
                && !is_nested_process_env_prefix(node, source)
            {
                push_finding(
                    analysis,
                    EnvFinding {
                        reason_code: "env-secret-read",
                        expression: text.clone(),
                        span: span_for(node),
                    },
                );
            }
            for alias in aliases {
                if compact.starts_with(&format!("{alias}.")) {
                    push_finding(
                        analysis,
                        EnvFinding {
                            reason_code: "env-secret-read",
                            expression: text.clone(),
                            span: span_for(node),
                        },
                    );
                }
            }
        }
        "subscript_expression" => {
            if compact.starts_with("process[") {
                push_unsupported(
                    analysis,
                    UnsupportedSurface {
                        reason_code: "dynamic-env-access",
                        expression: text,
                        span: span_for(node),
                    },
                );
            }
        }
        "call_expression" => {
            if compact.starts_with("require(") || compact.starts_with("import(") {
                push_unsupported(
                    analysis,
                    UnsupportedSurface {
                        reason_code: "unresolved-module-call",
                        expression: text,
                        span: span_for(node),
                    },
                );
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        scan_env_reads(child, source, aliases, analysis);
    }
}

fn is_process_env_member(compact: &str) -> bool {
    compact == "process.env" || compact.starts_with("process.env.")
}

fn is_nested_process_env_prefix(node: Node<'_>, source: &[u8]) -> bool {
    if compact_js_text(&node_text(node, source)) != "process.env" {
        return false;
    }
    let Some(parent) = node.parent() else {
        return false;
    };
    parent.kind() == "member_expression"
        && is_process_env_member(&compact_js_text(&node_text(parent, source)))
}

fn is_env_alias_initializer(node: Node<'_>, source: &[u8], aliases: &BTreeSet<String>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.kind() != "variable_declarator" {
        return false;
    }
    let Some(name) = parent.child_by_field_name("name") else {
        return false;
    };
    let Some(value) = parent.child_by_field_name("value") else {
        return false;
    };
    aliases.contains(&node_text(name, source)) && value.id() == node.id()
}

fn push_finding(analysis: &mut RuntimeNoEnvAnalysis, finding: EnvFinding) {
    if !analysis.findings.iter().any(|existing| {
        existing.reason_code == finding.reason_code
            && existing.expression == finding.expression
            && existing.span == finding.span
    }) {
        analysis.findings.push(finding);
    }
}

fn push_unsupported(analysis: &mut RuntimeNoEnvAnalysis, unsupported: UnsupportedSurface) {
    if !analysis.unsupported.iter().any(|existing| {
        existing.reason_code == unsupported.reason_code
            && existing.expression == unsupported.expression
            && existing.span == unsupported.span
    }) {
        analysis.unsupported.push(unsupported);
    }
}

fn node_text(node: Node<'_>, source: &[u8]) -> String {
    node.utf8_text(source)
        .unwrap_or("<invalid-utf8>")
        .trim()
        .to_string()
}

fn compact_js_text(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn span_for(node: Node<'_>) -> SourceSpan {
    let start = node.start_position();
    let end = node.end_position();
    SourceSpan {
        line_start: start.row + 1,
        column_start: start.column + 1,
        line_end: end.row + 1,
        column_end: end.column + 1,
    }
}

fn findings_json(findings: &[EnvFinding]) -> Value {
    Value::Array(
        findings
            .iter()
            .map(|finding| {
                json!({
                    "reasonCode": finding.reason_code,
                    "expression": finding.expression,
                    "span": span_json(&finding.span),
                })
            })
            .collect(),
    )
}

fn unsupported_json(unsupported: &[UnsupportedSurface]) -> Value {
    Value::Array(
        unsupported
            .iter()
            .map(|surface| {
                json!({
                    "reasonCode": surface.reason_code,
                    "expression": surface.expression,
                    "span": span_json(&surface.span),
                })
            })
            .collect(),
    )
}

fn source_spans_json(analysis: &RuntimeNoEnvAnalysis) -> Value {
    let mut spans = Vec::new();
    spans.extend(
        analysis
            .findings
            .iter()
            .map(|finding| span_json(&finding.span)),
    );
    spans.extend(
        analysis
            .unsupported
            .iter()
            .map(|surface| span_json(&surface.span)),
    );
    Value::Array(spans)
}

fn span_json(span: &SourceSpan) -> Value {
    json!({
        "lineStart": span.line_start,
        "columnStart": span.column_start,
        "lineEnd": span.line_end,
        "columnEnd": span.column_end,
    })
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

fn jcs_cid(value: &Value) -> String {
    let canonical = json_to_cvalue(value);
    let jcs = encode_jcs(&canonical);
    blake3_512_of(jcs.as_bytes())
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
            } else {
                CValue::integer(0)
            }
        }
        Value::String(s) => CValue::string(s.clone()),
        Value::Array(items) => CValue::array(items.iter().map(json_to_cvalue).collect()),
        Value::Object(map) => CValue::object(
            map.iter()
                .map(|(k, v)| (k.clone(), json_to_cvalue(v)))
                .collect::<Vec<_>>(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_no_env_accepts_source_without_env_reads() {
        let result = analyze_runtime_no_env_secret_read(
            "index.js",
            "export function parseJson(input) { return JSON.parse(input); }\n",
        )
        .expect("analysis succeeds");

        assert!(result.findings.is_empty());
        assert!(result.unsupported.is_empty());
    }

    #[test]
    fn runtime_no_env_rejects_direct_process_env_read_with_span() {
        let result = analyze_runtime_no_env_secret_read(
            "index.js",
            "export function parseJson(input) {\n  return process.env.SAFE_JSON_TOKEN || input;\n}\n",
        )
        .expect("analysis succeeds");

        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].reason_code, "env-secret-read");
        assert_eq!(result.findings[0].expression, "process.env.SAFE_JSON_TOKEN");
        assert_eq!(result.findings[0].span.line_start, 2);
    }

    #[test]
    fn runtime_no_env_rejects_aliased_env_read() {
        let result = analyze_runtime_no_env_secret_read(
            "index.js",
            "const env = process.env;\nexport function parseJson(input) { return env.SAFE_JSON_TOKEN || input; }\n",
        )
        .expect("analysis succeeds");

        assert_eq!(result.findings[0].reason_code, "env-secret-read");
        assert_eq!(result.findings[0].expression, "env.SAFE_JSON_TOKEN");
    }

    #[test]
    fn runtime_no_env_fails_closed_on_dynamic_process_env_read() {
        let result = analyze_runtime_no_env_secret_read(
            "index.js",
            "const key = 'env';\nexport function parseJson(input) { return process[key].SAFE_JSON_TOKEN || input; }\n",
        )
        .expect("analysis succeeds");

        assert_eq!(result.unsupported[0].reason_code, "dynamic-env-access");
    }

    #[test]
    fn runtime_no_env_fails_closed_on_unresolved_require() {
        let result = analyze_runtime_no_env_secret_read(
            "index.js",
            "const helper = require('./helper');\nexport function parseJson(input) { return helper.parse(input); }\n",
        )
        .expect("analysis succeeds");

        assert_eq!(result.unsupported[0].reason_code, "unresolved-module-call");
    }
}
