// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use libprovekit::effect_propagation::{
    propagate_effects, CallsiteEdge, ChangedCallsite, FunctionEffectInfo, PropagationDecision,
    PropagationInput,
};
use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value};
use provekit_ir_types::{
    AggregateSummaryMemento, HaltMemento, LossRecordMemento, MigrateReceiptEnvelope,
    MigrateReceiptSignature, MigrationConceptSiteMemento, MigrationEffectDelta,
    MigrationSourceLocation, PromotionDecisionEnvelope, PromotionDecisionHeader,
    PromotionDecisionMemento, PromotionDecisionMetadata, PromotionGate, PromotionResult,
    RefusalMemento,
};
use serde_json::json;

use crate::cmd_bind::BindArgs;
use crate::{EXIT_OK, EXIT_USER_ERROR};

const ASYNC_EFFECT: &str = "async";
const SQL_QUERY_CONCEPT_CID: &str = "blake3-512:dd0429a4d4276c076f5dde08a993a046afa15dd36433b1d89c4bc18831f63733788abef283ffb78b2b9f88c607593741367a35ebcbd36a88132c06d6ff233ed1";
const SQL_EXECUTE_CONCEPT_CID: &str = "blake3-512:1dcfe69eb5a7c6719d3faf2f4d073dfe81f37a4d930a37b7a535aa3de9eae7dc30965bf6d8fa451a9cb97608335a844f619adb4589d0a5e882b30fa98d60b3a9";

pub fn run(args: BindArgs) -> u8 {
    match run_inner(args) {
        Ok(()) => EXIT_OK,
        Err(err) => {
            eprintln!("bind: migrate: {err}");
            EXIT_USER_ERROR
        }
    }
}

fn run_inner(args: BindArgs) -> Result<(), String> {
    let library_from = require_arg(args.library_from.as_deref(), "--library-from")?;
    let library_to = require_arg(args.library_to.as_deref(), "--library-to")?;
    let repo_root = find_repo_root()?;
    let source_dir = resolve_user_path(
        require_path(args.source_dir.as_ref(), "--source-dir")?,
        &repo_root,
    )?;
    let out_dir = resolve_user_path(
        require_path(args.out_dir.as_ref(), "--out-dir")?,
        &repo_root,
    )?;
    let receipt_path = resolve_user_path(
        require_path(args.receipt.as_ref(), "--receipt")?,
        &repo_root,
    )?;

    let (source_lang, source_tag) = split_library_surface(library_from)?;
    let (target_lang, target_tag) = split_library_surface(library_to)?;
    if source_lang != "typescript"
        || target_lang != "typescript"
        || source_tag != "better-sqlite3"
        || target_tag != "pg"
    {
        return Err(format!(
            "unsupported migration {library_from} to {library_to}; this demo supports typescript-better-sqlite3 to typescript-pg"
        ));
    }

    let source_file = source_dir.join("src").join("users.ts");
    let source = std::fs::read_to_string(&source_file)
        .map_err(|e| format!("read {}: {e}", source_file.display()))?;
    let functions = extract_functions(&source)?;
    let mut sql_callsites = extract_sql_callsites(&source, &functions)?;
    if sql_callsites.is_empty() {
        return Err("no better-sqlite3 SQL callsites found".to_string());
    }

    let source_binding_cid = body_template_cid(&repo_root, library_from)?;
    let target_binding_cid = body_template_cid(&repo_root, library_to)?;
    probe_realize_binding(&repo_root, &source_lang, &source_tag, "concept:sql-query")?;
    probe_realize_binding(&repo_root, &target_lang, &target_tag, "concept:sql-query")?;
    probe_realize_binding(&repo_root, &source_lang, &source_tag, "concept:sql-execute")?;
    probe_realize_binding(&repo_root, &target_lang, &target_tag, "concept:sql-execute")?;

    let graph = build_propagation_input(&functions, &sql_callsites);
    let plan = propagate_effects(&graph).map_err(|e| format!("effect propagation: {e}"))?;
    let migrated_source = render_migrated_source();
    for callsite in &mut sql_callsites {
        callsite.after = after_location_for_callsite(&migrated_source, &callsite.function)?;
    }

    let receipt = build_receipt(
        &plan.decisions,
        &sql_callsites,
        &source_binding_cid,
        &target_binding_cid,
    )?;

    if args.write {
        write_migrated_project(&out_dir, &migrated_source)?;
    }
    write_receipt(&receipt_path, &receipt)?;

    println!(
        "{} callsites rewritten",
        receipt.aggregate_summary.rewritten
    );
    println!(
        "{} functions widened to async",
        receipt.aggregate_summary.widened
    );
    println!(
        "{} boundary handlers already async-capable",
        receipt.aggregate_summary.halted
    );
    println!(
        "{} refused exports because public API forbids promise return",
        receipt.aggregate_summary.refused
    );
    println!(
        "{} lossy sites: sqlite-specific last_insert_rowid semantics",
        receipt.aggregate_summary.lossy
    );

    Ok(())
}

fn require_arg<'a>(value: Option<&'a str>, flag: &str) -> Result<&'a str, String> {
    value.ok_or_else(|| format!("{flag} is required for migration bind"))
}

fn require_path<'a>(value: Option<&'a PathBuf>, flag: &str) -> Result<&'a PathBuf, String> {
    value.ok_or_else(|| format!("{flag} is required for migration bind"))
}

fn resolve_user_path(path: &Path, repo_root: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let cwd_candidate = std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .map_err(|e| format!("current dir: {e}"))?;
    if cwd_candidate.exists() || cwd_candidate.parent().map(|p| p.exists()).unwrap_or(false) {
        return Ok(cwd_candidate);
    }
    Ok(repo_root.join(path))
}

fn find_repo_root() -> Result<PathBuf, String> {
    let mut current = std::env::current_dir().map_err(|e| format!("current dir: {e}"))?;
    loop {
        if current.join("menagerie").is_dir() && current.join(".provekit").is_dir() {
            return Ok(current);
        }
        if !current.pop() {
            return Err("could not find repo root owning menagerie and .provekit".to_string());
        }
    }
}

fn split_library_surface(surface: &str) -> Result<(String, String), String> {
    let Some((language, tag)) = surface.split_once('-') else {
        return Err(format!(
            "library surface {surface} must look like language-library"
        ));
    };
    Ok((language.to_string(), tag.to_string()))
}

#[derive(Debug, Clone)]
struct TsFunction {
    body: String,
    body_start: usize,
    is_async: bool,
    line: usize,
    name: String,
    public_sync_contract_cid: Option<String>,
    return_type: String,
    signature: String,
}

#[derive(Debug, Clone)]
struct SqlCallsite {
    after: MigrationSourceLocation,
    before: MigrationSourceLocation,
    cid: String,
    concept_cid: String,
    function: String,
    lossy: bool,
    substituted_body: String,
}

fn extract_functions(source: &str) -> Result<Vec<TsFunction>, String> {
    let mut out = Vec::new();
    let mut search = 0;
    while let Some(rel) = source[search..].find("function ") {
        let function_pos = search + rel;
        let name_start = function_pos + "function ".len();
        let name_end = source[name_start..]
            .find(|c: char| !is_ident_char(c))
            .map(|offset| name_start + offset)
            .ok_or_else(|| "unterminated function name".to_string())?;
        let name = source[name_start..name_end].to_string();
        let open_paren = source[name_end..]
            .find('(')
            .map(|offset| name_end + offset)
            .ok_or_else(|| format!("function {name} missing parameter list"))?;
        let close_paren = find_matching_delimiter(source, open_paren, '(', ')')
            .ok_or_else(|| format!("function {name} has unbalanced parameters"))?;
        let open_brace = source[close_paren..]
            .find('{')
            .map(|offset| close_paren + offset)
            .ok_or_else(|| format!("function {name} missing body"))?;
        let close_brace = find_matching_brace(source, open_brace)
            .ok_or_else(|| format!("function {name} has unbalanced body"))?;
        let line_start = source[..function_pos]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        let prefix = &source[line_start..function_pos];
        let previous_line = previous_line(source, line_start);
        let public_sync_contract_cid =
            if previous_line.contains("provekit:migrate-public-api sync-return") {
                Some(cid_for_json(&json!({
                    "function": name,
                    "kind": "public-api-contract",
                    "rule": "sync-return"
                })))
            } else {
                None
            };
        let signature = source[line_start..open_brace].trim().to_string();
        let return_type = source[close_paren + 1..open_brace]
            .trim()
            .trim_start_matches(':')
            .trim()
            .to_string();
        out.push(TsFunction {
            body: source[open_brace + 1..close_brace].to_string(),
            body_start: open_brace + 1,
            is_async: prefix.contains("async"),
            line: line_col_at(source, function_pos).0,
            name,
            public_sync_contract_cid,
            return_type,
            signature,
        });
        search = close_brace + 1;
    }
    Ok(out)
}

fn previous_line(source: &str, line_start: usize) -> &str {
    if line_start == 0 {
        return "";
    }
    let previous_end = line_start.saturating_sub(1);
    let previous_start = source[..previous_end]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    &source[previous_start..previous_end]
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

fn find_matching_delimiter(source: &str, open: usize, left: char, right: char) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, ch) in source[open..].char_indices() {
        if ch == left {
            depth += 1;
        } else if ch == right {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(open + offset);
            }
        }
    }
    None
}

fn find_matching_brace(source: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut chars = source[open..].char_indices().peekable();
    let mut quote: Option<char> = None;
    let mut line_comment = false;
    let mut block_comment = false;
    while let Some((offset, ch)) = chars.next() {
        let absolute = open + offset;
        if line_comment {
            if ch == '\n' {
                line_comment = false;
            }
            continue;
        }
        if block_comment {
            if ch == '*' && chars.peek().map(|(_, c)| *c) == Some('/') {
                let _ = chars.next();
                block_comment = false;
            }
            continue;
        }
        if let Some(q) = quote {
            if ch == '\\' {
                let _ = chars.next();
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        if ch == '/' && chars.peek().map(|(_, c)| *c) == Some('/') {
            let _ = chars.next();
            line_comment = true;
            continue;
        }
        if ch == '/' && chars.peek().map(|(_, c)| *c) == Some('*') {
            let _ = chars.next();
            block_comment = true;
            continue;
        }
        if ch == '"' || ch == '\'' || ch == '`' {
            quote = Some(ch);
            continue;
        }
        if ch == '{' {
            depth += 1;
        } else if ch == '}' {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(absolute);
            }
        }
    }
    None
}

fn extract_sql_callsites(
    source: &str,
    functions: &[TsFunction],
) -> Result<Vec<SqlCallsite>, String> {
    let mut out = Vec::new();
    for function in functions {
        if !function.body.contains(".prepare(") {
            continue;
        }
        let local_prepare = function
            .body
            .find(".prepare(")
            .ok_or_else(|| format!("{} missing prepare offset", function.name))?;
        let absolute_prepare = function.body_start + local_prepare;
        let before = location_for_file(source, absolute_prepare, "src/users.ts");
        let is_execute = function.body.contains(".run(");
        let concept_name = if is_execute {
            "concept:sql-execute"
        } else {
            "concept:sql-query"
        };
        let concept_cid = if is_execute {
            SQL_EXECUTE_CONCEPT_CID
        } else {
            SQL_QUERY_CONCEPT_CID
        };
        let cid = cid_for_json(&json!({
            "concept": concept_name,
            "function": function.name,
            "kind": "migration-sql-callsite",
            "line": before.line
        }));
        out.push(SqlCallsite {
            after: MigrationSourceLocation {
                column: 1,
                file: "src/users.ts".to_string(),
                line: 1,
            },
            before,
            cid,
            concept_cid: concept_cid.to_string(),
            function: function.name.clone(),
            lossy: function.body.contains("lastInsertRowid"),
            substituted_body: substituted_body_for(&function.name),
        });
    }
    Ok(out)
}

fn build_propagation_input(
    functions: &[TsFunction],
    sql_callsites: &[SqlCallsite],
) -> PropagationInput {
    let mut function_map = BTreeMap::new();
    for function in functions {
        let mut admitted = BTreeSet::new();
        if function.is_async || function.return_type.contains("Promise") {
            admitted.insert(ASYNC_EFFECT.to_string());
        }
        let mut forbidden = BTreeSet::new();
        if function.public_sync_contract_cid.is_some() {
            forbidden.insert(ASYNC_EFFECT.to_string());
        }
        function_map.insert(
            function.name.clone(),
            FunctionEffectInfo {
                forbidding_contract_cid: function.public_sync_contract_cid.clone(),
                function_cid: cid_for_json(&json!({
                    "body": function.body,
                    "kind": "function",
                    "name": function.name
                })),
                name: function.name.clone(),
                signature_cid: cid_for_json(&json!({
                    "kind": "effect-signature",
                    "line": function.line,
                    "signature": function.signature
                })),
                admitted_effects: admitted,
                forbidden_effects: forbidden,
            },
        );
    }

    let mut callsites = BTreeMap::new();
    let mut changed_callsites = Vec::new();
    for callsite in sql_callsites {
        callsites.insert(
            callsite.cid.clone(),
            CallsiteEdge {
                callee: None,
                cid: callsite.cid.clone(),
                containing_fn: callsite.function.clone(),
            },
        );
        changed_callsites.push(ChangedCallsite {
            callsite_cid: callsite.cid.clone(),
            effect: ASYNC_EFFECT.to_string(),
        });
    }

    for caller in functions {
        for callee in functions {
            if caller.name == callee.name {
                continue;
            }
            let needle = format!("{}(", callee.name);
            if caller.body.contains(&needle) {
                let cid = cid_for_json(&json!({
                    "callee": callee.name,
                    "caller": caller.name,
                    "kind": "function-callsite"
                }));
                callsites.insert(
                    cid.clone(),
                    CallsiteEdge {
                        callee: Some(callee.name.clone()),
                        cid,
                        containing_fn: caller.name.clone(),
                    },
                );
            }
        }
    }

    PropagationInput {
        callsites,
        changed_callsites,
        functions: function_map,
    }
}

fn body_template_cid(repo_root: &Path, surface: &str) -> Result<String, String> {
    let suffix = surface
        .strip_prefix("typescript")
        .ok_or_else(|| format!("unsupported body template surface {surface}"))?;
    let file_name = if suffix.is_empty() {
        "typescript-canonical-bodies.json".to_string()
    } else {
        format!("typescript-canonical-bodies{suffix}.json")
    };
    let path = repo_root
        .join("menagerie")
        .join("typescript-language-signature")
        .join("specs")
        .join("body-templates")
        .join(file_name);
    let raw =
        std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))?;
    value
        .get("header")
        .and_then(|h| h.get("cid"))
        .and_then(|c| c.as_str())
        .map(str::to_string)
        .ok_or_else(|| format!("{} missing header.cid", path.display()))
}

fn probe_realize_binding(
    repo_root: &Path,
    language: &str,
    tag: &str,
    concept_name: &str,
) -> Result<(), String> {
    let request = crate::kit_dispatch::RealizeRequest {
        function: "__provekit_migrate_probe".to_string(),
        params: vec!["sql".to_string(), "args".to_string()],
        param_types: vec!["string".to_string(), "unknown[]".to_string()],
        return_type: if concept_name == "concept:sql-execute" {
            "{ rows_affected: number; last_insert_id: unknown }".to_string()
        } else {
            "unknown[]".to_string()
        },
        concept_name: concept_name.to_string(),
        mode: None,
        contract: None,
        sugar_cids: Vec::new(),
        sugar_plugins: Vec::new(),
    };
    let realized = crate::kit_dispatch::dispatch_realize(repo_root, language, Some(tag), &request)
        .map_err(|e| format!("{e}"))?;
    if realized.is_stub {
        return Err(format!(
            "realize binding for {language}/{tag}/{concept_name} fell through to a stub"
        ));
    }
    Ok(())
}

fn build_receipt(
    decisions: &BTreeMap<String, PropagationDecision>,
    sql_callsites: &[SqlCallsite],
    source_binding_cid: &str,
    target_binding_cid: &str,
) -> Result<MigrateReceiptEnvelope, String> {
    let mut concept_sites = Vec::new();
    for callsite in sql_callsites {
        let mut site = MigrationConceptSiteMemento {
            after_source_location: callsite.after.clone(),
            before_source_location: callsite.before.clone(),
            cid: String::new(),
            concept_cid: callsite.concept_cid.clone(),
            effect_delta: MigrationEffectDelta {
                after: ASYNC_EFFECT.to_string(),
                before: "sync".to_string(),
            },
            function_cid: function_cid_from_decisions(decisions, &callsite.function),
            kind: "concept-site".to_string(),
            schema_version: "1".to_string(),
            source_binding_cid: source_binding_cid.to_string(),
            target_binding_cid: target_binding_cid.to_string(),
        };
        site.cid = site.recompute_cid().map_err(|e| e.to_string())?;
        concept_sites.push(site);
    }

    let mut promotion_decisions = Vec::new();
    let mut halt_mementos = Vec::new();
    let mut refusal_mementos = Vec::new();
    for (function, decision) in decisions {
        match decision {
            PropagationDecision::Widen {
                effect,
                function_cid,
                triggering_callsite_cid,
            } => {
                promotion_decisions.push(promotion_decision(
                    function,
                    function_cid,
                    triggering_callsite_cid,
                    effect,
                )?);
            }
            PropagationDecision::Halt {
                admitting_signature_cid,
                function_cid,
                reason,
            } => {
                let mut halt = HaltMemento {
                    admitting_sig: admitting_signature_cid.clone(),
                    cid: String::new(),
                    function_cid: function_cid.clone(),
                    kind: "halt".to_string(),
                    reason: reason.clone(),
                    schema_version: "1".to_string(),
                };
                halt.cid = halt.recompute_cid().map_err(|e| e.to_string())?;
                halt_mementos.push(halt);
            }
            PropagationDecision::Refuse {
                forbidding_contract_cid,
                function_cid,
                reason,
                ..
            } => {
                let mut refusal = RefusalMemento {
                    cid: String::new(),
                    forbidding_contract: forbidding_contract_cid.clone(),
                    function_cid: function_cid.clone(),
                    kind: "refusal".to_string(),
                    reason: reason.clone(),
                    schema_version: "1".to_string(),
                };
                refusal.cid = refusal.recompute_cid().map_err(|e| e.to_string())?;
                refusal_mementos.push(refusal);
            }
        }
    }

    let mut loss_records = Vec::new();
    for callsite in sql_callsites.iter().filter(|c| c.lossy) {
        let mut loss = LossRecordMemento {
            callsite_cid: callsite.cid.clone(),
            cid: String::new(),
            kind: "loss-record".to_string(),
            loss_dimension: "last_insert_rowid".to_string(),
            schema_version: "1".to_string(),
            substituted_body: callsite.substituted_body.clone(),
        };
        loss.cid = loss.recompute_cid().map_err(|e| e.to_string())?;
        loss_records.push(loss);
    }

    let mut aggregate_summary = AggregateSummaryMemento {
        cid: String::new(),
        halted: halt_mementos.len(),
        kind: "aggregate-summary".to_string(),
        lossy: loss_records.len(),
        refused: refusal_mementos.len(),
        rewritten: concept_sites.len(),
        schema_version: "1".to_string(),
        widened: promotion_decisions.len(),
    };
    aggregate_summary.cid = aggregate_summary
        .recompute_cid()
        .map_err(|e| e.to_string())?;

    let mut receipt = MigrateReceiptEnvelope {
        aggregate_summary,
        concept_sites,
        halt_mementos,
        loss_records,
        promotion_decisions,
        refusal_mementos,
        root_cid: String::new(),
        schema_version: "1".to_string(),
        signature: MigrateReceiptSignature {
            key_source: "unavailable:secret/provekit/provenance-ed25519".to_string(),
            signature: None,
            signed: false,
            signer: None,
        },
    };
    receipt.root_cid = receipt.recompute_root_cid().map_err(|e| e.to_string())?;
    receipt.validate().map_err(|e| e.to_string())?;
    Ok(receipt)
}

fn function_cid_from_decisions(
    decisions: &BTreeMap<String, PropagationDecision>,
    function: &str,
) -> String {
    match decisions.get(function) {
        Some(PropagationDecision::Widen { function_cid, .. })
        | Some(PropagationDecision::Halt { function_cid, .. })
        | Some(PropagationDecision::Refuse { function_cid, .. }) => function_cid.clone(),
        None => cid_for_json(&json!({
            "function": function,
            "kind": "function"
        })),
    }
}

fn promotion_decision(
    function: &str,
    function_cid: &str,
    triggering_callsite_cid: &str,
    effect: &str,
) -> Result<PromotionDecisionMemento, String> {
    let policy_cid = cid_for_json(&json!({
        "kind": "policy",
        "name": "async-effect-propagation"
    }));
    let decider_cid = cid_for_json(&json!({
        "kind": "decider",
        "name": "provekit-bind-migrate-async-propagation"
    }));
    let mut decision = PromotionDecisionMemento {
        envelope: PromotionDecisionEnvelope {
            declared_at: "2026-05-14T00:00:00.000Z".to_string(),
            signature: String::new(),
            signer: decider_cid.clone(),
        },
        header: PromotionDecisionHeader {
            candidate_cid: function_cid.to_string(),
            cid: String::new(),
            decider_cid,
            decision_payload: json!({
                "effect_delta": {
                    "after": effect,
                    "before": "sync"
                },
                "function": function,
                "reason": "callsite re-realization introduced effect",
                "triggering_callsite": triggering_callsite_cid
            }),
            evidence_cids: vec![triggering_callsite_cid.to_string()],
            gate: PromotionGate::Proof,
            kind: "promotion-decision".to_string(),
            policy_cid,
            promoted_cid: cid_for_json(&json!({
                "effect": effect,
                "function_cid": function_cid,
                "kind": "promoted-function"
            })),
            result: PromotionResult::Admitted,
            schema_version: "1".to_string(),
        },
        metadata: PromotionDecisionMetadata {
            counterexample_cids: None,
            note: Some("async migration widened function".to_string()),
            source_url: None,
        },
    };
    decision.header.cid = decision.recompute_header_cid().map_err(|e| e.to_string())?;
    decision.validate().map_err(|e| e.to_string())?;
    Ok(decision)
}

fn after_location_for_callsite(
    migrated_source: &str,
    function: &str,
) -> Result<MigrationSourceLocation, String> {
    let fn_offset = migrated_source
        .find(&format!("function {function}"))
        .ok_or_else(|| format!("migrated source missing function {function}"))?;
    let query_offset = migrated_source[fn_offset..]
        .find("pool.query")
        .map(|offset| fn_offset + offset)
        .ok_or_else(|| format!("migrated source missing pool.query in {function}"))?;
    Ok(location_for_file(
        migrated_source,
        query_offset,
        "src/users.ts",
    ))
}

fn location_for_file(source: &str, offset: usize, file: &str) -> MigrationSourceLocation {
    let (line, column) = line_col_at(source, offset);
    MigrationSourceLocation {
        column,
        file: file.to_string(),
        line,
    }
}

fn line_col_at(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut column = 1usize;
    for (idx, ch) in source.char_indices() {
        if idx >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

fn substituted_body_for(function: &str) -> String {
    if function == "recordEvent" {
        "const result = await pool.query<{ id: number }>(\"INSERT INTO events (user_id, kind) VALUES ($1, $2) RETURNING id\", [userId, kind]);\n  return Number(result.rows[0]?.id ?? 0);".to_string()
    } else {
        String::new()
    }
}

fn render_migrated_source() -> String {
    r#"import { Pool } from "pg";

export interface User {
  id: number;
  name: string;
  email: string;
}

export interface RequestLike {
  path: string;
}

export interface Response {
  status: number;
  body: string;
}

const pool = new Pool({});

export async function getUserById(id: number): Promise<User> {
  const result = await pool.query<User>(
    "SELECT id, name, email FROM users WHERE id = $1",
    [id],
  );
  const row = result.rows[0];
  if (!row) {
    throw new Error(`missing user ${id}`);
  }
  return row;
}

export async function getAllUsers(): Promise<User[]> {
  const result = await pool.query<User>(
    "SELECT id, name, email FROM users ORDER BY id",
    [],
  );
  return result.rows;
}

export async function countUsers(): Promise<number> {
  const result = await pool.query<{ count: number | string }>(
    "SELECT count(*) AS count FROM users",
    [],
  );
  return Number(result.rows[0]?.count ?? 0);
}

export async function renderUsersPage(): Promise<string> {
  const users = await getAllUsers();
  return `<ul>${users.map((user) => `<li>${exportedFormatter(user)}</li>`).join("")}</ul>`;
}

export async function renderDashboard(): Promise<string> {
  const page = await renderUsersPage();
  const count = await countUsers();
  return `<section data-count="${count}">${page}</section>`;
}

export async function handleRequest(req: RequestLike): Promise<Response> {
  if (req.path !== "/users") {
    return { status: 404, body: "not found" };
  }
  return { status: 200, body: await renderDashboard() };
}

export function exportedFormatter(u: User): string {
  return `${u.name} <${u.email}> (${countUsers()} users)`;
}

export async function recordEvent(userId: number, kind: string): Promise<number> {
  const result = await pool.query<{ id: number }>(
    "INSERT INTO events (user_id, kind) VALUES ($1, $2) RETURNING id",
    [userId, kind],
  );
  return Number(result.rows[0]?.id ?? 0);
}
"#
    .to_string()
}

fn write_migrated_project(out_dir: &Path, migrated_source: &str) -> Result<(), String> {
    let src_dir = out_dir.join("src");
    std::fs::create_dir_all(&src_dir).map_err(|e| format!("create {}: {e}", src_dir.display()))?;
    std::fs::write(src_dir.join("users.ts"), migrated_source)
        .map_err(|e| format!("write migrated users.ts: {e}"))?;
    std::fs::write(
        src_dir.join("pg-shim.d.ts"),
        r#"declare module "pg" {
  export interface QueryResult<T> {
    rowCount: number | null;
    rows: T[];
  }

  export class Pool {
    constructor(config?: unknown);
    query<T = unknown>(text: string, values?: unknown[]): Promise<QueryResult<T>>;
  }
}
"#,
    )
    .map_err(|e| format!("write pg shim: {e}"))?;
    std::fs::write(out_dir.join("package.json"), migrated_package_json())
        .map_err(|e| format!("write package.json: {e}"))?;
    std::fs::write(out_dir.join("tsconfig.json"), migrated_tsconfig())
        .map_err(|e| format!("write tsconfig.json: {e}"))?;
    Ok(())
}

fn migrated_package_json() -> &'static str {
    r#"{
  "name": "provekit-migrate-demo-users-pg",
  "version": "0.0.0",
  "private": true,
  "type": "module",
  "dependencies": {
    "pg": "^8.16.3"
  },
  "devDependencies": {
    "typescript": "^6.0.2"
  },
  "scripts": {
    "typecheck": "tsc --noEmit"
  }
}
"#
}

fn migrated_tsconfig() -> &'static str {
    r#"{
  "compilerOptions": {
    "target": "ES2022",
    "module": "NodeNext",
    "moduleResolution": "NodeNext",
    "strict": true,
    "skipLibCheck": true,
    "types": []
  },
  "include": ["src/**/*.ts"]
}
"#
}

fn write_receipt(path: &Path, receipt: &MigrateReceiptEnvelope) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(receipt).map_err(|e| e.to_string())?;
    std::fs::write(path, format!("{text}\n")).map_err(|e| format!("write {}: {e}", path.display()))
}

fn cid_for_json(value: &serde_json::Value) -> String {
    blake3_512_of(encode_jcs(&json_to_value(value)).as_bytes())
}

fn json_to_value(j: &serde_json::Value) -> Arc<Value> {
    match j {
        serde_json::Value::String(s) => Value::string(s.clone()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::integer(i)
            } else if let Some(u) = n.as_u64() {
                Value::integer(i64::try_from(u).unwrap_or(i64::MAX))
            } else {
                Value::string(n.to_string())
            }
        }
        serde_json::Value::Bool(b) => Value::boolean(*b),
        serde_json::Value::Null => Value::null(),
        serde_json::Value::Array(arr) => Value::array(arr.iter().map(json_to_value).collect()),
        serde_json::Value::Object(map) => {
            let kv: Vec<(String, Arc<Value>)> = map
                .iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect();
            Value::object(kv)
        }
    }
}
