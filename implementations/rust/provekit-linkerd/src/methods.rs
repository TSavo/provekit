// SPDX-License-Identifier: Apache-2.0
//
// methods.rs — implementations of the five JSON-RPC methods per spec R5-R9.
//
// parseFile    (R5): update kit stream + re-link + return per-file diagnostics.
// getDiagnostics (R6): return cached diagnostics for a file without re-lifting.
// projectStatus (R7): return rank-3 pin from last link() call.
// flushCache   (R8): invalidate cached derivations.
// shutdown     (R9): write cache snapshot + signal server to exit.
//
// Lifting strategy for parseFile:
//   For `rust-kit` sources, this daemon MVP writes the source bytes to a
//   temporary directory and invokes `provekit_lift::lift_path`. For all
//   other kits the daemon returns error -33002 (lifter unavailable) —
//   those kits will be added per-kit in step 3 of the LSP+linker path.
//
//   This is the only file that touches provekit_lift; the rest of the
//   daemon is kit-agnostic.

use std::path::PathBuf;
use std::sync::Arc;

use provekit_linker::{LinkerCallEdge, LinkerContract};
use serde_json::Value as Json;
use tokio::sync::Mutex;
use tracing::instrument;

use crate::state::ProjectState;

// -------------------------------------------------------------------
// Error codes (R10)
// -------------------------------------------------------------------

pub const ERR_METHOD_NOT_FOUND: i64 = -32601;
pub const ERR_INVALID_PARAMS: i64 = -32602;
pub const ERR_KIT_NOT_IN_MANIFEST: i64 = -33001;
pub const ERR_LIFTER_UNAVAILABLE: i64 = -33002;
#[allow(dead_code)]
pub const ERR_LINKER_DISCHARGE_FAILURE: i64 = -33003;

pub fn rpc_error(code: i64, message: &str, id: &Json) -> Json {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
        }
    })
}

pub fn rpc_result(result: Json, id: &Json) -> Json {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

// -------------------------------------------------------------------
// R5: parseFile
// -------------------------------------------------------------------

/// Handle a `parseFile` request.
///
/// Params: `{ "kitId": <str>, "file": <absolute path>, "source": <str> }`
/// Returns: `{ "diagnostics": [LinterError] }` filtered to `file`.
#[instrument(skip(state, params))]
pub async fn handle_parse_file(
    state: Arc<Mutex<ProjectState>>,
    params: &Json,
    id: &Json,
) -> Json {
    let kit_id = match params.get("kitId").and_then(|v| v.as_str()) {
        Some(k) => k.to_string(),
        None => return rpc_error(ERR_INVALID_PARAMS, "missing 'kitId'", id),
    };
    let file = match params.get("file").and_then(|v| v.as_str()) {
        Some(f) => f.to_string(),
        None => return rpc_error(ERR_INVALID_PARAMS, "missing 'file'", id),
    };
    let source = match params.get("source").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => return rpc_error(ERR_INVALID_PARAMS, "missing 'source'", id),
    };

    // Lift source through kit-specific lifter.
    let (contracts, call_edges) = match lift_source(&kit_id, &file, &source).await {
        Ok(result) => result,
        Err(LiftError::LifterUnavailable(msg)) => {
            return rpc_error(ERR_LIFTER_UNAVAILABLE, &msg, id)
        }
        Err(LiftError::KitNotInManifest(msg)) => {
            return rpc_error(ERR_KIT_NOT_IN_MANIFEST, &msg, id)
        }
    };

    let diagnostics = {
        let mut st = state.lock().await;
        let output = st.update_and_link(&kit_id, &file, contracts, call_edges);
        // Filter diagnostics to the file that was just parsed.
        output
            .linker_errors
            .iter()
            .filter(|e| e.file.as_deref() == Some(&file))
            .map(|e| {
                serde_json::json!({
                    "kind": "linker-error",
                    "errorKind": e.kind,
                    "targetSymbol": e.target_symbol,
                    "sourceContractCid": e.source_contract_cid,
                    "reason": e.reason,
                    "file": e.file,
                })
            })
            .collect::<Vec<_>>()
    };

    rpc_result(serde_json::json!({ "diagnostics": diagnostics }), id)
}

// -------------------------------------------------------------------
// R6: getDiagnostics
// -------------------------------------------------------------------

/// Handle a `getDiagnostics` request.
///
/// Params: `{ "file": <absolute path> }`
/// Returns: `[LinterError]` from the last cached link output.
pub async fn handle_get_diagnostics(
    state: Arc<Mutex<ProjectState>>,
    params: &Json,
    id: &Json,
) -> Json {
    let file = match params.get("file").and_then(|v| v.as_str()) {
        Some(f) => f.to_string(),
        None => return rpc_error(ERR_INVALID_PARAMS, "missing 'file'", id),
    };

    let diagnostics = {
        let st = state.lock().await;
        st.diagnostics_for_file(&file)
    };

    rpc_result(Json::Array(diagnostics), id)
}

// -------------------------------------------------------------------
// R7: projectStatus
// -------------------------------------------------------------------

/// Handle a `projectStatus` request.
///
/// Params: `{}`
/// Returns: `{ contractSetCid, callEdgeSetCid, bridgeSetCid, linkBundleCid }`
pub async fn handle_project_status(
    state: Arc<Mutex<ProjectState>>,
    _params: &Json,
    id: &Json,
) -> Json {
    let status = {
        let st = state.lock().await;
        st.project_status()
    };

    match status {
        Some(s) => rpc_result(s, id),
        None => rpc_result(
            serde_json::json!({
                "contractSetCid": null,
                "callEdgeSetCid": null,
                "bridgeSetCid":   null,
                "linkBundleCid":  null,
            }),
            id,
        ),
    }
}

// -------------------------------------------------------------------
// R8: flushCache
// -------------------------------------------------------------------

/// Handle a `flushCache` request.
///
/// Params: `{}`
/// Returns: `null`
pub async fn handle_flush_cache(
    state: Arc<Mutex<ProjectState>>,
    _params: &Json,
    id: &Json,
) -> Json {
    {
        let mut st = state.lock().await;
        st.flush_cache();
    }
    rpc_result(Json::Null, id)
}

// -------------------------------------------------------------------
// R9: shutdown (handled in server.rs — snapshot write happens there)
// -------------------------------------------------------------------

// The shutdown method is handled directly in server.rs because it needs
// access to the snapshot path and the shutdown signal. We export the
// response-building helper here for use by server.rs.

pub fn shutdown_response(id: &Json) -> Json {
    rpc_result(Json::Null, id)
}

// -------------------------------------------------------------------
// Lifter dispatch (kit-specific)
// -------------------------------------------------------------------

enum LiftError {
    LifterUnavailable(String),
    KitNotInManifest(String),
}

/// Lift `source` for the given `kit_id` and return `(contracts, call_edges)`.
///
/// For `rust-kit`: writes source to a temp dir and calls `provekit_lift::lift_path`.
/// For all other kits: returns `LiftError::LifterUnavailable` (step 3 adds them).
async fn lift_source(
    kit_id: &str,
    file: &str,
    source: &str,
) -> Result<(Vec<LinkerContract>, Vec<LinkerCallEdge>), LiftError> {
    match kit_id {
        "rust" => lift_rust_source(file, source).await,
        "go" | "cpp" | "csharp" | "python" | "ruby" | "swift" | "ts" | "zig" | "java" | "c" => {
            Err(LiftError::LifterUnavailable(format!(
                "kit lifter for '{kit_id}' is not yet implemented in provekit-linkerd MVP; \
                 add via step 3 (per-kit LSP plugin daemon-client mode)"
            )))
        }
        other => Err(LiftError::KitNotInManifest(format!(
            "unknown kitId '{other}'; valid kits: rust, go, cpp, csharp, python, ruby, swift, ts, zig, java, c"
        ))),
    }
}

/// Lift a single Rust source file.
///
/// Writes the source to a fresh temp directory so `provekit_lift::lift_path`
/// can walk it. Returns `(LinkerContract[], LinkerCallEdge[])`.
async fn lift_rust_source(
    file: &str,
    source: &str,
) -> Result<(Vec<LinkerContract>, Vec<LinkerCallEdge>), LiftError> {
    use provekit_linker::{LinkerCallEdge as LinkerEdge, LinkerContract};

    // Write source to a temp dir (blocking I/O, run in spawn_blocking).
    let source_owned = source.to_string();
    let file_name = PathBuf::from(file)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "lifted.rs".to_string());

    let result = tokio::task::spawn_blocking(move || {
        let tmp_dir = std::env::temp_dir().join(format!(
            "provekit-linkerd-lift-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&tmp_dir)
            .map_err(|e| LiftError::LifterUnavailable(format!("create temp dir: {e}")))?;

        let tmp_file = tmp_dir.join(&file_name);
        std::fs::write(&tmp_file, &source_owned)
            .map_err(|e| LiftError::LifterUnavailable(format!("write temp file: {e}")))?;

        let report = provekit_lift::lift_path(&tmp_dir);

        // Convert provekit_lift::ContractDecl -> LinkerContract.
        let mut contracts: Vec<LinkerContract> = Vec::new();
        for decl in &report.decls {
            use provekit_ir_symbolic::serialize::formula_to_value;
            use provekit_claim_envelope::{contract_cid as compute_contract_cid, MintContractArgs, Authoring};

            let pre_v = decl.pre.as_deref().map(formula_to_value);
            let post_v = decl.post.as_deref().map(formula_to_value);
            let inv_v = decl.inv.as_deref().map(formula_to_value);

            let args = MintContractArgs {
                contract_name: decl.name.clone(),
                pre: pre_v.clone(),
                post: post_v.clone(),
                inv: inv_v.clone(),
                out_binding: decl.out_binding.clone(),
                produced_by: "provekit-linkerd@0.1.0".into(),
                produced_at: "2026-05-04T00:00:00.000Z".into(),
                input_cids: vec![],
                authoring: Authoring::Lift {
                    lifter: "provekit-lift".into(),
                    evidence: format!("lifted from `{}` annotations", decl.name),
                    source_cid: None,
                },
                signer_seed: [0x42; 32],
            };
            let cid = compute_contract_cid(&args);

            let pre_json = pre_v.map(|v| value_arc_to_json(&v));
            let post_json = post_v.map(|v| value_arc_to_json(&v));

            contracts.push(LinkerContract {
                name: decl.name.clone(),
                kit: "rust-kit".into(),
                contract_cid: cid,
                pre_json,
                post_json,
            });
        }

        // Convert call-edge mementos -> LinkerEdge.
        // Build name->cid index from this file's contracts for same-file resolution.
        let name_to_cid: std::collections::BTreeMap<String, String> = contracts
            .iter()
            .map(|c| (c.name.clone(), c.contract_cid.clone()))
            .collect();

        let mut call_edges: Vec<LinkerEdge> = Vec::new();
        for edge in &report.call_edges {
            let target_cid = name_to_cid.get(&edge.target_symbol).cloned();
            call_edges.push(LinkerEdge {
                source_contract_cid: edge.source_contract_cid.clone(),
                target_contract_cid: target_cid,
                target_symbol: edge.target_symbol.clone(),
                call_site_locus_json: serde_json::json!({
                    "file": file_name,
                    "line": edge.call_site_locus.line,
                    "column": edge.call_site_locus.col,
                }),
                evidence_term_json: serde_json::json!({
                    "kind": "Atomic",
                    "name": "call-site-obligation",
                    "args": [],
                }),
            });
        }

        // Clean up temp dir (best effort).
        let _ = std::fs::remove_dir_all(&tmp_dir);

        Ok((contracts, call_edges))
    })
    .await;

    match result {
        Ok(inner) => inner,
        Err(join_err) => Err(LiftError::LifterUnavailable(format!(
            "spawn_blocking error: {join_err}"
        ))),
    }
}

fn value_arc_to_json(v: &std::sync::Arc<provekit_canonicalizer::Value>) -> Json {
    value_to_json(v)
}

fn value_to_json(v: &provekit_canonicalizer::Value) -> Json {
    use provekit_canonicalizer::Value;
    match v {
        Value::Null => Json::Null,
        Value::Bool(b) => Json::Bool(*b),
        Value::Integer(i) => Json::Number((*i).into()),
        Value::String(s) => Json::String(s.clone()),
        Value::Array(items) => Json::Array(items.iter().map(|i| value_to_json(i)).collect()),
        Value::Object(kvs) => {
            let mut map = serde_json::Map::new();
            for (k, val) in kvs {
                map.insert(k.clone(), value_to_json(val));
            }
            Json::Object(map)
        }
    }
}
