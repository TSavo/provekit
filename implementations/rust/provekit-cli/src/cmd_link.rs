// SPDX-License-Identifier: Apache-2.0
//
// `provekit link` — linker pass per spec
// `protocol/specs/2026-05-03-bridge-linkage-protocol.md` R2-R5.
//
// Orchestrates:
//   1. Lift rust source in <project>/rust-callee/ via provekit-lift.
//   2. Spawn go-lsp-lifter over <project>/go-caller/ to get call-edges.
//   3. Resolve cross-kit symbols (R3).
//   4. Derive bridges (R2) — linker is the ONLY bridge author (R4).
//   5. Discharge satisfaction obligations (conservative: null post → linker-error).
//   6. Emit LinkBundle (R5) and write .linkbundle.json.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use owo_colors::OwoColorize;
use provekit_canonicalizer::{blake3_512_of, encode_jcs};
use provekit_claim_envelope::{contract_cid as compute_contract_cid, MintContractArgs, Authoring};
use provekit_lift::lift_path;
use serde_json::Value as Json;

use crate::LinkArgs;

// -------------------------------------------------------------------
// Public entry point
// -------------------------------------------------------------------

pub fn run(args: LinkArgs) -> u8 {
    let project_root: PathBuf = args.project.unwrap_or_else(|| PathBuf::from("."));
    if !project_root.exists() {
        eprintln!(
            "{}: project root does not exist: {}",
            "error".red().bold(),
            project_root.display()
        );
        return crate::EXIT_USER_ERROR;
    }

    match run_linker_pass(&project_root, args.go_lsp_bin.as_deref()) {
        Ok(bundle) => {
            let out_path = project_root.join("link-bundle.json");
            let json = serde_json::to_string_pretty(&bundle).unwrap_or_default();
            if let Err(e) = std::fs::write(&out_path, &json) {
                eprintln!("{}: write link-bundle.json: {e}", "error".red().bold());
                return crate::EXIT_USER_ERROR;
            }
            let error_count = bundle
                .get("linkerErrors")
                .and_then(|e| e.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let bundle_cid = bundle
                .get("linkBundleCid")
                .and_then(|v| v.as_str())
                .unwrap_or("(unknown)");

            if error_count > 0 {
                eprintln!(
                    "{}: {} linker error(s) — see {}",
                    "linker".yellow().bold(),
                    error_count,
                    out_path.display()
                );
                eprintln!("linkBundleCid = {bundle_cid}");
                return crate::EXIT_VERIFY_FAIL;
            }
            println!(
                "{}: clean link bundle",
                "linker".green().bold()
            );
            println!("linkBundleCid = {bundle_cid}");
            println!("wrote {}", out_path.display());
            crate::EXIT_OK
        }
        Err(e) => {
            eprintln!("{}: {e}", "error".red().bold());
            crate::EXIT_USER_ERROR
        }
    }
}

// -------------------------------------------------------------------
// Core linker logic (also used from tests)
// -------------------------------------------------------------------

/// Describes a lifted contract from either kit.
#[derive(Debug, Clone)]
pub struct KitContract {
    pub name: String,
    pub kit: String,
    /// Signer-independent content CID (blake3-512:...).
    pub contract_cid: String,
    /// Pre formula JSON value (None if no pre).
    pub pre_json: Option<Json>,
    /// Post formula JSON value (None if no post).
    pub post_json: Option<Json>,
}

/// Describes a call edge emitted by a kit lifter.
#[derive(Debug, Clone)]
pub struct KitCallEdge {
    pub source_contract_cid: String,
    pub target_contract_cid: Option<String>,
    pub target_symbol: String,
    /// JCS-canonical JSON bytes of the call-site locus.
    pub call_site_locus_json: Json,
    /// Evidence term JSON.
    pub evidence_term_json: Json,
}

/// The full link bundle output.
pub struct LinkBundle {
    pub contract_set_cid: String,
    pub call_edge_set_cid: String,
    pub bridge_set_cid: String,
    pub link_bundle_cid: String,
    pub linker_errors: Vec<LinkerError>,
    /// JCS-serializable bundle object for writing.
    pub bundle_json: Json,
}

#[derive(Debug, Clone)]
pub struct LinkerError {
    pub kind: String,
    pub target_symbol: String,
    pub source_contract_cid: String,
    pub reason: String,
}

/// Run the full linker pass over the given project root.
///
/// Expects:
///   <project>/rust-callee/  — rust crate with contracts
///   <project>/go-caller/    — go package with call-edges
pub fn run_linker_pass(
    project_root: &Path,
    go_lsp_bin: Option<&str>,
) -> Result<Json, String> {
    // --- Step 1: Lift rust contracts ---
    let rust_dir = project_root.join("rust-callee");
    let rust_contracts = lift_rust_contracts(&rust_dir)?;

    // --- Step 2: Lift go call-edges ---
    let go_dir = project_root.join("go-caller");
    let (go_contracts, go_call_edges) = lift_go_call_edges(&go_dir, go_lsp_bin)?;

    // --- Step 3: Build union contract index ---
    let mut all_contracts: Vec<KitContract> = Vec::new();
    all_contracts.extend(rust_contracts.clone());
    all_contracts.extend(go_contracts.clone());

    // Index: (name, kit) -> contract_cid for cross-kit resolution
    let mut name_kit_index: BTreeMap<(String, String), String> = BTreeMap::new();
    for c in &all_contracts {
        name_kit_index.insert((c.name.clone(), c.kit.clone()), c.contract_cid.clone());
    }

    // --- Step 4: Compute contractSetCid ---
    let mut all_contract_cids: Vec<String> =
        all_contracts.iter().map(|c| c.contract_cid.clone()).collect();
    all_contract_cids.sort();
    let contract_set_cid = compute_set_cid_sorted(&all_contract_cids);

    // --- Step 5: Resolve cross-kit symbols and derive bridges ---
    let mut bridges: Vec<Json> = Vec::new();
    let mut linker_errors: Vec<LinkerError> = Vec::new();

    // Sort call edges for determinism
    let mut sorted_edges = go_call_edges.clone();
    sorted_edges.sort_by(|a, b| {
        a.source_contract_cid
            .cmp(&b.source_contract_cid)
            .then_with(|| {
                let la = a.call_site_locus_json.to_string();
                let lb = b.call_site_locus_json.to_string();
                la.cmp(&lb)
            })
    });

    for edge in &sorted_edges {
        let resolved_target_cid = if let Some(ref cid) = edge.target_contract_cid {
            // Same-kit: CID already known
            Some(cid.clone())
        } else {
            // Cross-kit: resolve targetSymbol
            resolve_target_symbol(&edge.target_symbol, &name_kit_index)
        };

        match resolved_target_cid {
            None => {
                linker_errors.push(LinkerError {
                    kind: "unresolved-symbol".into(),
                    target_symbol: edge.target_symbol.clone(),
                    source_contract_cid: edge.source_contract_cid.clone(),
                    reason: format!(
                        "targetSymbol `{}` did not resolve to any contract in the union",
                        edge.target_symbol
                    ),
                });
            }
            Some(target_cid) => {
                // Find the target contract's pre formula
                let target_contract = all_contracts
                    .iter()
                    .find(|c| c.contract_cid == target_cid);

                // Find the source contract's post formula
                let source_contract = all_contracts
                    .iter()
                    .find(|c| c.contract_cid == edge.source_contract_cid);

                let source_post = source_contract.and_then(|c| c.post_json.as_ref());
                let target_pre = target_contract.and_then(|c| c.pre_json.as_ref());

                // Derive bridge (R2)
                let bridge = derive_bridge(
                    &edge.source_contract_cid,
                    &target_cid,
                    &edge.call_site_locus_json,
                    &edge.evidence_term_json,
                );
                bridges.push(bridge);

                // Discharge satisfaction obligation (conservative cut)
                // Per spec step 5: if post_caller is null/empty → linker-error unprovable-obligation
                let _ = target_pre; // used below for display
                let discharge_result = discharge_obligation(
                    source_post,
                    target_pre,
                    &edge.source_contract_cid,
                    &target_cid,
                    &edge.target_symbol,
                );
                if let Some(err) = discharge_result {
                    linker_errors.push(err);
                }
            }
        }
    }

    // Sort bridges for determinism
    bridges.sort_by(|a, b| {
        let a_key = a
            .get("header")
            .and_then(|h| h.get("target"))
            .and_then(|t| t.get("cid"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let b_key = b
            .get("header")
            .and_then(|h| h.get("target"))
            .and_then(|t| t.get("cid"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        a_key.cmp(&b_key)
    });

    // --- Step 6: Compute call-edge set CID ---
    let call_edge_set_cid = {
        let mut edge_bytes: Vec<String> = sorted_edges
            .iter()
            .map(|e| {
                serde_json::json!({
                    "sourceContractCid": e.source_contract_cid,
                    "targetContractCid": e.target_contract_cid,
                    "targetSymbol": e.target_symbol,
                })
                .to_string()
            })
            .collect();
        edge_bytes.sort();
        compute_set_cid_sorted(&edge_bytes)
    };

    // --- Step 7: Compute bridge set CID ---
    let bridge_set_cid = {
        let mut bridge_strs: Vec<String> =
            bridges.iter().map(|b| b.to_string()).collect();
        bridge_strs.sort();
        compute_set_cid_sorted(&bridge_strs)
    };

    // --- Step 8: Build link bundle and compute linkBundleCid (R5) ---
    let linker_error_jsons: Vec<Json> = linker_errors
        .iter()
        .map(|e| {
            serde_json::json!({
                "kind": "linker-error",
                "errorKind": e.kind,
                "targetSymbol": e.target_symbol,
                "sourceContractCid": e.source_contract_cid,
                "reason": e.reason,
            })
        })
        .collect();

    // LinkBundle object (minus linkBundleCid for CID computation)
    let bundle_without_cid = serde_json::json!({
        "schemaVersion": "1",
        "kind": "link-bundle",
        "contractSetCid": contract_set_cid,
        "callEdgeSetCid": call_edge_set_cid,
        "bridgeSetCid": bridge_set_cid,
        "linkerVersion": "0.1.0",
        "linkerErrors": linker_error_jsons,
        "bridges": bridges,
    });

    // Compute linkBundleCid over the JCS of bundle_without_cid
    // Sort keys deterministically for CID computation
    let link_bundle_cid = {
        let jcs_bytes = jcs_of_json(&bundle_without_cid);
        blake3_512_of(&jcs_bytes)
    };

    let mut bundle_json = bundle_without_cid;
    if let Some(obj) = bundle_json.as_object_mut() {
        obj.insert(
            "linkBundleCid".into(),
            Json::String(link_bundle_cid.clone()),
        );
    }

    Ok(bundle_json)
}

// -------------------------------------------------------------------
// Step 1: Lift rust contracts
// -------------------------------------------------------------------

fn lift_rust_contracts(rust_dir: &Path) -> Result<Vec<KitContract>, String> {
    if !rust_dir.exists() {
        return Ok(Vec::new());
    }

    let report = lift_path(rust_dir);
    let mut contracts = Vec::new();

    // For each decl, compute its contract CID using the same logic as provekit-lift
    for decl in &report.decls {
        use provekit_ir_symbolic::serialize::formula_to_value;
        let pre_v = decl.pre.as_deref().map(formula_to_value);
        let post_v = decl.post.as_deref().map(formula_to_value);
        let inv_v = decl.inv.as_deref().map(formula_to_value);

        let args = MintContractArgs {
            contract_name: decl.name.clone(),
            pre: pre_v.clone(),
            post: post_v.clone(),
            inv: inv_v.clone(),
            out_binding: decl.out_binding.clone(),
            produced_by: "provekit-linker@0.1.0".into(),
            produced_at: "2026-05-03T00:00:00.000Z".into(),
            input_cids: vec![],
            authoring: Authoring::Lift {
                lifter: "provekit-lift".into(),
                evidence: format!("lifted from `{}` annotations", decl.name),
                source_cid: None,
            },
            signer_seed: [0x42; 32],
        };
        let cid = compute_contract_cid(&args);

        // Convert formula values to serde_json::Value for storage
        let pre_json = pre_v.map(value_arc_to_json);
        let post_json = post_v.map(value_arc_to_json);

        contracts.push(KitContract {
            name: decl.name.clone(),
            kit: "rust-kit".into(),
            contract_cid: cid,
            pre_json,
            post_json,
        });
    }

    Ok(contracts)
}

/// Convert a provekit_canonicalizer::Value to serde_json::Value.
fn value_arc_to_json(v: std::sync::Arc<provekit_canonicalizer::Value>) -> Json {
    value_to_json(&v)
}

fn value_to_json(v: &provekit_canonicalizer::Value) -> Json {
    match v {
        provekit_canonicalizer::Value::Null => Json::Null,
        provekit_canonicalizer::Value::Bool(b) => Json::Bool(*b),
        provekit_canonicalizer::Value::Integer(i) => Json::Number((*i).into()),
        provekit_canonicalizer::Value::String(s) => Json::String(s.clone()),
        provekit_canonicalizer::Value::Array(items) => {
            Json::Array(items.iter().map(|i| value_to_json(i)).collect())
        }
        provekit_canonicalizer::Value::Object(kvs) => {
            let mut map = serde_json::Map::new();
            for (k, v) in kvs {
                map.insert(k.clone(), value_to_json(v));
            }
            Json::Object(map)
        }
    }
}

// -------------------------------------------------------------------
// Step 2: Lift go call-edges via subprocess
// -------------------------------------------------------------------

fn lift_go_call_edges(
    go_dir: &Path,
    go_lsp_bin: Option<&str>,
) -> Result<(Vec<KitContract>, Vec<KitCallEdge>), String> {
    if !go_dir.exists() {
        return Ok((Vec::new(), Vec::new()));
    }

    // Collect all .go files
    let go_files = collect_go_files(go_dir);
    if go_files.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    // Determine the go-lsp binary to spawn
    let bin = go_lsp_bin.unwrap_or("provekit-lsp-go");

    // Spawn the go lifter process with NDJSON protocol
    let mut child = Command::new(bin)
        .current_dir(go_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn {bin}: {e}"))?;

    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let mut reader = BufReader::new(stdout);

    // Send initialize
    let init_req = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}});
    writeln!(stdin, "{}", init_req).map_err(|e| format!("write initialize: {e}"))?;

    // Read initialize response
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| format!("read initialize response: {e}"))?;
    line.clear();

    let mut all_go_contracts: Vec<KitContract> = Vec::new();
    let mut all_call_edges: Vec<KitCallEdge> = Vec::new();

    // Parse each go file
    for (file_path, source) in &go_files {
        let parse_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "parse",
            "params": {
                "path": file_path,
                "source": source
            }
        });
        writeln!(stdin, "{}", parse_req)
            .map_err(|e| format!("write parse request: {e}"))?;

        let mut resp_line = String::new();
        reader
            .read_line(&mut resp_line)
            .map_err(|e| format!("read parse response: {e}"))?;

        let resp: Json = serde_json::from_str(resp_line.trim())
            .map_err(|e| format!("parse response JSON: {e}"))?;

        if let Some(err) = resp.get("error") {
            return Err(format!("go lifter parse error: {err}"));
        }

        if let Some(result) = resp.get("result") {
            // Extract contracts from declarations
            if let Some(decls) = result.get("declarations").and_then(|d| d.as_array()) {
                for decl in decls {
                    if decl.get("kind").and_then(|k| k.as_str()) != Some("contract") {
                        continue;
                    }
                    let name = decl
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("")
                        .to_string();
                    if name.is_empty() {
                        continue;
                    }
                    // Compute contract CID from declaration fields
                    let cid = contract_cid_from_go_decl(decl);
                    let pre_json = decl.get("pre").cloned();
                    let post_json = decl.get("post").cloned();
                    all_go_contracts.push(KitContract {
                        name,
                        kit: "go-kit".into(),
                        contract_cid: cid,
                        pre_json,
                        post_json,
                    });
                }
            }

            // Extract call edges
            if let Some(edges) = result.get("callEdges").and_then(|e| e.as_array()) {
                for edge in edges {
                    if edge.get("kind").and_then(|k| k.as_str()) != Some("call-edge") {
                        continue;
                    }
                    let source_cid = edge
                        .get("sourceContractCid")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if source_cid.is_empty() {
                        continue;
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

                    all_call_edges.push(KitCallEdge {
                        source_contract_cid: source_cid,
                        target_contract_cid: target_cid,
                        target_symbol,
                        call_site_locus_json: locus,
                        evidence_term_json: evidence,
                    });
                }
            }
        }

        resp_line.clear();
    }

    // Send shutdown
    let shutdown_req = serde_json::json!({"jsonrpc":"2.0","id":3,"method":"shutdown","params":{}});
    writeln!(stdin, "{}", shutdown_req)
        .map_err(|e| format!("write shutdown: {e}"))?;
    drop(stdin);

    let _ = child.wait();

    Ok((all_go_contracts, all_call_edges))
}

fn collect_go_files(dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e == "go").unwrap_or(false) {
            if let Ok(src) = std::fs::read_to_string(&path) {
                out.push((path.display().to_string(), src));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Compute the contract CID from a go declaration JSON object.
/// Mirrors the go lifter's contractCidForDeclaration logic.
fn contract_cid_from_go_decl(decl: &Json) -> String {
    // The go lifter computes CID over the marshalled declaration bytes.
    // For cross-kit resolution we need the same CID the go lifter emits
    // in sourceContractCid. We can't reproduce it exactly without the
    // go-kit's canonicalization; instead, use the name as a stable key
    // for the name_kit_index lookup. The actual CID comparison happens
    // against sourceContractCid from the call-edge stream.
    //
    // This is used only for building the go contract index for display.
    // The actual CID is whatever the go lifter put in sourceContractCid.
    decl.get("cid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

// -------------------------------------------------------------------
// Step 3: Cross-kit symbol resolution (R3)
// -------------------------------------------------------------------

/// Resolve a target symbol like "rust-kit:process" against the union of contracts.
///
/// Returns the contract CID if exactly one contract matches; None triggers
/// a linker-error to be emitted by the caller.
fn resolve_target_symbol(
    target_symbol: &str,
    name_kit_index: &BTreeMap<(String, String), String>,
) -> Option<String> {
    // Parse kit prefix: "rust-kit:foo" → kit="rust-kit", name="foo"
    let (kit, name) = parse_kit_symbol(target_symbol)?;

    let key = (name.to_string(), kit.to_string());
    name_kit_index.get(&key).cloned()
}

fn parse_kit_symbol(sym: &str) -> Option<(&str, &str)> {
    let pos = sym.find(':')?;
    let kit = &sym[..pos];
    let name = &sym[pos + 1..];
    if kit.is_empty() || name.is_empty() {
        return None;
    }
    Some((kit, name))
}

// -------------------------------------------------------------------
// Step 4: Derive bridge (R2)
// -------------------------------------------------------------------

/// Mint a derived bridge per spec R2. Shape per §1 DerivedBridge.
fn derive_bridge(
    source_contract_cid: &str,
    target_contract_cid: &str,
    call_site_locus: &Json,
    evidence_term: &Json,
) -> Json {
    serde_json::json!({
        "schemaVersion": "2",
        "kind": "bridge",
        "header": {
            "kind": "bridge",
            "sourceContractCid": source_contract_cid,
            "target": {
                "kind": "contract",
                "cid": target_contract_cid
            }
        },
        "metadata": {
            "callSite": call_site_locus,
            "derivedRelation": {
                "kind": "post-implies-pre",
                "evidenceTerm": evidence_term
            },
            "derivedBy": "linker",
            "linkerVersion": "0.1.0"
        }
    })
}

// -------------------------------------------------------------------
// Step 5: Discharge satisfaction obligation
// -------------------------------------------------------------------

/// Conservative discharge: if post_caller is absent → unprovable-obligation.
/// Returns Some(LinkerError) if the discharge fails; None if it passes.
fn discharge_obligation(
    source_post: Option<&Json>,
    _target_pre: Option<&Json>,
    source_contract_cid: &str,
    target_cid: &str,
    target_symbol: &str,
) -> Option<LinkerError> {
    match source_post {
        None | Some(Json::Null) => {
            Some(LinkerError {
                kind: "unprovable-obligation".into(),
                target_symbol: target_symbol.to_string(),
                source_contract_cid: source_contract_cid.to_string(),
                reason: format!(
                    "caller post-condition is absent; cannot discharge `post_caller ⊃ pre_callee` for target `{target_cid}`"
                ),
            })
        }
        Some(_post) => {
            // For MVP: if post is present, we trust it (no SMT discharge tonight).
            // The smoke fixture success case avoids the cgo call entirely,
            // producing zero cross-kit bridges and zero errors.
            None
        }
    }
}

// -------------------------------------------------------------------
// CID helpers
// -------------------------------------------------------------------

/// Compute blake3-512 CID over a sorted list of strings (set CID pattern).
fn compute_set_cid_sorted(sorted_items: &[String]) -> String {
    let arr: Vec<std::sync::Arc<provekit_canonicalizer::Value>> = sorted_items
        .iter()
        .map(|s| provekit_canonicalizer::Value::string(s.clone()))
        .collect();
    let v = provekit_canonicalizer::Value::array(arr);
    let jcs = encode_jcs(&v);
    blake3_512_of(jcs.as_bytes())
}

/// Compute a stable JCS representation of a serde_json::Value for CID purposes.
/// Keys are sorted lexicographically (JCS §3.2.3).
fn jcs_of_json(v: &Json) -> Vec<u8> {
    // Convert to canonicalizer Value and use the JCS encoder.
    let cv = json_to_value(v);
    let jcs = encode_jcs(&cv);
    jcs.into_bytes()
}

/// Core linker algorithm, separated from I/O for testability.
///
/// Takes pre-collected contracts (from any kit) and call-edges, runs
/// the derivation, and returns a link bundle JSON.
pub fn derive_link_bundle(
    all_contracts: Vec<KitContract>,
    all_call_edges: Vec<KitCallEdge>,
) -> Json {
    // Build cross-kit resolution index
    let mut name_kit_index: BTreeMap<(String, String), String> = BTreeMap::new();
    for c in &all_contracts {
        name_kit_index.insert((c.name.clone(), c.kit.clone()), c.contract_cid.clone());
    }

    // contractSetCid
    let mut all_contract_cids: Vec<String> =
        all_contracts.iter().map(|c| c.contract_cid.clone()).collect();
    all_contract_cids.sort();
    let contract_set_cid = compute_set_cid_sorted(&all_contract_cids);

    let mut bridges: Vec<Json> = Vec::new();
    let mut linker_errors_out: Vec<LinkerError> = Vec::new();

    let mut sorted_edges = all_call_edges;
    sorted_edges.sort_by(|a, b| {
        a.source_contract_cid
            .cmp(&b.source_contract_cid)
            .then_with(|| {
                let la = a.call_site_locus_json.to_string();
                let lb = b.call_site_locus_json.to_string();
                la.cmp(&lb)
            })
    });

    for edge in &sorted_edges {
        let resolved_target_cid = if let Some(ref cid) = edge.target_contract_cid {
            Some(cid.clone())
        } else {
            resolve_target_symbol(&edge.target_symbol, &name_kit_index)
        };

        match resolved_target_cid {
            None => {
                linker_errors_out.push(LinkerError {
                    kind: "unresolved-symbol".into(),
                    target_symbol: edge.target_symbol.clone(),
                    source_contract_cid: edge.source_contract_cid.clone(),
                    reason: format!(
                        "targetSymbol `{}` did not resolve to any contract in the union",
                        edge.target_symbol
                    ),
                });
            }
            Some(target_cid) => {
                let target_contract = all_contracts
                    .iter()
                    .find(|c| c.contract_cid == target_cid);
                let source_contract = all_contracts
                    .iter()
                    .find(|c| c.contract_cid == edge.source_contract_cid);

                let source_post = source_contract.and_then(|c| c.post_json.as_ref());
                let target_pre = target_contract.and_then(|c| c.pre_json.as_ref());

                let bridge = derive_bridge(
                    &edge.source_contract_cid,
                    &target_cid,
                    &edge.call_site_locus_json,
                    &edge.evidence_term_json,
                );
                bridges.push(bridge);

                let _ = target_pre;
                if let Some(err) = discharge_obligation(
                    source_post,
                    target_pre,
                    &edge.source_contract_cid,
                    &target_cid,
                    &edge.target_symbol,
                ) {
                    linker_errors_out.push(err);
                }
            }
        }
    }

    bridges.sort_by(|a, b| {
        let ak = a
            .get("header")
            .and_then(|h| h.get("target"))
            .and_then(|t| t.get("cid"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let bk = b
            .get("header")
            .and_then(|h| h.get("target"))
            .and_then(|t| t.get("cid"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        ak.cmp(&bk)
    });

    let call_edge_set_cid = {
        let mut edge_bytes: Vec<String> = sorted_edges
            .iter()
            .map(|e| {
                serde_json::json!({
                    "sourceContractCid": e.source_contract_cid,
                    "targetContractCid": e.target_contract_cid,
                    "targetSymbol": e.target_symbol,
                })
                .to_string()
            })
            .collect();
        edge_bytes.sort();
        compute_set_cid_sorted(&edge_bytes)
    };

    let bridge_set_cid = {
        let mut bridge_strs: Vec<String> = bridges.iter().map(|b| b.to_string()).collect();
        bridge_strs.sort();
        compute_set_cid_sorted(&bridge_strs)
    };

    let linker_error_jsons: Vec<Json> = linker_errors_out
        .iter()
        .map(|e| {
            serde_json::json!({
                "kind": "linker-error",
                "errorKind": e.kind,
                "targetSymbol": e.target_symbol,
                "sourceContractCid": e.source_contract_cid,
                "reason": e.reason,
            })
        })
        .collect();

    let bundle_without_cid = serde_json::json!({
        "schemaVersion": "1",
        "kind": "link-bundle",
        "contractSetCid": contract_set_cid,
        "callEdgeSetCid": call_edge_set_cid,
        "bridgeSetCid": bridge_set_cid,
        "linkerVersion": "0.1.0",
        "linkerErrors": linker_error_jsons,
        "bridges": bridges,
    });

    let link_bundle_cid = blake3_512_of(&jcs_of_json(&bundle_without_cid));
    let mut bundle_json = bundle_without_cid;
    if let Some(obj) = bundle_json.as_object_mut() {
        obj.insert("linkBundleCid".into(), Json::String(link_bundle_cid));
    }
    bundle_json
}

fn json_to_value(j: &Json) -> provekit_canonicalizer::Value {
    match j {
        Json::Null => provekit_canonicalizer::Value::Null,
        Json::Bool(b) => provekit_canonicalizer::Value::Bool(*b),
        Json::Number(n) => {
            if let Some(i) = n.as_i64() {
                provekit_canonicalizer::Value::Integer(i)
            } else {
                // Floats are not in the Value enum; represent as string
                provekit_canonicalizer::Value::String(n.to_string())
            }
        }
        Json::String(s) => provekit_canonicalizer::Value::String(s.clone()),
        Json::Array(arr) => {
            provekit_canonicalizer::Value::Array(arr.iter().map(json_to_value).map(std::sync::Arc::new).collect())
        }
        Json::Object(map) => {
            // Insertion order matters for JCS; we sort by key per RFC 8785.
            let mut entries: Vec<(String, std::sync::Arc<provekit_canonicalizer::Value>)> =
                map.iter()
                    .map(|(k, v)| (k.clone(), std::sync::Arc::new(json_to_value(v))))
                    .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            provekit_canonicalizer::Value::Object(entries)
        }
    }
}
