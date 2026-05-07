// SPDX-License-Identifier: Apache-2.0
//
// `provekit link` — linker pass per spec
// `protocol/specs/2026-05-03-bridge-linkage-protocol.md` R2-R5.
//
// Orchestrates:
//   1. Lift rust source in <project>/rust-callee/ via provekit-lift.
//   2. Spawn go-lsp-lifter over <project>/go-caller/ to get call-edges.
//   3. Call provekit_linker::link() — pure derivation; no side effects.
//   4. Write the resulting LinkBundle JSON to disk.
//
// The pure linker algebra lives in `provekit-linker`.  This module is
// responsible only for I/O: reading source files, spawning the go lifter
// subprocess, and writing the output bundle.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use owo_colors::OwoColorize;
use provekit_claim_envelope::{contract_cid as compute_contract_cid, Authoring, MintContractArgs};
use provekit_lift::lift_path;
use provekit_linker::{link, LinkerCallEdge, LinkerContract, LinkerInputs};
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

    match gather_and_link(&project_root, args.go_lsp_bin.as_deref()) {
        Ok(output) => {
            let bundle = &output.bundle_json;
            let out_path = project_root.join("link-bundle.json");
            let json = serde_json::to_string_pretty(bundle)
                .expect("bundle JSON serialization is infallible for content-hashed data");
            if let Err(e) = std::fs::write(&out_path, &json) {
                eprintln!("{}: write link-bundle.json: {e}", "error".red().bold());
                return crate::EXIT_USER_ERROR;
            }
            let error_count = output.linker_errors.len();
            let bundle_cid = &output.link_bundle_cid;

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
            println!("{}: clean link bundle", "linker".green().bold());
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
// I/O gathering — lift both kits, then delegate to provekit_linker::link
// -------------------------------------------------------------------

fn gather_and_link(
    project_root: &Path,
    go_lsp_bin: Option<&str>,
) -> Result<provekit_linker::LinkerOutput, String> {
    // --- Step 1: Lift rust contracts ---
    let rust_dir = project_root.join("rust-callee");
    let rust_contracts = lift_rust_contracts(&rust_dir)?;

    // --- Step 2: Lift go call-edges ---
    let go_dir = project_root.join("go-caller");
    let (go_contracts, go_call_edges) = lift_go_call_edges(&go_dir, go_lsp_bin)?;

    // --- Step 3: Delegate pure derivation to provekit-linker ---
    let mut all_contracts = rust_contracts;
    all_contracts.extend(go_contracts);

    Ok(link(LinkerInputs {
        contracts: all_contracts,
        call_edges: go_call_edges,
    }))
}

// -------------------------------------------------------------------
// Step 1: Lift rust contracts
// -------------------------------------------------------------------

fn lift_rust_contracts(rust_dir: &Path) -> Result<Vec<LinkerContract>, String> {
    if !rust_dir.exists() {
        return Ok(Vec::new());
    }

    let report = lift_path(rust_dir);
    let mut contracts = Vec::new();

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

        let pre_json = pre_v.map(value_arc_to_json);
        let post_json = post_v.map(value_arc_to_json);

        contracts.push(LinkerContract {
            name: decl.name.clone(),
            kit: "rust-kit".into(),
            contract_cid: cid,
            pre_json,
            post_json,
        });
    }

    Ok(contracts)
}

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
) -> Result<(Vec<LinkerContract>, Vec<LinkerCallEdge>), String> {
    if !go_dir.exists() {
        return Ok((Vec::new(), Vec::new()));
    }

    let go_files = collect_go_files(go_dir);
    if go_files.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    let bin = go_lsp_bin.unwrap_or("provekit-lsp-go");

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

    let init_req = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}});
    writeln!(stdin, "{}", init_req).map_err(|e| format!("write initialize: {e}"))?;

    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| format!("read initialize response: {e}"))?;
    line.clear();

    let mut all_go_contracts: Vec<LinkerContract> = Vec::new();
    let mut all_call_edges: Vec<LinkerCallEdge> = Vec::new();

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
        writeln!(stdin, "{}", parse_req).map_err(|e| format!("write parse request: {e}"))?;

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
                    let cid = contract_cid_from_go_decl(decl);
                    let pre_json = decl.get("pre").cloned();
                    let post_json = decl.get("post").cloned();
                    all_go_contracts.push(LinkerContract {
                        name,
                        kit: "go-kit".into(),
                        contract_cid: cid,
                        pre_json,
                        post_json,
                    });
                }
            }

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
                    let locus = edge.get("callSiteLocus").cloned().unwrap_or(Json::Null);
                    let evidence = edge.get("evidenceTerm").cloned().unwrap_or(Json::Null);

                    all_call_edges.push(LinkerCallEdge {
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

    let shutdown_req = serde_json::json!({"jsonrpc":"2.0","id":3,"method":"shutdown","params":{}});
    writeln!(stdin, "{}", shutdown_req).map_err(|e| format!("write shutdown: {e}"))?;
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

fn contract_cid_from_go_decl(decl: &Json) -> String {
    decl.get("cid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}
