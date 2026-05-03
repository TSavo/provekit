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
//   For `rust` sources: call `provekit_lift::lift_path` in-process (fast).
//   For `go`, `csharp`, `ruby`, `java`, `swift`, `ts`, `cpp`, `c`, `zig`:
//     spawn the kit's LSP plugin binary, send a JSON-RPC `parse` request,
//     read the `{declarations, callEdges}` response, and map into
//     `LinkerContract`/`LinkerCallEdge` (see `spawn_kit_lifter`).
//   For `zig`: spawn `provekit-lsp-zig` (no args — reads stdin directly).
//     `callEdges` may be omitted from the response and is treated as empty.
//   For `python`: the LSP module has no installed binary (it's a Python module,
//     invoked as `python -m provekit_lift_py_tests.lsp`); additionally, its
//     `declarations` field is a JSON-encoded string rather than a JSON array
//     (shape divergence from go/csharp/ruby). Documented gap; returns
//     LifterUnavailable until a proper installed binary ships.
//   For `java`: spawn `provekit-lsp-java --rpc`, same protocol as go/csharp/ruby.
//     Requires `mvn package` in implementations/java/provekit-lift-java-core first;
//     returns LifterUnavailable if the binary is not on PATH.
//   For `swift`: spawn `provekit-lsp-swift` (no args — reads stdin directly).
//     Binary is built via `swift build -c release` in implementations/swift.
//   For `ts`: spawn `provekit-lsp-ts` (no args — node-based CJS binary).
//     Binary must be on PATH; returns LifterUnavailable if not installed.
//   For `cpp`: spawn `provekit-lsp-cpp` (no args — native binary, int main()).
//     Binary built via `g++ -std=c++17 -o provekit-lsp-cpp main.cpp`.
//   For `c`: spawn `provekit-lsp-c --rpc` (requires --rpc flag).
//     Binary built via `cc -std=c11 -o provekit-lsp-c main.c`.
//
// Binary discovery order (for subprocess kits):
//   1. Check PATH for the named binary.
//   2. Return LifterUnavailable with a clear install hint if not found.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;

use provekit_canonicalizer::blake3_512_of;
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
/// Dispatch:
/// - `rust`: in-process via `provekit_lift::lift_path`.
/// - `go`: subprocess `provekit-lsp-go` (no args), method `parse`.
/// - `csharp`: subprocess `provekit-lsp-csharp --rpc`, method `parse`.
/// - `ruby`: subprocess `provekit-lsp-ruby --rpc`, method `parse`.
/// - `zig`: subprocess `provekit-lsp-zig` (no args), method `parse`; `callEdges`
///   field may be absent from response and is treated as empty.
/// - `python`: no installed binary + shape divergence in response. LifterUnavailable.
/// - `java`: subprocess `provekit-lsp-java --rpc`, method `parse`; binary must be
///   installed via `mvn package` in implementations/java/provekit-lift-java-core.
/// - `swift`: subprocess `provekit-lsp-swift` (no args), method `parse`.
/// - `ts`: subprocess `provekit-lsp-ts` (no args), method `parse`.
/// - `cpp`: subprocess `provekit-lsp-cpp` (no args), method `parse`.
/// - `c`: subprocess `provekit-lsp-c --rpc`, method `parse`.
async fn lift_source(
    kit_id: &str,
    file: &str,
    source: &str,
) -> Result<(Vec<LinkerContract>, Vec<LinkerCallEdge>), LiftError> {
    match kit_id {
        "rust" => lift_rust_source(file, source).await,

        "go" => {
            let binary = find_binary("provekit-lsp-go").ok_or_else(|| {
                LiftError::LifterUnavailable(
                    "kit 'go' binary not found on PATH; install via: \
                     cd implementations/go && go install ./cmd/provekit-lsp-go"
                        .to_string(),
                )
            })?;
            spawn_kit_lifter(&binary, &[], file, source, "go-kit")
        }

        "csharp" => {
            let binary = find_binary("provekit-lsp-csharp").ok_or_else(|| {
                LiftError::LifterUnavailable(
                    "kit 'csharp' binary not found on PATH; install via: \
                     cd implementations/csharp && dotnet publish -c Release -o ~/.local/bin"
                        .to_string(),
                )
            })?;
            spawn_kit_lifter(&binary, &["--rpc"], file, source, "csharp-kit")
        }

        "ruby" => {
            let binary = find_binary("provekit-lsp-ruby").ok_or_else(|| {
                LiftError::LifterUnavailable(
                    "kit 'ruby' binary not found on PATH; install via: \
                     cp implementations/ruby/bin/provekit-lsp-ruby ~/.local/bin/ && chmod +x ~/.local/bin/provekit-lsp-ruby"
                        .to_string(),
                )
            })?;
            spawn_kit_lifter(&binary, &["--rpc"], file, source, "ruby-kit")
        }

        "zig" => {
            let binary = find_binary("provekit-lsp-zig").ok_or_else(|| {
                LiftError::LifterUnavailable(
                    "kit 'zig' binary not found on PATH; install via: \
                     cd implementations/zig/provekit-lsp-zig && zig build -Doptimize=ReleaseSafe"
                        .to_string(),
                )
            })?;
            // Note: zig lsp binary reads stdin directly (no --rpc flag needed).
            // callEdges may be omitted from its response; treated as empty.
            spawn_kit_lifter(&binary, &[], file, source, "zig-kit")
        }

        "python" => Err(LiftError::LifterUnavailable(
            "kit 'python' lifter has no installed binary (it ships as a Python module, \
             not a standalone executable) and its RPC response uses a non-standard \
             shape (declarations encoded as a JSON string rather than a JSON array). \
             Gap documented in spec §3 R5 commentary. Follow-up required."
                .to_string(),
        )),

        "java" => {
            let binary = find_binary("provekit-lsp-java").ok_or_else(|| {
                LiftError::LifterUnavailable(
                    "kit 'java' binary not found on PATH; install via: \
                     cd implementations/java/provekit-lift-java-core && \
                     mvn package -q && \
                     cp target/appassembler/bin/provekit-lsp-java ~/.local/bin/"
                        .to_string(),
                )
            })?;
            spawn_kit_lifter(&binary, &["--rpc"], file, source, "java-kit")
        }

        "swift" => {
            let binary = find_binary("provekit-lsp-swift").ok_or_else(|| {
                LiftError::LifterUnavailable(
                    "kit 'swift' binary not found on PATH; install via: \
                     cd implementations/swift && swift build -c release && \
                     cp .build/release/provekit-lsp-swift ~/.local/bin/"
                        .to_string(),
                )
            })?;
            // swift lsp binary reads stdin directly (no --rpc flag needed).
            spawn_kit_lifter(&binary, &[], file, source, "swift-kit")
        }

        "ts" => {
            let binary = find_binary("provekit-lsp-ts").ok_or_else(|| {
                LiftError::LifterUnavailable(
                    "kit 'ts' binary not found on PATH; install via: \
                     cd implementations/typescript && pnpm install && pnpm build && \
                     cp bin/provekit-lsp-ts.cjs ~/.local/bin/provekit-lsp-ts && \
                     chmod +x ~/.local/bin/provekit-lsp-ts"
                        .to_string(),
                )
            })?;
            // ts lsp binary is a node CJS shim; reads stdin directly (no --rpc flag needed).
            spawn_kit_lifter(&binary, &[], file, source, "ts-kit")
        }

        "cpp" => {
            let binary = find_binary("provekit-lsp-cpp").ok_or_else(|| {
                LiftError::LifterUnavailable(
                    "kit 'cpp' binary not found on PATH; install via: \
                     cd implementations/cpp/provekit-lsp-cpp && \
                     g++ -std=c++17 -O2 -o provekit-lsp-cpp main.cpp && \
                     cp provekit-lsp-cpp ~/.local/bin/"
                        .to_string(),
                )
            })?;
            // cpp lsp binary reads stdin directly (no --rpc flag needed).
            spawn_kit_lifter(&binary, &[], file, source, "cpp-kit")
        }

        "c" => {
            let binary = find_binary("provekit-lsp-c").ok_or_else(|| {
                LiftError::LifterUnavailable(
                    "kit 'c' binary not found on PATH; install via: \
                     cd implementations/c/provekit-lsp-c && \
                     cc -std=c11 -Wall -o provekit-lsp-c main.c && \
                     cp provekit-lsp-c ~/.local/bin/"
                        .to_string(),
                )
            })?;
            // c lsp binary requires --rpc flag (errors without it).
            spawn_kit_lifter(&binary, &["--rpc"], file, source, "c-kit")
        }

        other => Err(LiftError::KitNotInManifest(format!(
            "unknown kitId '{other}'; valid kits: rust, go, cpp, csharp, python, ruby, swift, ts, zig, java, c"
        ))),
    }
}

// -------------------------------------------------------------------
// Binary discovery
// -------------------------------------------------------------------

/// Find a binary by name, checking PATH only.
///
/// Returns `Some(path)` if the binary is found and executable, `None` otherwise.
fn find_binary(name: &str) -> Option<String> {
    // Use `which`-style lookup: search each PATH component.
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in path_var.split(':') {
            let candidate = PathBuf::from(dir).join(name);
            if candidate.is_file() {
                // Check executable bit.
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(meta) = candidate.metadata() {
                        if meta.permissions().mode() & 0o111 != 0 {
                            return Some(candidate.display().to_string());
                        }
                    }
                }
                #[cfg(not(unix))]
                {
                    return Some(candidate.display().to_string());
                }
            }
        }
    }
    None
}

// -------------------------------------------------------------------
// Subprocess lifter
// -------------------------------------------------------------------

/// Spawn a kit lifter as a subprocess, send a JSON-RPC `parse` request,
/// and parse the response into `(LinkerContract[], LinkerCallEdge[])`.
///
/// Protocol:
///   1. Send `{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}`.
///   2. Read and discard the initialize response.
///   3. Send `{"jsonrpc":"2.0","id":2,"method":"parse","params":{"path":file,"source":source}}`.
///   4. Read one response line.
///   5. Parse `result.declarations` (array of IR declarations) into `LinkerContract`.
///   6. Parse `result.callEdges` (array of call-edge mementos) into `LinkerCallEdge`.
///   7. Send shutdown and close stdin.
///
/// Field name mapping (kit → LinkerCallEdge):
///   - `sourceContractCid` → `source_contract_cid`
///   - `targetContractCid` → `target_contract_cid`
///   - `targetSymbol`      → `target_symbol`
///   - `callSiteLocus`     → `call_site_locus_json`
///   - `evidenceTerm`      → `evidence_term_json`
///
/// CID strategy for declarations:
///   The daemon computes a stable `contract_cid` from the declaration's
///   `{name, outBinding?, pre?, post?, inv?}` fields using BLAKE3-512(JCS(...)).
///   This may differ from the CID a kit computed into its own call-edge mementos
///   (each kit uses its own serialisation formula for cross-kit CID computation).
///   Cross-kit linker resolution works via `targetSymbol` lookup rather than CID
///   matching, so this discrepancy does not break bridge derivation for targets.
///   Source-contract post-condition lookup will return None for cross-kit sources
///   (discharge checking is skipped, not errored) — acceptable MVP behaviour.
///
/// Missing fields:
///   - `callEdges` absent from response: treated as empty (zig case).
///   - `declarations` absent: treated as empty.
fn spawn_kit_lifter(
    binary: &str,
    args: &[&str],
    file: &str,
    source: &str,
    kit_label: &str,
) -> Result<(Vec<LinkerContract>, Vec<LinkerCallEdge>), LiftError> {
    let mut child = Command::new(binary)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            LiftError::LifterUnavailable(format!("spawn {binary}: {e}"))
        })?;

    let mut stdin = child.stdin.take().ok_or_else(|| {
        LiftError::LifterUnavailable("no stdin handle".to_string())
    })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        LiftError::LifterUnavailable("no stdout handle".to_string())
    })?;
    let mut reader = BufReader::new(stdout);

    // 1. Send initialize.
    let init_req = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}
    });
    let init_line = serde_json::to_string(&init_req).unwrap() + "\n";
    stdin.write_all(init_line.as_bytes()).map_err(|e| {
        LiftError::LifterUnavailable(format!("write initialize to {binary}: {e}"))
    })?;

    // 2. Read initialize response (discard).
    let mut init_resp = String::new();
    reader.read_line(&mut init_resp).map_err(|e| {
        LiftError::LifterUnavailable(format!("read initialize from {binary}: {e}"))
    })?;

    // 3. Send parse request.
    let parse_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "parse",
        "params": { "path": file, "source": source }
    });
    let parse_line = serde_json::to_string(&parse_req).unwrap() + "\n";
    stdin.write_all(parse_line.as_bytes()).map_err(|e| {
        LiftError::LifterUnavailable(format!("write parse to {binary}: {e}"))
    })?;

    // 4. Read parse response.
    let mut resp_line = String::new();
    reader.read_line(&mut resp_line).map_err(|e| {
        LiftError::LifterUnavailable(format!("read parse from {binary}: {e}"))
    })?;

    // Send shutdown and drop stdin to let the subprocess exit cleanly.
    let shutdown_req = serde_json::json!({
        "jsonrpc": "2.0", "id": 3, "method": "shutdown", "params": {}
    });
    let shutdown_line = serde_json::to_string(&shutdown_req).unwrap() + "\n";
    let _ = stdin.write_all(shutdown_line.as_bytes());
    drop(stdin);
    let _ = child.wait();

    // 5. Parse response.
    let resp: Json = serde_json::from_str(resp_line.trim()).map_err(|e| {
        LiftError::LifterUnavailable(format!(
            "parse JSON from {binary}: {e}; raw: {resp_line}"
        ))
    })?;

    if let Some(err) = resp.get("error") {
        return Err(LiftError::LifterUnavailable(format!(
            "{binary} returned RPC error: {err}"
        )));
    }

    let result = resp.get("result").ok_or_else(|| {
        LiftError::LifterUnavailable(format!(
            "{binary} response missing 'result' field"
        ))
    })?;

    // 6. Extract declarations array.
    let decls_json = extract_array_field(result, "declarations");
    let contracts = decls_json
        .iter()
        .filter_map(|decl| parse_declaration_to_contract(decl, kit_label))
        .collect::<Vec<_>>();

    // 7. Extract callEdges array (may be absent for some kits, e.g. zig).
    let edges_json = extract_array_field(result, "callEdges");
    let call_edges = edges_json
        .iter()
        .filter_map(parse_call_edge)
        .collect::<Vec<_>>();

    Ok((contracts, call_edges))
}

/// Extract a JSON array from a field in a result object.
///
/// Handles the case where the field is absent (returns empty vec) or where the
/// field is a JSON-encoded string (a shape used by some kit lifters) — in that
/// case the string is re-parsed as JSON.
fn extract_array_field<'a>(result: &'a Json, field: &str) -> Vec<Json> {
    match result.get(field) {
        None => vec![],
        Some(Json::Array(arr)) => arr.clone(),
        Some(Json::String(s)) => {
            // Some kit lifters (e.g. python) encode the array as a JSON string.
            // Re-parse it.
            match serde_json::from_str::<Json>(s) {
                Ok(Json::Array(arr)) => arr,
                _ => vec![],
            }
        }
        _ => vec![],
    }
}

/// Parse a single IR declaration JSON object into a `LinkerContract`.
///
/// Only `kind: "contract"` declarations are mapped; bridges and call-edges
/// in the declarations array are skipped.
///
/// CID is computed as BLAKE3-512(JCS({name, outBinding?, pre?, post?, inv?})).
fn parse_declaration_to_contract(decl: &Json, kit_label: &str) -> Option<LinkerContract> {
    if decl.get("kind").and_then(|k| k.as_str()) != Some("contract") {
        return None;
    }
    let name = decl.get("name").and_then(|n| n.as_str())?.to_string();
    if name.is_empty() {
        return None;
    }

    let pre_json = decl.get("pre").cloned();
    let post_json = decl.get("post").cloned();
    let inv_json = decl.get("inv").cloned();
    let out_binding = decl
        .get("outBinding")
        .and_then(|v| v.as_str())
        .unwrap_or("out")
        .to_string();

    // Compute a stable content CID: BLAKE3-512(JCS({name, outBinding, pre?, post?, inv?})).
    let contract_cid = compute_contract_cid_from_json(
        &name,
        &out_binding,
        pre_json.as_ref(),
        post_json.as_ref(),
        inv_json.as_ref(),
    );

    Some(LinkerContract {
        name,
        kit: kit_label.to_string(),
        contract_cid,
        pre_json,
        post_json,
    })
}

/// Compute a contract content CID from declaration fields.
///
/// Algorithm: BLAKE3-512(JCS({name, outBinding, pre?, post?, inv?}))
/// Matches the rust lifter's `contract_cid()` formula so that cross-kit
/// contracts with identical logical content produce identical CIDs.
fn compute_contract_cid_from_json(
    name: &str,
    out_binding: &str,
    pre: Option<&Json>,
    post: Option<&Json>,
    inv: Option<&Json>,
) -> String {
    use provekit_canonicalizer::{encode_jcs, Value};

    // Build the canonical object with required + optional fields.
    let mut kvs: Vec<(String, std::sync::Arc<Value>)> = Vec::new();
    kvs.push(("name".into(), Value::string(name.to_string())));
    kvs.push(("outBinding".into(), Value::string(out_binding.to_string())));
    if let Some(p) = pre {
        if let Some(v) = json_to_canon_value(p) {
            kvs.push(("pre".into(), v));
        }
    }
    if let Some(p) = post {
        if let Some(v) = json_to_canon_value(p) {
            kvs.push(("post".into(), v));
        }
    }
    if let Some(i) = inv {
        if let Some(v) = json_to_canon_value(i) {
            kvs.push(("inv".into(), v));
        }
    }
    let obj = std::sync::Arc::new(Value::Object(kvs));
    let jcs = encode_jcs(&obj);
    blake3_512_of(jcs.as_bytes())
}

/// Parse a single call-edge JSON object from a kit lifter's response.
///
/// JSON field names (camelCase) per the IR spec:
///   `sourceContractCid`, `targetContractCid`, `targetSymbol`,
///   `callSiteLocus`, `evidenceTerm`
fn parse_call_edge(edge: &Json) -> Option<LinkerCallEdge> {
    let source_cid = edge
        .get("sourceContractCid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if source_cid.is_empty() {
        return None;
    }

    let target_cid = edge
        .get("targetContractCid")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let target_symbol = edge
        .get("targetSymbol")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let locus = edge
        .get("callSiteLocus")
        .cloned()
        .unwrap_or(Json::Null);
    let evidence = edge
        .get("evidenceTerm")
        .cloned()
        .unwrap_or(Json::Null);

    Some(LinkerCallEdge {
        source_contract_cid: source_cid,
        target_contract_cid: target_cid,
        target_symbol,
        call_site_locus_json: locus,
        evidence_term_json: evidence,
    })
}

/// Convert a `serde_json::Value` to a `provekit_canonicalizer::Value`.
///
/// Used when computing content CIDs from parsed declaration JSON.
fn json_to_canon_value(v: &Json) -> Option<std::sync::Arc<provekit_canonicalizer::Value>> {
    use provekit_canonicalizer::Value;
    let cv = match v {
        Json::Null => Value::Null,
        Json::Bool(b) => Value::Bool(*b),
        Json::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Integer(i)
            } else {
                Value::String(n.to_string())
            }
        }
        Json::String(s) => Value::String(s.clone()),
        Json::Array(arr) => {
            let items: Vec<std::sync::Arc<Value>> = arr
                .iter()
                .filter_map(json_to_canon_value)
                .collect();
            Value::Array(items)
        }
        Json::Object(map) => {
            let kvs: Vec<(String, std::sync::Arc<Value>)> = map
                .iter()
                .filter_map(|(k, v)| json_to_canon_value(v).map(|cv| (k.clone(), cv)))
                .collect();
            Value::Object(kvs)
        }
    };
    Some(std::sync::Arc::new(cv))
}

// -------------------------------------------------------------------
// Rust in-process lifter
// -------------------------------------------------------------------

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
