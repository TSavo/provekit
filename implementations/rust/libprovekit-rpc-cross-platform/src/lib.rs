// SPDX-License-Identifier: Apache-2.0
//
//! # libprovekit-rpc
//!
//! JSON-RPC 2.0 / NDJSON-over-stdio server for ProvekIt's lift-plugin
//! protocol (`pep/1.7.0`). **Cleanroom implementation**: this crate
//! has zero `provekit-*` dependencies. The only external crates it
//! pulls in are [`blake3`] (for the BLAKE3-512 content-address hash)
//! and [`serde_json`] (for the on-wire JSON values). The JSON
//! Canonicalization Scheme (RFC 8785) used to canonicalize mementos
//! before hashing is implemented inline in this file; see [`encode_jcs`].
//!
//! ## Layering / responsibilities
//!
//! Adapter binaries (e.g. proptest, kani, creusot, rust-tests, ...)
//! depend on this crate and call [`run_server`] from `main`, passing
//! an implementation of [`AdapterLifter`]. **The adapter owns**:
//!
//! * source enumeration (which files / directories to walk),
//! * parsing (`syn`, `tree-sitter`, regex, whatever),
//! * conversion of its parsed results into canonical-JSON contract
//!   mementos of the shape:
//!
//!   ```json
//!   {
//!     "kind": "contract",
//!     "name": "<adapter-supplied-original-name>",
//!     "outBinding": "out",
//!     "inv":  <ir-json>,
//!     "pre":  <ir-json>,   // optional
//!     "post": <ir-json>    // optional
//!   }
//!   ```
//!
//! **The library owns**:
//!
//! * JSON-RPC 2.0 / NDJSON framing,
//! * the `initialize` / `lift` / `shutdown` method set,
//! * RFC-8785 JCS canonicalization,
//! * BLAKE3-512 content-addressed naming
//!   (`<original-name>#blake3-512:<128-hex>`),
//! * dedup by content-addressed name,
//! * the `kind: "ir-document"` response envelope.
//!
//! This split makes the library vendorable -- any future ProvekIt
//! library that wants the same JSON-RPC contract can depend on this
//! crate alone and still talk to the rest of the substrate, because
//! every byte that crosses the wire is hashable, byte-deterministic
//! JSON.
//!
//! ## Protocol surface
//!
//! * `initialize` -> [`initialize_result`] built from
//!   [`AdapterLifter::name`] / [`AdapterLifter::surface`].
//! * `lift` with `options.emit == "ir-document"` (the default) ->
//!   [`build_ir_document`] over the adapter's [`LiftResult`].
//! * `shutdown` -> `null` result; the read loop exits cleanly.
//!
//! Unknown methods produce JSON-RPC error `-32601`; malformed input
//! produces `-32700`; missing required parameters produce `-32602`;
//! any other failure produces `-32603`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

// ---------------------------------------------------------------------
// Public protocol constants
// ---------------------------------------------------------------------

/// Protocol-version token advertised in the `initialize` response, per
/// `protocol/specs/2026-04-30-lift-plugin-protocol.md` as renamed under
/// `pep/1.7.0`.
pub const PROTOCOL_VERSION: &str = "pep/1.7.0";

/// IR shape version the server emits in `initialize`.
pub const IR_VERSION: &str = "v1.1.0";

/// Plugin version reported in the `initialize` response. Adapters get
/// to advertise a name and surface; the wire version is owned by this
/// library so all adapters that depend on it report the same value.
pub const PLUGIN_VERSION: &str = "0.1.0";

/// String prefix used by the BLAKE3-512 self-identifying hash form.
/// The protocol cut is scorched earth: `blake3-512` is the only hash
/// function permitted, always 512 bits wide, no truncation.
pub const BLAKE3_512_PREFIX: &str = "blake3-512:";

// ---------------------------------------------------------------------
// AdapterLifter trait + result type
// ---------------------------------------------------------------------

/// The result of one `lift` call delegated to the adapter. The adapter
/// returns canonical-JSON mementos and diagnostics; the library does
/// the content-addressing, dedup, and envelope construction.
///
/// Each entry in `mementos` should be a JSON object roughly shaped like:
///
/// ```json
/// {
///   "kind": "contract",
///   "name": "<adapter-original-name>",
///   "outBinding": "out",
///   "inv":  { ... ir-json ... },
///   "pre":  { ... ir-json ... },   // optional
///   "post": { ... ir-json ... }    // optional
/// }
/// ```
///
/// Anything else in the object is preserved verbatim in the output IR
/// document but does *not* contribute to the content-address (only
/// `inv` / `pre` / `post` do). The library rewrites `name` to
/// `<original-name>#<content-cid>` on the way out.
///
/// `diagnostics` follow the protocol's free-form diagnostic shape
/// (e.g. `{"kind": "parse-error", "path": "...", "detail": "..."}`);
/// the library passes them through to the response untouched.
pub struct LiftResult {
    /// Contract mementos discovered by the adapter, before dedup.
    pub mementos: Vec<Value>,
    /// Diagnostics emitted during the lift, surfaced to the client.
    pub diagnostics: Vec<Value>,
}

/// A lift adapter. The adapter owns source enumeration, parsing, and
/// memento construction; the library owns the protocol mechanics.
pub trait AdapterLifter {
    /// Walk the supplied source paths under `workspace_root`, lift
    /// whatever this adapter understands, and return the
    /// canonical-JSON mementos plus diagnostics.
    ///
    /// `workspace_root` is the absolute path the JSON-RPC client
    /// supplied (no normalization by the library). `source_paths` are
    /// adapter-resolution-relative paths (typically POSIX-style and
    /// joined with `workspace_root`). The adapter is free to ignore
    /// either argument -- some adapters (e.g. environment-driven
    /// linters) may not need them.
    fn lift(&self, workspace_root: &Path, source_paths: &[String]) -> LiftResult;

    /// Human-readable authoring-surface name advertised in
    /// `initialize.result.capabilities.authoring_surfaces`. The client
    /// selects a plugin by matching `[authoring] surface = ...` from
    /// `.provekit/config.toml` against this string.
    fn surface(&self) -> &str;

    /// Plugin name advertised in `initialize.result.name`.
    fn name(&self) -> &str;
}

// ---------------------------------------------------------------------
// JSON-RPC server loop
// ---------------------------------------------------------------------

// ---------------------------------------------------------------------
// stdio + JSON parse/emit BOUNDARIES — defer to per-target shims.
// ---------------------------------------------------------------------

/// `concept:stdio-read-line` — std::io's sugar. Reads one line from
/// stdin, returning `None` at EOF.
#[provekit::boundary(
    concept = "concept:stdio-read-line",
    library = "provekit-shim-stdio-rust",
    boundary_contract = "boundary:stdio-line-stream",
    loss = [],
)]
pub fn stdin_read_line() -> Option<String> {
    unimplemented!("materialize-fillable boundary; per-target stdio shim provides the sugar realization")
}

/// `concept:stdio-write-line` — std::io's sugar. Writes one line +
/// newline to stdout.
#[provekit::boundary(
    concept = "concept:stdio-write-line",
    library = "provekit-shim-stdio-rust",
    boundary_contract = "boundary:stdio-line-stream",
    loss = [],
)]
pub fn stdout_write_line(line: &str) {
    unimplemented!("materialize-fillable boundary; per-target stdio shim provides the sugar realization")
}

/// `concept:stderr-write-line` — std::io's sugar. Writes one line +
/// newline to stderr (used for human-readable progress messages).
#[provekit::boundary(
    concept = "concept:stderr-write-line",
    library = "provekit-shim-stdio-rust",
    boundary_contract = "boundary:stdio-line-stream",
    loss = [],
)]
pub fn stderr_write_line(line: &str) {
    unimplemented!("materialize-fillable boundary; per-target stdio shim provides the sugar realization")
}

/// `concept:json-parse` — serde_json's sugar. Parses one canonical
/// JSON value from a string.
#[provekit::boundary(
    concept = "concept:json-parse",
    library = "provekit-shim-serde-json-rust",
    boundary_contract = "boundary:rfc8259-json",
    loss = [],
)]
pub fn json_parse(s: &str) -> Result<Value, String> {
    unimplemented!("materialize-fillable boundary; per-target JSON shim provides the sugar realization")
}

/// `concept:json-serialize` — serde_json's sugar. Serializes a JSON
/// value to a string (non-canonical; for use over the wire, NOT for
/// content-addressing — that uses encode_jcs).
#[provekit::boundary(
    concept = "concept:json-serialize",
    library = "provekit-shim-serde-json-rust",
    boundary_contract = "boundary:rfc8259-json",
    loss = [],
)]
pub fn json_serialize(v: &Value) -> Result<String, String> {
    unimplemented!("materialize-fillable boundary; per-target JSON shim provides the sugar realization")
}

/// `concept:jsonrpc-ndjson-server-loop` — our sugar. NDJSON dispatch
/// loop. Read line → parse → dispatch → write response. Pure
/// composition of boundary primitives; same shape in every language.
#[provekit::sugar(
    concept = "concept:jsonrpc-ndjson-server-loop",
    library = "libprovekit-rpc-cross-platform",
    loss = [],
)]
pub fn run_server<A: AdapterLifter>(adapter: A) {
    stderr_write_line(&format!(
        "{} listening on stdio (JSON-RPC 2.0, NDJSON)",
        adapter.name()
    ));
    while let Some(line) = stdin_read_line() {
        if line.trim().is_empty() {
            continue;
        }
        let (response, stop) = handle_line(&line, &adapter);
        let response_str = json_serialize(&response).unwrap_or_else(|e| {
            format!(
                "{{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{{\"code\":-32603,\"message\":\"{}\"}}}}",
                e
            )
        });
        stdout_write_line(&response_str);
        if stop {
            break;
        }
    }
}

/// `concept:jsonrpc-request-dispatch` — our sugar. Parses one
/// JSON-RPC line, dispatches by method, returns the response value
/// plus a `should_stop` flag (set by `shutdown`). Pure composition;
/// JSON parse boundary materializes per target.
#[provekit::sugar(
    concept = "concept:jsonrpc-request-dispatch",
    library = "libprovekit-rpc-cross-platform",
    loss = [],
)]
/// Parse one JSON-RPC request line, dispatch, and produce the JSON
/// response value plus a "should stop" flag (set by `shutdown`).
fn handle_line<A: AdapterLifter>(line: &str, adapter: &A) -> (Value, bool) {
    let req: Value = match json_parse(line) {
        Ok(v) => v,
        Err(e) => {
            return (
                error_response(Value::Null, -32700, format!("parse error: {}", e)),
                false,
            );
        }
    };
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = req.get("params").cloned().unwrap_or(Value::Null);

    match method {
        "initialize" => (ok_response(id, initialize_result(adapter)), false),
        "lift" => match lift(&params, adapter) {
            Ok(v) => (ok_response(id, v), false),
            Err(LiftError::InvalidParams(msg)) => (error_response(id, -32602, msg), false),
            Err(LiftError::Internal(msg)) => (error_response(id, -32603, msg), false),
        },
        "shutdown" => (ok_response(id, Value::Null), true),
        "" => (
            error_response(id, -32600, "missing `method` field".into()),
            false,
        ),
        other => (
            error_response(id, -32601, format!("unknown method: {}", other)),
            false,
        ),
    }
}

/// `concept:jsonrpc-initialize-response` — our sugar. Builds the
/// `initialize` result advertising protocol version, IR version, and
/// the adapter's authoring surface. Same shape in every language.
#[provekit::sugar(
    concept = "concept:jsonrpc-initialize-response",
    library = "libprovekit-rpc-cross-platform",
    loss = [],
)]
pub fn initialize_result<A: AdapterLifter>(adapter: &A) -> Value {
    json!({
        "name": adapter.name(),
        "version": PLUGIN_VERSION,
        "protocol_version": PROTOCOL_VERSION,
        "capabilities": {
            "authoring_surfaces": [adapter.surface()],
            "ir_version": IR_VERSION,
            "emits_signed_mementos": false,
        }
    })
}

/// Internal classification of failures during `lift`.
enum LiftError {
    InvalidParams(String),
    Internal(String),
}

/// `concept:lift-method-handler` — our sugar. Reads workspace_root +
/// source_paths from params, validates emit mode, delegates the
/// actual lift work to the adapter via the trait, then composes the
/// result through content-addressing.
#[provekit::sugar(
    concept = "concept:lift-method-handler",
    library = "libprovekit-rpc-cross-platform",
    loss = [],
)]
fn lift<A: AdapterLifter>(params: &Value, adapter: &A) -> Result<Value, LiftError> {
    let workspace_root = params
        .get("workspace_root")
        .and_then(Value::as_str)
        .ok_or_else(|| LiftError::InvalidParams("missing `workspace_root`".into()))?;
    let source_paths_raw = params
        .get("source_paths")
        .and_then(Value::as_array)
        .ok_or_else(|| LiftError::InvalidParams("missing `source_paths`".into()))?;
    let source_paths: Vec<String> = source_paths_raw
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();

    let options = params.get("options").cloned().unwrap_or(Value::Null);
    let emit = options
        .get("emit")
        .and_then(Value::as_str)
        .unwrap_or("ir-document");
    if emit != "ir-document" {
        return Err(LiftError::Internal(format!(
            "emit mode `{}` not implemented (only `ir-document` is supported in this version)",
            emit
        )));
    }

    let root = PathBuf::from(workspace_root);
    Ok(build_ir_document(&root, &source_paths, adapter))
}

/// `concept:ir-document-assembly` — our sugar. Invokes the adapter,
/// content-addresses each returned memento, dedups by name, assembles
/// the `kind: "ir-document"` envelope. Composition of adapter call +
/// content-addressing primitive + dedup. Same in every language.
#[provekit::sugar(
    concept = "concept:ir-document-assembly",
    library = "libprovekit-rpc-cross-platform",
    loss = [],
)]
pub fn build_ir_document<A: AdapterLifter>(
    workspace_root: &Path,
    source_paths: &[String],
    adapter: &A,
) -> Value {
    let LiftResult {
        mementos,
        diagnostics,
    } = adapter.lift(workspace_root, source_paths);

    let mut ir_entries: Vec<Value> = Vec::new();
    let mut seen_names: BTreeSet<String> = BTreeSet::new();
    for mut memento in mementos {
        let original_name = memento
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let addressed_name = content_addressed_name(&original_name, &memento);
        if !seen_names.insert(addressed_name.clone()) {
            continue;
        }
        if let Value::Object(map) = &mut memento {
            map.insert("name".to_string(), Value::String(addressed_name));
        }
        ir_entries.push(memento);
    }

    json!({
        "kind": "ir-document",
        "ir": ir_entries,
        "diagnostics": diagnostics,
    })
}

// ---------------------------------------------------------------------
// Content-addressed naming
// ---------------------------------------------------------------------

/// Compute the content-addressed name for a memento:
///
/// 1. JCS-encode each present slot (`inv`, `pre`, `post`).
/// 2. BLAKE3-512 each JCS-encoded slot to its self-identifying CID
///    string. Absent slots contribute the empty string.
/// 3. Concatenate `"<inv_cid>|<pre_cid>|<post_cid>"` and hash the
///    bytes once more to produce a single content CID.
/// 4. Return `format!("{}#{}", original_name, content_cid)`.
///
/// "Absent" means either the key is missing OR its value is
/// `Value::Null` -- the library treats explicit-null and omitted
/// identically, matching the existing IR-emission contract that drops
/// empty slots rather than serializing them as `null`.
#[provekit::sugar(
    concept = "concept:content-addressed-memento-name",
    library = "libprovekit-rpc-cross-platform",
    loss = [],
)]
fn content_addressed_name(original_name: &str, memento: &Value) -> String {
    let inv_cid = slot_cid(memento, "inv");
    let pre_cid = slot_cid(memento, "pre");
    let post_cid = slot_cid(memento, "post");
    let composed = format!("{}|{}|{}", inv_cid, pre_cid, post_cid);
    let content_cid = blake3_512_cid(composed.as_bytes());
    format!("{}#{}", original_name, content_cid)
}

/// `concept:formula-slot-content-cid` — our sugar. Composes JCS
/// canonicalization (boundary) with BLAKE3 hex-cid (our sugar) to
/// produce a slot's content CID; returns empty string for absent
/// slots. The "absent = explicit null" treatment is part of this
/// sugar's contract.
#[provekit::sugar(
    concept = "concept:formula-slot-content-cid",
    library = "libprovekit-rpc-cross-platform",
    loss = [],
)]
fn slot_cid(memento: &Value, key: &str) -> String {
    match memento.get(key) {
        Some(v) if !v.is_null() => blake3_512_cid(encode_jcs(v).as_bytes()),
        _ => String::new(),
    }
}

// ---------------------------------------------------------------------
// BLAKE3-512 — `concept:blake3-512-of` is BLAKE3's sugar (blake3 library
// realizes it). We declare a BOUNDARY here pointing at our blake3 shim;
// the body is `unimplemented!()` because we're not the realizer.
// Materialize per target substitutes the sister shim's @sugar body.
// ---------------------------------------------------------------------

/// `concept:blake3-512-of` — bytes to 64-byte BLAKE3-XOF digest.
/// BLAKE3's sugar. We declare the boundary; the per-target blake3 shim
/// owns the realization.
#[provekit::boundary(
    concept = "concept:blake3-512-of",
    library = "provekit-shim-blake3-rust",
    boundary_contract = "boundary:blake3-512",
    loss = [],
)]
pub fn blake3_512_of(bytes: &[u8]) -> [u8; 64] {
    unimplemented!("materialize-fillable boundary; per-target blake3 shim provides the sugar realization")
}

// ---------------------------------------------------------------------
// OUR sugar: `concept:blake3-512-self-identifying-cid`. We compose
// BLAKE3's sugar (the raw 64-byte digest) with the substrate's
// self-identifying CID format ("blake3-512:" + lowercase hex). This
// composition is the same in every language — pure formatting logic
// that materializes line-by-line. Owned by this crate.
// ---------------------------------------------------------------------

/// `concept:blake3-512-self-identifying-cid` — our sugar. Calls
/// BLAKE3's `blake3_512_of` (which materialize substitutes per-target)
/// and formats the result as the substrate's CID string. Used as the
/// content-address primitive for every memento.
#[provekit::sugar(
    concept = "concept:blake3-512-self-identifying-cid",
    library = "libprovekit-rpc-cross-platform",
    loss = [],
)]
pub fn blake3_512_cid(bytes: &[u8]) -> String {
    let raw = blake3_512_of(bytes);
    let mut s = String::with_capacity("blake3-512:".len() + 128);
    s.push_str("blake3-512:");
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for &b in &raw {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0F) as usize] as char);
    }
    s
}

// ---------------------------------------------------------------------
// JCS encoder (RFC 8785), inlined for `serde_json::Value`
// ---------------------------------------------------------------------

/// `concept:rfc8785-jcs-encode` — RFC 8785 (JSON Canonicalization
/// Scheme) deterministic serialization of a JSON value. RFC 8785's
/// sugar. We declare the boundary; the per-target JCS shim
/// (provekit-shim-rfc8785-jcs-rust, sister shims in TS/Python/...)
/// owns the realization.
#[provekit::boundary(
    concept = "concept:rfc8785-jcs-encode",
    library = "provekit-shim-rfc8785-jcs-rust",
    boundary_contract = "boundary:rfc8785-canonical-json",
    loss = [],
)]
pub fn encode_jcs(v: &Value) -> String {
    unimplemented!("materialize-fillable boundary; per-target JCS shim provides the sugar realization")
}

/// `concept:rfc8785-jcs-encode-value` — interior helper of the JCS
/// algorithm, recursive over `serde_json::Value`. The JCS shim is
/// authoritative for the implementation. We declare the boundary in
/// cross-platform because the materialized `encode_jcs` body calls
/// `encode_value(v, &mut out)`; without a corresponding @boundary in
/// cross-platform, the substituted body has an unresolved name.
#[provekit::boundary(
    concept = "concept:rfc8785-jcs-encode-value",
    library = "provekit-shim-rfc8785-jcs-rust",
    boundary_contract = "boundary:rfc8785-canonical-json",
    loss = [],
)]
fn encode_value(v: &Value, out: &mut String) {
    unimplemented!("materialize-fillable boundary; per-target JCS shim provides the sugar realization")
}

/// `concept:rfc8785-jcs-encode-string` — interior helper of the JCS
/// algorithm, escapes a string per RFC 8785's rules. Same boundary
/// rationale as encode_value: the JCS shim's encode_value body calls
/// encode_string, so cross-platform must declare it as a @boundary too.
#[provekit::boundary(
    concept = "concept:rfc8785-jcs-encode-string",
    library = "provekit-shim-rfc8785-jcs-rust",
    boundary_contract = "boundary:rfc8785-canonical-json",
    loss = [],
)]
fn encode_string(s: &str, out: &mut String) {
    unimplemented!("materialize-fillable boundary; per-target JCS shim provides the sugar realization")
}

// ---------------------------------------------------------------------
// JSON-RPC response helpers
// ---------------------------------------------------------------------

/// `concept:jsonrpc-success-response` — our sugar.
#[provekit::sugar(
    concept = "concept:jsonrpc-success-response",
    library = "libprovekit-rpc-cross-platform",
    loss = [],
)]
fn ok_response(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

/// `concept:jsonrpc-error-response` — our sugar.
#[provekit::sugar(
    concept = "concept:jsonrpc-error-response",
    library = "libprovekit-rpc-cross-platform",
    loss = [],
)]
fn error_response(id: Value, code: i64, message: String) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------
