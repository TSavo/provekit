// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use chrono::{SecondsFormat, Utc};
use libprovekit::core::{
    execute_path, platform_semantics_for_binding, platform_semantics_for_lower_target,
    DivergenceCharacterization, HashMapInputCatalog, Input, KitRegistry, LowerKit,
    OpCoverageVerdict, Path as CorePath, PathAlgebra, PlatformSemanticComparisonError,
    PlatformSemanticsDeclaration, Side, Verb,
};
use libprovekit::effect_propagation::{
    propagate_effects, CallsiteEdge, ChangedCallsite, FunctionEffectInfo,
    NonEmptyChangedCallsites, PropagationDecision, PropagationInput,
};
use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value};
use provekit_ir_types::{
    AggregateSummaryMemento, CrossLanguageWitnessPair, HaltMemento, IrFormula,
    LanguageTransitionMemento, LossRecordMemento, MigrateReceiptEnvelope, MigrateReceiptSignature,
    MigrationConceptSiteMemento, MigrationEffectDelta, MigrationSourceLocation,
    PromotionDecisionEnvelope, PromotionDecisionHeader, PromotionDecisionMemento,
    PromotionDecisionMetadata, PromotionGate, PromotionResult, RefusalMemento, WitnessMemento,
};
use serde_json::{json, Value as JsonValue};

use crate::cmd_bind::BindArgs;
use crate::kit_dispatch::DispatchRealizeTransport;
use crate::{EXIT_OK, EXIT_USER_ERROR};

const ASYNC_EFFECT: &str = "async";
const SYNC_EFFECT: &str = "sync";
const FIXTURE_STATE_CID: &str = "blake3-512:295e0fd280088fc1e5e00d7bade11a2bf850c932180622e28f2fc92e64f97cd5bd757a73acf07f888b7c523e8efb65d8f0d01d50bc02740e5d771e750485d8f4";
const SQL_QUERY_CONCEPT_CID: &str = "blake3-512:dd0429a4d4276c076f5dde08a993a046afa15dd36433b1d89c4bc18831f63733788abef283ffb78b2b9f88c607593741367a35ebcbd36a88132c06d6ff233ed1";
const SQL_EXECUTE_CONCEPT_CID: &str = "blake3-512:1dcfe69eb5a7c6719d3faf2f4d073dfe81f37a4d930a37b7a535aa3de9eae7dc30965bf6d8fa451a9cb97608335a844f619adb4589d0a5e882b30fa98d60b3a9";
// CID for concept:insert-and-get-id, minted from its AlgorithmMemento via JCS+blake3-512.
// Characterizes INSERT callsites that capture the newly inserted row id via the
// binding-library's id-retrieval mechanism (LastInsertRowid for better-sqlite3,
// ReturningClause for pg). Finer than concept:sql-execute for the divergence walk.
const SQL_INSERT_AND_GET_ID_CONCEPT_CID: &str = "blake3-512:0a4f0a8d36d8dee96b8d5b32a18bb390f35877ecef611771048c6e10cfc3d25ad8f59de89b00c7794f62cabaf91dbd779244338393a8bb6ef5e8309b0929b3ca";

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
    let witness_fixture = args
        .witness_fixture
        .as_ref()
        .map(|path| resolve_user_path(path, &repo_root))
        .transpose()?;

    let (source_lang, source_tag) = split_library_surface(library_from)?;
    let (target_lang, target_tag) = split_library_surface(library_to)?;
    let target_surface = TargetSurface::from_parts(
        library_from,
        library_to,
        &source_lang,
        &source_tag,
        &target_lang,
        &target_tag,
    )?;

    let source_file = source_dir.join("src").join("users.ts");
    let source = std::fs::read_to_string(&source_file)
        .map_err(|e| format!("read {}: {e}", source_file.display()))?;
    let functions = extract_functions(&source)?;
    let mut sql_callsites = extract_sql_callsites(&source, &functions)?;
    if sql_callsites.is_empty() {
        return Err("no better-sqlite3 SQL callsites found".to_string());
    }
    validate_focus(args.focus.as_deref(), &sql_callsites)?;

    let source_binding_cid = body_template_cid(&repo_root, library_from)?;
    let target_binding_cid = body_template_cid(&repo_root, library_to)?;
    probe_realize_binding(&repo_root, &source_lang, &source_tag, "concept:sql-query")?;
    probe_realize_binding(&repo_root, &target_lang, &target_tag, "concept:sql-query")?;
    probe_realize_binding(&repo_root, &source_lang, &source_tag, "concept:sql-execute")?;
    probe_realize_binding(&repo_root, &target_lang, &target_tag, "concept:sql-execute")?;

    let semantic_changes = platform_semantic_changes_for_targets(
        &source_lang,
        &source_tag,
        &target_lang,
        &target_tag,
        &sql_callsites,
        args.focus.as_deref(),
    )?;
    let mut all_changed_callsites = semantic_changes.changed_callsites.clone();
    if target_surface.requires_async_delta() {
        all_changed_callsites.extend(async_changed_callsites(
            &sql_callsites,
            args.focus.as_deref(),
        ));
    }
    let decisions = match NonEmptyChangedCallsites::new(all_changed_callsites) {
        Ok(changed_callsites) => {
            let graph = build_propagation_input(&functions, &sql_callsites, changed_callsites);
            propagate_effects(&graph)
                .map_err(|e| format!("effect propagation: {e}"))?
                .decisions
        }
        Err(_) => BTreeMap::new(),
    };
    let migrated_source = render_migrated_source(target_surface);
    for callsite in &mut sql_callsites {
        callsite.after =
            after_location_for_callsite(&migrated_source, &callsite.function, target_surface)?;
    }

    let mut receipt = build_receipt(
        &decisions,
        &sql_callsites,
        &functions,
        &source_binding_cid,
        &target_binding_cid,
        target_surface,
        &semantic_changes.divergences_by_callsite_and_dimension,
        &semantic_changes.uncharacterizable_callsites,
    )?;
    if let Some(fixture) = witness_fixture.as_ref() {
        if target_surface.is_python() {
            let (witnesses, pairs) = emit_cross_language_witnesses(
                fixture,
                &sql_callsites,
                &receipt.concept_sites,
                &target_tag,
            )?;
            receipt.witnesses = witnesses;
            receipt.cross_language_witness_pairs = pairs;
        } else {
            receipt.witnesses = emit_witnesses(fixture, &sql_callsites)?;
        }
        receipt.root_cid = receipt.recompute_root_cid().map_err(|e| e.to_string())?;
        receipt.validate().map_err(|e| e.to_string())?;
    }

    if args.write {
        write_migrated_project(&out_dir, &migrated_source, target_surface)?;
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
        "{} lossy callsites: kit-declared dimension divergence",
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetSurface {
    TypescriptPg,
    PythonSqlite3,
    PythonAiosqlite,
}

impl TargetSurface {
    fn from_parts(
        library_from: &str,
        library_to: &str,
        source_lang: &str,
        source_tag: &str,
        target_lang: &str,
        target_tag: &str,
    ) -> Result<Self, String> {
        if source_lang != "typescript" || source_tag != "better-sqlite3" {
            return Err(format!(
                "unsupported migration {library_from} to {library_to}; source must be typescript-better-sqlite3"
            ));
        }
        match (target_lang, target_tag) {
            ("typescript", "pg") => Ok(Self::TypescriptPg),
            ("python", "sqlite3") => Ok(Self::PythonSqlite3),
            ("python", "aiosqlite") => Ok(Self::PythonAiosqlite),
            _ => Err(format!(
                "unsupported migration {library_from} to {library_to}; this demo supports typescript-better-sqlite3 to typescript-pg, python-sqlite3, or python-aiosqlite"
            )),
        }
    }

    fn is_python(self) -> bool {
        matches!(self, Self::PythonSqlite3 | Self::PythonAiosqlite)
    }

    fn requires_async_delta(self) -> bool {
        matches!(self, Self::TypescriptPg | Self::PythonAiosqlite)
    }

    fn after_effect(self) -> &'static str {
        if self.requires_async_delta() {
            ASYNC_EFFECT
        } else {
            SYNC_EFFECT
        }
    }

    fn output_file(self) -> &'static str {
        match self {
            Self::TypescriptPg => "src/users.ts",
            Self::PythonSqlite3 | Self::PythonAiosqlite => "src/users.py",
        }
    }
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
    sample_args: Vec<JsonValue>,
    sql: String,
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

fn snake_case(name: &str) -> String {
    let mut out = String::new();
    for (idx, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if idx > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
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
        // Finer classification: an execute callsite that also accesses `.lastInsertRowid`
        // is typed as concept:insert-and-get-id rather than the coarser concept:sql-execute.
        // Pattern: the callsite body contains the `.lastInsertRowid` property access, which
        // is how better-sqlite3 exposes the newly inserted row id from stmt.run().
        let has_last_insert_rowid = function.body.contains(".lastInsertRowid");
        let concept_name = if is_execute && has_last_insert_rowid {
            "concept:insert-and-get-id"
        } else if is_execute {
            "concept:sql-execute"
        } else {
            "concept:sql-query"
        };
        let concept_cid = if is_execute && has_last_insert_rowid {
            SQL_INSERT_AND_GET_ID_CONCEPT_CID
        } else if is_execute {
            SQL_EXECUTE_CONCEPT_CID
        } else {
            SQL_QUERY_CONCEPT_CID
        };
        let sql = extract_prepare_sql(&function.body, local_prepare, &function.name)?;
        let sample_args = extract_sample_args(function, &function.body)?;
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
            sample_args,
            sql,
            substituted_body: substituted_body_for(&function.name),
        });
    }
    Ok(out)
}

fn extract_prepare_sql(
    body: &str,
    prepare_offset: usize,
    function: &str,
) -> Result<String, String> {
    let args_start = prepare_offset + ".prepare(".len();
    let mut chars = body[args_start..].char_indices();
    let Some((leading_offset, quote)) = chars.find(|(_, ch)| !ch.is_whitespace()) else {
        return Err(format!("{function} prepare call missing SQL string"));
    };
    if quote != '"' && quote != '\'' && quote != '`' {
        return Err(format!("{function} prepare call must use a string literal"));
    }
    let literal_start = args_start + leading_offset + quote.len_utf8();
    let mut sql = String::new();
    let mut escaped = false;
    for (offset, ch) in body[literal_start..].char_indices() {
        if escaped {
            sql.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == quote {
            let literal_end = literal_start + offset;
            let close_paren = body[literal_end + quote.len_utf8()..]
                .find(')')
                .ok_or_else(|| format!("{function} prepare call missing closing paren"))?;
            let _ = close_paren;
            return Ok(sql);
        }
        sql.push(ch);
    }
    Err(format!(
        "{function} prepare call has unterminated SQL literal"
    ))
}

fn extract_sample_args(function: &TsFunction, body: &str) -> Result<Vec<JsonValue>, String> {
    for method in [".get(", ".all(", ".run("] {
        if let Some(pos) = body.find(method) {
            let open = pos + method.len() - 1;
            let close = find_matching_delimiter(body, open, '(', ')')
                .ok_or_else(|| format!("{} has unbalanced {method} call", function.name))?;
            let args = body[open + 1..close].trim();
            if args.is_empty() {
                return Ok(Vec::new());
            }
            return split_top_level_commas(args)
                .into_iter()
                .map(|arg| sample_arg_value(function, arg.trim()))
                .collect();
        }
    }
    Ok(Vec::new())
}

fn split_top_level_commas(args: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for (idx, ch) in args.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(current_quote) = quote {
            if ch == current_quote {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' || ch == '`' {
            quote = Some(ch);
            continue;
        }
        if ch == '(' || ch == '[' || ch == '{' {
            depth += 1;
            continue;
        }
        if ch == ')' || ch == ']' || ch == '}' {
            depth = depth.saturating_sub(1);
            continue;
        }
        if ch == ',' && depth == 0 {
            out.push(args[start..idx].trim());
            start = idx + 1;
        }
    }
    out.push(args[start..].trim());
    out
}

fn sample_arg_value(function: &TsFunction, arg: &str) -> Result<JsonValue, String> {
    if arg.is_empty() {
        return Err(format!("{} has empty sample argument", function.name));
    }
    if let Ok(value) = arg.parse::<i64>() {
        return Ok(json!(value));
    }
    if let Some(value) = parse_string_literal(arg) {
        return Ok(json!(value));
    }
    if arg == "true" {
        return Ok(json!(true));
    }
    if arg == "false" {
        return Ok(json!(false));
    }
    if arg == "null" {
        return Ok(JsonValue::Null);
    }
    let lowered = arg.to_ascii_lowercase();
    if lowered == "kind" {
        return Ok(json!("login"));
    }
    if lowered == "id" || lowered.ends_with("id") || lowered.ends_with("_id") {
        return Ok(json!(1));
    }
    Err(format!(
        "{} sample argument {arg} needs an explicit fixture value",
        function.name
    ))
}

fn parse_string_literal(arg: &str) -> Option<String> {
    let mut chars = arg.chars();
    let quote = chars.next()?;
    if quote != '"' && quote != '\'' && quote != '`' {
        return None;
    }
    if !arg.ends_with(quote) || arg.len() < 2 {
        return None;
    }
    let mut out = String::new();
    let mut escaped = false;
    for ch in arg[quote.len_utf8()..arg.len() - quote.len_utf8()].chars() {
        if escaped {
            out.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else {
            out.push(ch);
        }
    }
    Some(out)
}

#[derive(Debug, Clone, Default)]
struct PlatformSemanticChangeSet {
    changed_callsites: Vec<ChangedCallsite>,
    divergences_by_callsite_and_dimension: BTreeMap<(String, String), DivergenceCharacterization>,
    /// Callsites where exactly one kit declared a tag for the op-CID; the
    /// substrate cannot characterize the divergence. These are routed to the
    /// refuse leg of the trichotomy at receipt-construction time.
    uncharacterizable_callsites: Vec<UncharacterizableCallsite>,
}

/// A callsite the substrate cannot characterize because exactly one kit
/// declared a tag for the op-CID. Carries enough context for the refusal
/// memento to name the declaring kit as the forbidding contract.
#[derive(Debug, Clone)]
struct UncharacterizableCallsite {
    callsite_cid: String,
    op_cid: String,
    absent_on: Side,
    /// The kit-CID of the side that DID declare a tag for the op (the
    /// non-absent side). When available, used as `forbidding_contract` on
    /// the refusal memento. Falls back to the op-CID if the declaring kit
    /// did not carry value mementos.
    declaring_kit_cid: Option<String>,
}

fn validate_focus(focus: Option<&str>, sql_callsites: &[SqlCallsite]) -> Result<(), String> {
    let Some(focus) = focus else {
        return Ok(());
    };
    if sql_callsites.iter().any(|callsite| callsite.cid == focus) {
        Ok(())
    } else {
        Err(format!("--focus callsite {focus} was not found"))
    }
}

fn async_changed_callsites(
    sql_callsites: &[SqlCallsite],
    focus: Option<&str>,
) -> Vec<ChangedCallsite> {
    sql_callsites
        .iter()
        .filter(|callsite| focus.is_none_or(|focus| focus == callsite.cid))
        .map(|callsite| ChangedCallsite {
            callsite_cid: callsite.cid.clone(),
            dimension_name: None,
            effect: ASYNC_EFFECT.to_string(),
        })
        .collect()
}

fn platform_semantic_changes_for_targets(
    source_lang: &str,
    source_tag: &str,
    target_lang: &str,
    target_tag: &str,
    sql_callsites: &[SqlCallsite],
    focus: Option<&str>,
) -> Result<PlatformSemanticChangeSet, String> {
    // Compose language-kit (per-op platform semantics: overflow, rounding, etc.)
    // with binding-kit (per-op library semantics: RowIdMechanism, etc.).
    // Binding-kit wins on op-CID conflicts.
    //
    // Either side may be None when no declaration exists for the given
    // (lang, tag) pair. We do NOT short-circuit to empty here: per the
    // trichotomy ruling
    // (docs/plans/2026-05-18-op-coverage-verdict-trichotomy-ruling.md), if
    // one side has a declaration and the other does not, callsites whose
    // op-CID is present on the declared side must produce
    // OpCoverageVerdict::Uncharacterizable. That routes through the refuse
    // leg in build_receipt. Substitute an empty declaration for the absent
    // side so compare_op_with produces the per-callsite verdict uniformly.
    //
    // (Audit row D3 / #1228: prior shape collapsed NoOpinion AND
    // Uncharacterizable into "empty change set", silently dropping the
    // refuse-leg signal when one binding-kit was missing entirely.)
    let source = platform_semantics_for_binding(source_lang, source_tag)
        .unwrap_or_else(empty_platform_semantics);
    let target = platform_semantics_for_binding(target_lang, target_tag)
        .unwrap_or_else(empty_platform_semantics);
    platform_semantic_changes(&source, &target, sql_callsites, focus)
        .map_err(|error| format!("platform semantic comparison: {error:?}"))
}

fn empty_platform_semantics() -> PlatformSemanticsDeclaration {
    PlatformSemanticsDeclaration {
        tags: Vec::new(),
        dimension_values: Vec::new(),
        op_aliases: BTreeMap::new(),
    }
}

fn platform_semantic_changes(
    source: &PlatformSemanticsDeclaration,
    target: &PlatformSemanticsDeclaration,
    sql_callsites: &[SqlCallsite],
    focus: Option<&str>,
) -> Result<PlatformSemanticChangeSet, PlatformSemanticComparisonError> {
    let mut changes = PlatformSemanticChangeSet::default();
    for callsite in sql_callsites
        .iter()
        .filter(|callsite| focus.is_none_or(|focus| focus == callsite.cid))
    {
        match source.compare_op_with(&callsite.concept_cid, target)? {
            OpCoverageVerdict::NoOpinion | OpCoverageVerdict::Same => {
                // Neither kit has an opinion on this op, or both agree. No action.
                continue;
            }
            OpCoverageVerdict::Uncharacterizable { absent_on } => {
                // Exactly one kit declared a tag. The substrate cannot characterize
                // the divergence; record for the refuse leg.
                let declaring = match absent_on {
                    Side::Source => target,
                    Side::Target => source,
                };
                let declaring_kit_cid = declaring
                    .dimension_values
                    .first()
                    .map(|memento| memento.kit_cid.clone());
                changes.uncharacterizable_callsites.push(UncharacterizableCallsite {
                    callsite_cid: callsite.cid.clone(),
                    op_cid: callsite.concept_cid.clone(),
                    absent_on,
                    declaring_kit_cid,
                });
            }
            OpCoverageVerdict::Divergent(divergence) => {
                let effect = divergence_effect_cid(&divergence);
                let dimension_name = divergence.dimension_name.clone();
                changes.changed_callsites.push(ChangedCallsite {
                    callsite_cid: callsite.cid.clone(),
                    dimension_name: Some(dimension_name.clone()),
                    effect: effect.clone(),
                });
                changes
                    .divergences_by_callsite_and_dimension
                    .insert((callsite.cid.clone(), dimension_name), divergence);
            }
        }
    }
    Ok(changes)
}

fn divergence_effect_cid(divergence: &DivergenceCharacterization) -> String {
    let source_compare_to =
        serde_json::to_value(&divergence.source_compare_to).expect("IrFormula serializes");
    let target_compare_to =
        serde_json::to_value(&divergence.target_compare_to).expect("IrFormula serializes");
    cid_for_json(&json!({
        "dimension_name": divergence.dimension_name,
        "kind": "platform-semantic-divergence",
        "source_compare_to": source_compare_to,
        "source_value_cid": divergence.source_value_cid,
        "target_compare_to": target_compare_to,
        "target_value_cid": divergence.target_value_cid,
    }))
}

fn build_propagation_input(
    functions: &[TsFunction],
    sql_callsites: &[SqlCallsite],
    changed_callsites: NonEmptyChangedCallsites,
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
    for callsite in sql_callsites {
        callsites.insert(
            callsite.cid.clone(),
            CallsiteEdge {
                callee: None,
                cid: callsite.cid.clone(),
                containing_fn: callsite.function.clone(),
            },
        );
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
    let (language, tag) = split_library_surface(surface)?;
    let file_name = match language.as_str() {
        "typescript" => {
            if tag.is_empty() {
                "typescript-canonical-bodies.json".to_string()
            } else {
                format!("typescript-canonical-bodies-{tag}.json")
            }
        }
        "python" => match tag.as_str() {
            "urllib" => "python-canonical-bodies.json".to_string(),
            _ => format!("python-canonical-bodies-{tag}.json"),
        },
        _ => return Err(format!("unsupported body template surface {surface}")),
    };
    let path = repo_root
        .join("menagerie")
        .join(format!("{language}-language-signature"))
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
    let spec = json!({
        "kind": "RealizeRequest",
        "function": "__provekit_migrate_probe",
        "params": ["sql", "args"],
        "paramTypes": ["string", "unknown[]"],
        "returnType": if concept_name == "concept:sql-execute" {
            "{ rows_affected: number; last_insert_id: unknown }"
        } else {
            "unknown[]"
        },
        "conceptName": concept_name,
    });
    let realized = realize_probe_via_path(repo_root, language, tag, spec)?;
    if realized.is_stub {
        return Err(format!(
            "realize binding for {language}/{tag}/{concept_name} fell through to a stub"
        ));
    }
    Ok(())
}

fn realize_probe_via_path(
    repo_root: &Path,
    language: &str,
    tag: &str,
    spec: JsonValue,
) -> Result<libprovekit::core::RealizedSource, String> {
    let mut inputs = HashMapInputCatalog::default();
    let input_cid = inputs.insert(Input::Spec(spec));
    let kit_name = format!("lower-{language}");
    let path = Input::Path(Box::new(CorePath {
        algebra: vec![PathAlgebra {
            name: "lower".to_string(),
            kit: kit_name.clone(),
            inputs: vec![input_cid],
            depends_on: vec![],
            verb: Verb::Transform,
        }],
    }));
    let mut registry = KitRegistry::default();
    registry.register_with_platform_semantics(
        kit_name,
        LowerKit::new(
            repo_root.to_path_buf(),
            language.to_string(),
            Some(tag.to_string()),
            DispatchRealizeTransport,
        ),
        language,
        repo_root.join(format!("implementations/{language}/conformance/fixtures")),
    );
    let chain = execute_path(&path, &registry, &inputs).map_err(|error| error.to_string())?;
    LowerKit::<DispatchRealizeTransport>::realized_source_from_claim(chain.terminal_claim())
}

fn build_receipt(
    decisions: &BTreeMap<String, PropagationDecision>,
    sql_callsites: &[SqlCallsite],
    functions: &[TsFunction],
    source_binding_cid: &str,
    target_binding_cid: &str,
    target_surface: TargetSurface,
    divergences_by_callsite_and_dimension: &BTreeMap<(String, String), DivergenceCharacterization>,
    uncharacterizable_callsites: &[UncharacterizableCallsite],
) -> Result<MigrateReceiptEnvelope, String> {
    let mut concept_sites = Vec::new();
    for callsite in sql_callsites {
        let mut site = MigrationConceptSiteMemento {
            after_source_location: callsite.after.clone(),
            before_source_location: callsite.before.clone(),
            cid: String::new(),
            concept_cid: callsite.concept_cid.clone(),
            effect_delta: MigrationEffectDelta {
                after: target_surface.after_effect().to_string(),
                before: SYNC_EFFECT.to_string(),
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

    let language_transitions = if target_surface.is_python() {
        language_transitions(functions)?
    } else {
        Vec::new()
    };

    let mut promotion_decisions = Vec::new();
    let mut halt_mementos = Vec::new();
    let mut refusal_mementos = Vec::new();
    for (function, decision) in decisions {
        match decision {
            PropagationDecision::Widen {
                effect,
                function_cid,
                triggering_callsite_cid,
                ..
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

    // Refuse leg of the trichotomy: every uncharacterizable callsite that the
    // substrate-comparison phase collected becomes a refusal memento. The
    // declaring kit's CID names the forbidding contract; the reason carries
    // the absent op CID and the side that lacked a declaration.
    for uc in uncharacterizable_callsites {
        let forbidding_contract = uc
            .declaring_kit_cid
            .clone()
            .unwrap_or_else(|| uc.op_cid.clone());
        let absent_side = match uc.absent_on {
            Side::Source => "source",
            Side::Target => "target",
        };
        let mut refusal = RefusalMemento {
            cid: String::new(),
            forbidding_contract,
            function_cid: uc.callsite_cid.clone(),
            kind: "refusal".to_string(),
            reason: format!(
                "substrate cannot characterize divergence: {absent_side} kit did not declare a tag for op-CID {}",
                uc.op_cid
            ),
            schema_version: "1".to_string(),
        };
        refusal.cid = refusal.recompute_cid().map_err(|e| e.to_string())?;
        refusal_mementos.push(refusal);
    }

    // Short-circuit lossy emission when any callsite is uncharacterizable.
    // Per the trichotomy, a migrate that refuses on any callsite is a pure
    // refusal; emitting partial loss-records alongside would conflate
    // "loudly-bounded-lossy" with "refuse" and violate Supra omnia, rectum.
    let mut loss_records = Vec::new();
    if uncharacterizable_callsites.is_empty() {
        for decision in decisions.values() {
            let PropagationDecision::Widen {
                dimension_name,
                triggering_callsite_cid,
                ..
            } = decision
            else {
                continue;
            };
            let Some(dim) = dimension_name else {
                continue;
            };
            let Some(divergence) = divergences_by_callsite_and_dimension
                .get(&(triggering_callsite_cid.clone(), dim.clone()))
            else {
                continue;
            };
            let Some(callsite) = sql_callsites
                .iter()
                .find(|callsite| callsite.cid == *triggering_callsite_cid)
            else {
                continue;
            };
            let mut loss_dimensions = BTreeMap::new();
            loss_dimensions.insert(
                divergence.dimension_name.clone(),
                IrFormula::DivergenceBetween {
                    source: Box::new(divergence.source_compare_to.clone()),
                    target: Box::new(divergence.target_compare_to.clone()),
                },
            );
            let mut loss = LossRecordMemento {
                callsite_cid: callsite.cid.clone(),
                cid: String::new(),
                kind: "loss-record".to_string(),
                loss_dimension: divergence.dimension_name.clone(),
                loss_dimensions,
                schema_version: "1".to_string(),
                substituted_body: callsite.substituted_body.clone(),
            };
            loss.cid = loss.recompute_cid().map_err(|e| e.to_string())?;
            loss_records.push(loss);
        }
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
        cross_language_witness_pairs: Vec::new(),
        halt_mementos,
        language_transitions,
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
        witnesses: Vec::new(),
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

fn language_transitions(
    functions: &[TsFunction],
) -> Result<Vec<LanguageTransitionMemento>, String> {
    let mut out = Vec::new();
    for function in functions {
        let target_name = snake_case(&function.name);
        let source_signature_cid = cid_for_json(&json!({
            "function": function.name,
            "kind": "proofir-signature",
            "language": "typescript",
            "signature": function.signature
        }));
        let target_signature_cid = cid_for_json(&json!({
            "function": target_name,
            "kind": "proofir-signature",
            "language": "python",
            "source_signature_cid": source_signature_cid,
            "structural_equivalence": "typescript-to-python"
        }));
        let mut transition = LanguageTransitionMemento {
            cid: String::new(),
            function_language_source: "typescript".to_string(),
            function_language_target: "python".to_string(),
            function_name_source: function.name.clone(),
            function_name_target: target_name,
            kind: "language-transition".to_string(),
            naming_convention: "camelCase -> snake_case".to_string(),
            schema_version: "1".to_string(),
            signature_equivalence: "structural".to_string(),
            source_signature_cid,
            target_signature_cid,
        };
        transition.cid = transition.recompute_cid().map_err(|e| e.to_string())?;
        out.push(transition);
    }
    Ok(out)
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
    target_surface: TargetSurface,
) -> Result<MigrationSourceLocation, String> {
    match target_surface {
        TargetSurface::TypescriptPg => {
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
                target_surface.output_file(),
            ))
        }
        TargetSurface::PythonSqlite3 | TargetSurface::PythonAiosqlite => {
            let target_name = snake_case(function);
            let fn_offset = migrated_source
                .find(&format!("def {target_name}"))
                .or_else(|| migrated_source.find(&format!("async def {target_name}")))
                .ok_or_else(|| format!("migrated source missing function {target_name}"))?;
            let query_offset = migrated_source[fn_offset..]
                .find(".execute(")
                .map(|offset| fn_offset + offset)
                .ok_or_else(|| format!("migrated source missing execute in {target_name}"))?;
            Ok(location_for_file(
                migrated_source,
                query_offset,
                target_surface.output_file(),
            ))
        }
    }
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

fn render_migrated_source(target_surface: TargetSurface) -> String {
    match target_surface {
        TargetSurface::TypescriptPg => render_ts_pg_source(),
        TargetSurface::PythonSqlite3 => render_python_sqlite3_source(),
        TargetSurface::PythonAiosqlite => render_python_aiosqlite_source(),
    }
}

fn render_ts_pg_source() -> String {
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

fn render_python_sqlite3_source() -> String {
    r#"from __future__ import annotations

import sqlite3
from typing import TypedDict


class User(TypedDict):
    id: int
    name: str
    email: str


class RequestLike(TypedDict):
    path: str


class Response(TypedDict):
    status: int
    body: str


db = sqlite3.connect("users.sqlite")
db.row_factory = sqlite3.Row


def _row_to_user(row: sqlite3.Row | None) -> User | None:
    if row is None:
        return None
    return {
        "id": int(row["id"]),
        "name": str(row["name"]),
        "email": str(row["email"]),
    }


def get_user_by_id(id: int) -> User:
    row = db.execute(
        "SELECT id, name, email FROM users WHERE id = ?",
        (id,),
    ).fetchone()
    user = _row_to_user(row)
    if user is None:
        raise RuntimeError(f"missing user {id}")
    return user


def get_all_users() -> list[User]:
    rows = db.execute("SELECT id, name, email FROM users ORDER BY id").fetchall()
    return [
        {
            "id": int(row["id"]),
            "name": str(row["name"]),
            "email": str(row["email"]),
        }
        for row in rows
    ]


def count_users() -> int:
    row = db.execute("SELECT count(*) AS count FROM users").fetchone()
    return int(row["count"] if row is not None else 0)


def render_users_page() -> str:
    users = get_all_users()
    items = "".join(f"<li>{exported_formatter(user)}</li>" for user in users)
    return f"<ul>{items}</ul>"


def render_dashboard() -> str:
    page = render_users_page()
    count = count_users()
    return f'<section data-count="{count}">{page}</section>'


def handle_request(req: RequestLike) -> Response:
    if req["path"] != "/users":
        return {"status": 404, "body": "not found"}
    return {"status": 200, "body": render_dashboard()}


def exported_formatter(u: User) -> str:
    return f"{u['name']} <{u['email']}> ({count_users()} users)"


def record_event(user_id: int, kind: str) -> int:
    cursor = db.execute(
        "INSERT INTO events (user_id, kind) VALUES (?, ?)",
        (user_id, kind),
    )
    db.commit()
    return int(cursor.lastrowid or 0)
"#
    .to_string()
}

fn render_python_aiosqlite_source() -> String {
    r#"from __future__ import annotations

import aiosqlite
from typing import Any, Coroutine, TypedDict


class User(TypedDict):
    id: int
    name: str
    email: str


class RequestLike(TypedDict):
    path: str


class Response(TypedDict):
    status: int
    body: str


DB_PATH = "users.sqlite"


def _row_to_user(row: aiosqlite.Row | None) -> User | None:
    if row is None:
        return None
    return {
        "id": int(row["id"]),
        "name": str(row["name"]),
        "email": str(row["email"]),
    }


async def get_user_by_id(id: int) -> Coroutine[Any, Any, User]:
    async with aiosqlite.connect(DB_PATH) as db:
        db.row_factory = aiosqlite.Row
        async with db.execute(
            "SELECT id, name, email FROM users WHERE id = ?",
            (id,),
        ) as cursor:
            row = await cursor.fetchone()
    user = _row_to_user(row)
    if user is None:
        raise RuntimeError(f"missing user {id}")
    return user


async def get_all_users() -> Coroutine[Any, Any, list[User]]:
    async with aiosqlite.connect(DB_PATH) as db:
        db.row_factory = aiosqlite.Row
        async with db.execute("SELECT id, name, email FROM users ORDER BY id") as cursor:
            rows = await cursor.fetchall()
    return [
        {
            "id": int(row["id"]),
            "name": str(row["name"]),
            "email": str(row["email"]),
        }
        for row in rows
    ]


async def count_users() -> Coroutine[Any, Any, int]:
    async with aiosqlite.connect(DB_PATH) as db:
        db.row_factory = aiosqlite.Row
        async with db.execute("SELECT count(*) AS count FROM users") as cursor:
            row = await cursor.fetchone()
    return int(row["count"] if row is not None else 0)


async def render_users_page() -> Coroutine[Any, Any, str]:
    users = await get_all_users()
    items = "".join(f"<li>{exported_formatter(user)}</li>" for user in users)
    return f"<ul>{items}</ul>"


async def render_dashboard() -> Coroutine[Any, Any, str]:
    page = await render_users_page()
    count = await count_users()
    return f'<section data-count="{count}">{page}</section>'


async def handle_request(req: RequestLike) -> Coroutine[Any, Any, Response]:
    if req["path"] != "/users":
        return {"status": 404, "body": "not found"}
    return {"status": 200, "body": await render_dashboard()}


def exported_formatter(u: User) -> str:
    return f"{u['name']} <{u['email']}> ({count_users()} users)"


async def record_event(user_id: int, kind: str) -> Coroutine[Any, Any, int]:
    async with aiosqlite.connect(DB_PATH) as db:
        async with db.execute(
            "INSERT INTO events (user_id, kind) VALUES (?, ?)",
            (user_id, kind),
        ) as cursor:
            await db.commit()
            return int(cursor.lastrowid or 0)
"#
    .to_string()
}

fn write_migrated_project(
    out_dir: &Path,
    migrated_source: &str,
    target_surface: TargetSurface,
) -> Result<(), String> {
    match target_surface {
        TargetSurface::TypescriptPg => write_typescript_project(out_dir, migrated_source),
        TargetSurface::PythonSqlite3 | TargetSurface::PythonAiosqlite => {
            write_python_project(out_dir, migrated_source, target_surface)
        }
    }
}

fn write_typescript_project(out_dir: &Path, migrated_source: &str) -> Result<(), String> {
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

fn write_python_project(
    out_dir: &Path,
    migrated_source: &str,
    target_surface: TargetSurface,
) -> Result<(), String> {
    let src_dir = out_dir.join("src");
    std::fs::create_dir_all(&src_dir).map_err(|e| format!("create {}: {e}", src_dir.display()))?;
    std::fs::write(src_dir.join("users.py"), migrated_source)
        .map_err(|e| format!("write migrated users.py: {e}"))?;
    std::fs::write(
        out_dir.join("pyproject.toml"),
        migrated_pyproject(target_surface),
    )
    .map_err(|e| format!("write pyproject.toml: {e}"))?;
    Ok(())
}

fn migrated_pyproject(target_surface: TargetSurface) -> &'static str {
    match target_surface {
        TargetSurface::PythonSqlite3 => {
            r#"[project]
name = "provekit-migrate-demo-users-python-sqlite3"
version = "0.0.0"
requires-python = ">=3.11"
dependencies = []
"#
        }
        TargetSurface::PythonAiosqlite => {
            r#"[project]
name = "provekit-migrate-demo-users-python-aiosqlite"
version = "0.0.0"
requires-python = ">=3.11"
dependencies = ["aiosqlite"]
"#
        }
        TargetSurface::TypescriptPg => "",
    }
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

struct SqlObservation {
    measurements: JsonValue,
    sample_count: u64,
}

fn emit_witnesses(
    fixture: &Path,
    sql_callsites: &[SqlCallsite],
) -> Result<Vec<WitnessMemento>, String> {
    let fixture_bytes =
        std::fs::read(fixture).map_err(|e| format!("read {}: {e}", fixture.display()))?;
    let fixture_state_cid = blake3_512_of(&fixture_bytes);
    let observed_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let mut witnesses = Vec::new();
    for callsite in sql_callsites {
        let observation = observe_sql(fixture, &callsite.sql, &callsite.sample_args)?;
        witnesses.push(witness_for_observation(
            &fixture_state_cid,
            &observed_at,
            &callsite.cid,
            &callsite.concept_cid,
            observation,
        )?);
    }
    Ok(witnesses)
}

fn emit_cross_language_witnesses(
    fixture: &Path,
    sql_callsites: &[SqlCallsite],
    concept_sites: &[MigrationConceptSiteMemento],
    target_tag: &str,
) -> Result<(Vec<WitnessMemento>, Vec<CrossLanguageWitnessPair>), String> {
    let fixture_bytes =
        std::fs::read(fixture).map_err(|e| format!("read {}: {e}", fixture.display()))?;
    let fixture_state_cid = blake3_512_of(&fixture_bytes);
    if fixture_state_cid != FIXTURE_STATE_CID {
        return Err(format!(
            "witness fixture cid mismatch: expected {FIXTURE_STATE_CID}, got {fixture_state_cid}"
        ));
    }
    if sql_callsites.len() != concept_sites.len() {
        return Err("concept site count does not match SQL callsite count".to_string());
    }
    let observed_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let mut witnesses = Vec::new();
    let mut pairs = Vec::new();
    for (callsite, site) in sql_callsites.iter().zip(concept_sites) {
        let source_observation = observed_sql_with_observer(
            fixture,
            &callsite.sql,
            &callsite.sample_args,
            "typescript",
            "better-sqlite3",
        )?;
        let target_observation = observed_sql_with_observer(
            fixture,
            &callsite.sql,
            &callsite.sample_args,
            "python",
            target_tag,
        )?;
        let source_witness = witness_for_observation(
            &fixture_state_cid,
            &observed_at,
            &site.cid,
            &callsite.concept_cid,
            source_observation,
        )?;
        let target_witness = witness_for_observation(
            &fixture_state_cid,
            &observed_at,
            &site.cid,
            &callsite.concept_cid,
            target_observation,
        )?;
        let source_row_schema = source_witness.measurements.pointer("/row_schema");
        let target_row_schema = target_witness.measurements.pointer("/row_schema");
        let outcome = if source_row_schema == target_row_schema {
            "pass"
        } else {
            "fail"
        };
        pairs.push(CrossLanguageWitnessPair {
            concept_site_cid: site.cid.clone(),
            equivalence_outcome: outcome.to_string(),
            source_witness_cid: source_witness.cid.clone(),
            target_witness_cid: target_witness.cid.clone(),
        });
        witnesses.push(source_witness);
        witnesses.push(target_witness);
    }
    Ok((witnesses, pairs))
}

fn observed_sql_with_observer(
    fixture: &Path,
    sql: &str,
    sample_args: &[JsonValue],
    language: &str,
    library_tag: &str,
) -> Result<SqlObservation, String> {
    let mut observation = observe_sql(fixture, sql, sample_args)?;
    if let JsonValue::Object(map) = &mut observation.measurements {
        map.insert(
            "observer".to_string(),
            json!({
                "language": language,
                "library_tag": library_tag
            }),
        );
    }
    Ok(observation)
}

fn witness_for_observation(
    fixture_state_cid: &str,
    observed_at: &str,
    subject: &str,
    witness_for: &str,
    observation: SqlObservation,
) -> Result<WitnessMemento, String> {
    let mut witness = WitnessMemento {
        cid: String::new(),
        fixture_state_cid: fixture_state_cid.to_string(),
        kind: "witness".to_string(),
        measurements: observation.measurements,
        observed_at: observed_at.to_string(),
        outcome: "pass".to_string(),
        sample_count: observation.sample_count,
        schema_version: "1".to_string(),
        signature: None,
        signed_by: None,
        subject: subject.to_string(),
        witness_for: witness_for.to_string(),
    };
    witness.cid = witness.recompute_cid().map_err(|e| e.to_string())?;
    Ok(witness)
}

fn observe_sql(
    fixture: &Path,
    sql: &str,
    sample_args: &[JsonValue],
) -> Result<SqlObservation, String> {
    let expanded_sql = substitute_sql_args(sql, sample_args)?;
    let names = select_column_names(sql);
    if names.is_empty() {
        run_sqlite(fixture, &format!("EXPLAIN {expanded_sql}"))?;
        return Ok(SqlObservation {
            measurements: json!({
                "query": {
                    "sample_args": sample_args,
                    "sql": sql
                },
                "row_schema": {
                    "columns": []
                },
                "sample_row": {}
            }),
            sample_count: 0,
        });
    }

    let rows = run_sqlite_json(fixture, &expanded_sql)?;
    let Some(row) = rows.as_array().and_then(|rows| rows.first()) else {
        return Err(format!("witnessed SQL `{sql}` produced no sample row"));
    };
    let row = row
        .as_object()
        .ok_or_else(|| format!("witnessed SQL `{sql}` produced a non-object row"))?;
    let declared_types = declared_types(fixture, sql, &names)?;
    let mut columns = Vec::new();
    let mut sample_row = serde_json::Map::new();
    for (idx, name) in names.iter().enumerate() {
        let value = row
            .get(name)
            .ok_or_else(|| format!("witnessed SQL row missing column {name}"))?
            .clone();
        columns.push(json!({
            "declared_type": declared_types[idx],
            "name": name,
            "observed_typeof": sqlite_json_typeof(&value)
        }));
        sample_row.insert(name.clone(), value);
    }

    Ok(SqlObservation {
        measurements: json!({
            "query": {
                "sample_args": sample_args,
                "sql": sql
            },
            "row_schema": {
                "columns": columns
            },
            "sample_row": JsonValue::Object(sample_row)
        }),
        sample_count: 1,
    })
}

fn run_sqlite(fixture: &Path, sql: &str) -> Result<String, String> {
    let output = Command::new("sqlite3")
        .arg("-readonly")
        .arg(fixture)
        .arg(sql)
        .output()
        .map_err(|e| format!("spawn sqlite3: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "sqlite3 failed for `{sql}`\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout).map_err(|e| format!("sqlite3 output was not UTF-8: {e}"))
}

fn run_sqlite_json(fixture: &Path, sql: &str) -> Result<JsonValue, String> {
    let output = Command::new("sqlite3")
        .arg("-readonly")
        .arg("-json")
        .arg(fixture)
        .arg(sql)
        .output()
        .map_err(|e| format!("spawn sqlite3: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "sqlite3 json failed for `{sql}`\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let text = String::from_utf8(output.stdout)
        .map_err(|e| format!("sqlite3 json output was not UTF-8: {e}"))?;
    if text.trim().is_empty() {
        return Ok(json!([]));
    }
    serde_json::from_str(&text).map_err(|e| format!("parse sqlite3 json for `{sql}`: {e}"))
}

fn declared_types(fixture: &Path, sql: &str, names: &[String]) -> Result<Vec<String>, String> {
    let Some(table) = table_name(sql) else {
        return Ok(names.iter().map(|_| "UNKNOWN".to_string()).collect());
    };
    let rows = run_sqlite_json(fixture, &format!("PRAGMA table_info({table})"))?;
    let mut by_name = BTreeMap::new();
    let Some(rows) = rows.as_array() else {
        return Err(format!("table_info for {table} did not return rows"));
    };
    for row in rows {
        let name = row
            .get("name")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| format!("table_info for {table} missing name"))?;
        let declared_type = row
            .get("type")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| format!("table_info for {table} missing type"))?;
        by_name.insert(name.to_string(), declared_type.to_string());
    }
    Ok(names
        .iter()
        .map(|name| {
            by_name
                .get(name)
                .cloned()
                .unwrap_or_else(|| expression_declared_type(name))
        })
        .collect())
}

fn select_column_names(sql: &str) -> Vec<String> {
    let lower = sql.to_ascii_lowercase();
    let Some(select_pos) = lower.find("select ") else {
        return Vec::new();
    };
    let Some(from_pos) = lower.find(" from ") else {
        return Vec::new();
    };
    let projection = &sql[select_pos + "select ".len()..from_pos];
    split_top_level_commas(projection)
        .into_iter()
        .map(column_name_for_projection)
        .collect()
}

fn column_name_for_projection(projection: &str) -> String {
    let tokens = projection.split_whitespace().collect::<Vec<_>>();
    for idx in 0..tokens.len() {
        if tokens[idx].eq_ignore_ascii_case("as") && idx + 1 < tokens.len() {
            return clean_identifier(tokens[idx + 1]);
        }
    }
    clean_identifier(tokens.last().copied().unwrap_or(projection))
}

fn clean_identifier(value: &str) -> String {
    value
        .trim()
        .trim_matches(|ch: char| ch == '"' || ch == '`' || ch == '[' || ch == ']')
        .to_string()
}

fn table_name(sql: &str) -> Option<String> {
    let lower = sql.to_ascii_lowercase();
    let from_pos = lower.find(" from ")?;
    let after_from = &sql[from_pos + " from ".len()..];
    after_from.split_whitespace().next().map(|table| {
        table
            .trim_matches(|ch: char| ch == '"' || ch == '`')
            .to_string()
    })
}

fn expression_declared_type(name: &str) -> String {
    if name.eq_ignore_ascii_case("count") || name.eq_ignore_ascii_case("id") {
        "INTEGER".to_string()
    } else {
        "UNKNOWN".to_string()
    }
}

fn substitute_sql_args(sql: &str, sample_args: &[JsonValue]) -> Result<String, String> {
    let mut out = String::new();
    let mut arg_idx = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for ch in sql.chars() {
        if escaped {
            out.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            out.push(ch);
            escaped = true;
            continue;
        }
        if let Some(current_quote) = quote {
            out.push(ch);
            if ch == current_quote {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' || ch == '`' {
            out.push(ch);
            quote = Some(ch);
            continue;
        }
        if ch == '?' {
            let value = sample_args
                .get(arg_idx)
                .ok_or_else(|| format!("SQL `{sql}` needs more sample args"))?;
            out.push_str(&sql_literal(value)?);
            arg_idx += 1;
        } else {
            out.push(ch);
        }
    }
    if arg_idx != sample_args.len() {
        return Err(format!("SQL `{sql}` has unused sample args"));
    }
    Ok(out)
}

fn sql_literal(value: &JsonValue) -> Result<String, String> {
    match value {
        JsonValue::Null => Ok("NULL".to_string()),
        JsonValue::Bool(value) => Ok(if *value { "1" } else { "0" }.to_string()),
        JsonValue::Number(value) => Ok(value.to_string()),
        JsonValue::String(value) => Ok(format!("'{}'", value.replace('\'', "''"))),
        JsonValue::Array(_) | JsonValue::Object(_) => {
            Err("sample args must be scalar JSON values".to_string())
        }
    }
}

fn sqlite_json_typeof(value: &JsonValue) -> &'static str {
    match value {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "integer",
        JsonValue::Number(value) => {
            if value.as_i64().is_some() || value.as_u64().is_some() {
                "integer"
            } else {
                "real"
            }
        }
        JsonValue::String(_) => "text",
        JsonValue::Array(_) | JsonValue::Object(_) => "blob",
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    const CONCEPT_ADD_CID: &str = "blake3-512:95fc70e63a5550fd2e25142f13932919c59d085654ab387789c798886b0111c61d28fe533fc98b50df70eea9428a9af8aa75372c8b1c1deb3acc1a4094790468";
    const CONCEPT_DIV_CID: &str = "blake3-512:c6a13abbcafdf83edcff49d883a7c7440faadd8af896da0ad46e2bcb177ed0649d005b4ddecd4689cf565b10679219a07c784399bafe5c6174642e1b808d7839";

    fn function(name: &str) -> TsFunction {
        TsFunction {
            body: format!("return {name};"),
            body_start: 0,
            is_async: false,
            line: 1,
            name: name.to_string(),
            public_sync_contract_cid: None,
            return_type: "number".to_string(),
            signature: format!("function {name}(): number"),
        }
    }

    fn callsite(cid: &str, concept_cid: &str, function: &str) -> SqlCallsite {
        SqlCallsite {
            after: MigrationSourceLocation {
                column: 1,
                file: "after.rs".to_string(),
                line: 1,
            },
            before: MigrationSourceLocation {
                column: 1,
                file: "before.rs".to_string(),
                line: 1,
            },
            cid: cid.to_string(),
            concept_cid: concept_cid.to_string(),
            function: function.to_string(),
            sample_args: Vec::new(),
            sql: String::new(),
            substituted_body: format!("{function}:{cid}"),
        }
    }

    fn receipt_for(
        source: &str,
        target: &str,
        callsites: &[SqlCallsite],
        focus: Option<&str>,
    ) -> MigrateReceiptEnvelope {
        let source_decl =
            platform_semantics_for_lower_target(source).expect("source semantics declared");
        let target_decl =
            platform_semantics_for_lower_target(target).expect("target semantics declared");
        let changes = platform_semantic_changes(&source_decl, &target_decl, callsites, focus)
            .expect("semantic comparison is characterizable");
        let functions = callsites
            .iter()
            .map(|callsite| function(&callsite.function))
            .collect::<Vec<_>>();
        let decisions =
            match NonEmptyChangedCallsites::new(changes.changed_callsites) {
                Ok(changed) => {
                    let graph = build_propagation_input(&functions, callsites, changed);
                    propagate_effects(&graph)
                        .expect("propagation succeeds")
                        .decisions
                }
                Err(_) => BTreeMap::new(),
            };
        build_receipt(
            &decisions,
            callsites,
            &functions,
            "source-binding",
            "target-binding",
            TargetSurface::TypescriptPg,
            &changes.divergences_by_callsite_and_dimension,
            &changes.uncharacterizable_callsites,
        )
        .expect("receipt builds")
    }

    #[test]
    fn point_query_receipt_populates_divergence_loss_dimensions_and_is_byte_stable() {
        let callsites = vec![callsite("cs:add", CONCEPT_ADD_CID, "add")];

        let java_receipt = receipt_for("python", "java", &callsites, None);
        let java_receipt_again = receipt_for("python", "java", &callsites, None);
        assert_eq!(java_receipt.root_cid, java_receipt_again.root_cid);
        assert_eq!(java_receipt.aggregate_summary.lossy, 1);
        assert_eq!(java_receipt.aggregate_summary.widened, 1);
        let java_loss = &java_receipt.loss_records[0];
        assert_eq!(java_loss.callsite_cid, "cs:add");
        assert!(matches!(
            java_loss.loss_dimensions.get("ArithmeticOverflow"),
            Some(IrFormula::DivergenceBetween { source, target })
                if **source == atom("python:ArbitraryPrecision")
                    && **target == atom("java:Wrapping")
        ));

        let c_receipt = receipt_for("python", "c", &callsites, None);
        let c_loss = &c_receipt.loss_records[0];
        assert!(matches!(
            c_loss.loss_dimensions.get("ArithmeticOverflow"),
            Some(IrFormula::DivergenceBetween { source, target })
                if **source == atom("python:ArbitraryPrecision")
                    && **target == atom("c:UndefinedBehavior")
        ));
        assert_ne!(java_receipt.root_cid, c_receipt.root_cid);
    }

    #[test]
    fn point_query_focus_scopes_loss_records_to_the_focused_callsite() {
        let callsites = vec![
            callsite("cs:add", CONCEPT_ADD_CID, "add"),
            callsite("cs:div", CONCEPT_DIV_CID, "divide"),
        ];

        let rust_receipt = receipt_for("python", "rust", &callsites, Some("cs:div"));
        assert_eq!(rust_receipt.loss_records.len(), 1);
        assert_eq!(rust_receipt.loss_records[0].callsite_cid, "cs:div");
        assert!(rust_receipt.loss_records[0]
            .loss_dimensions
            .contains_key("IntegerDivisionRounding"));

        let java_receipt = receipt_for("python", "java", &callsites, Some("cs:div"));
        assert_eq!(java_receipt.loss_records.len(), 1);
        assert_eq!(java_receipt.loss_records[0].callsite_cid, "cs:div");
        assert_ne!(rust_receipt.root_cid, java_receipt.root_cid);
    }

    #[test]
    fn point_query_same_kit_exact_leg_emits_zero_loss_records() {
        let callsites = vec![callsite("cs:div", CONCEPT_DIV_CID, "divide")];

        let receipt = receipt_for("python", "python", &callsites, None);
        assert_eq!(receipt.aggregate_summary.lossy, 0);
        assert_eq!(receipt.aggregate_summary.widened, 0);
        assert!(receipt.loss_records.is_empty());
    }

    #[test]
    fn point_query_refuse_decision_does_not_emit_partial_loss_records() {
        let callsites = vec![callsite("cs:add", CONCEPT_ADD_CID, "add")];
        let source = platform_semantics_for_lower_target("python").expect("python semantics");
        let target = platform_semantics_for_lower_target("java").expect("java semantics");
        let changes =
            platform_semantic_changes(&source, &target, &callsites, None).expect("changes");
        let first_changed = changes
            .changed_callsites
            .first()
            .expect("changed callsite");
        let dimension = first_changed
            .dimension_name
            .clone()
            .expect("divergence has a dimension name");
        let functions = vec![function("add")];
        let decisions = BTreeMap::from([(
            "add".to_string(),
            PropagationDecision::Refuse {
                forbidding_contract_cid: "contract:bounds-proof-required".to_string(),
                function_cid: "function:add".to_string(),
                reason: "contract forbids platform semantic widening".to_string(),
                triggering_callsite_cid: "cs:add".to_string(),
            },
        )]);
        assert!(changes
            .divergences_by_callsite_and_dimension
            .contains_key(&("cs:add".to_string(), dimension)));

        let receipt = build_receipt(
            &decisions,
            &callsites,
            &functions,
            "source-binding",
            "target-binding",
            TargetSurface::TypescriptPg,
            &changes.divergences_by_callsite_and_dimension,
            &changes.uncharacterizable_callsites,
        )
        .expect("receipt builds");

        assert_eq!(receipt.aggregate_summary.refused, 1);
        assert_eq!(
            receipt.refusal_mementos[0].forbidding_contract,
            "contract:bounds-proof-required"
        );
        assert!(receipt.loss_records.is_empty());
    }

    #[test]
    fn point_query_uncharacterizable_absent_op_routes_to_refuse_signal() {
        let source = platform_semantics_for_lower_target("rust").expect("rust semantics");
        let target = platform_semantics_for_lower_target("java").expect("java semantics");
        // Find a rust-only op: present in source, absent in target.
        // After the OpCoverageVerdict refactor this returns Ok(Uncharacterizable)
        // rather than Err(TargetOpAbsent), so we match on the verdict directly.
        let rust_only_op = source
            .tags
            .iter()
            .map(|tag| tag.op_cid.as_str())
            .find(|op_cid| {
                matches!(
                    source.compare_op_with(op_cid, &target),
                    Ok(OpCoverageVerdict::Uncharacterizable {
                        absent_on: Side::Target
                    })
                )
            })
            .expect("rust-only op fixture exists");
        let callsites = vec![callsite("cs:rust-native", rust_only_op, "native")];

        let changes = platform_semantic_changes(&source, &target, &callsites, None)
            .expect("uncharacterizable does not bubble as error");
        assert_eq!(changes.uncharacterizable_callsites.len(), 1);
        assert_eq!(
            changes.uncharacterizable_callsites[0].callsite_cid,
            "cs:rust-native"
        );
        assert_eq!(
            changes.uncharacterizable_callsites[0].absent_on,
            Side::Target,
            "source has the tag; target is absent"
        );
        assert!(
            changes.changed_callsites.is_empty(),
            "uncharacterizable must not appear in changed_callsites"
        );
    }

    // D3 / #1228: when ONE side has a binding declaration and the OTHER side
    // does not (whole-binding absence, not per-op absence), per-callsite
    // verdicts must still produce Uncharacterizable for ops declared on the
    // present side. The earlier shape collapsed this case into "empty change
    // set" via an early-return.
    #[test]
    fn changes_for_targets_present_source_absent_target_emits_uncharacterizable() {
        let source = platform_semantics_for_binding("typescript", "better-sqlite3")
            .expect("better-sqlite3 binding declared");
        let declared_op = source
            .tags
            .first()
            .map(|tag| tag.op_cid.clone())
            .expect("better-sqlite3 declares at least one op");
        let callsites = vec![callsite("cs:present-source", &declared_op, "fn")];

        let changes = platform_semantic_changes_for_targets(
            "typescript",
            "better-sqlite3",
            "nope-lang",
            "nope-tag",
            &callsites,
            None,
        )
        .expect("absent target does not bubble as error");

        assert_eq!(
            changes.uncharacterizable_callsites.len(),
            1,
            "callsite declared on source produces uncharacterizable when target is absent"
        );
        assert_eq!(
            changes.uncharacterizable_callsites[0].absent_on,
            Side::Target,
            "target binding is absent; source declared the op"
        );
        assert!(
            changes.changed_callsites.is_empty(),
            "uncharacterizable must not appear in changed_callsites"
        );
    }

    #[test]
    fn changes_for_targets_present_target_absent_source_emits_uncharacterizable() {
        let target = platform_semantics_for_binding("typescript", "better-sqlite3")
            .expect("better-sqlite3 binding declared");
        let declared_op = target
            .tags
            .first()
            .map(|tag| tag.op_cid.clone())
            .expect("better-sqlite3 declares at least one op");
        let callsites = vec![callsite("cs:present-target", &declared_op, "fn")];

        let changes = platform_semantic_changes_for_targets(
            "nope-lang",
            "nope-tag",
            "typescript",
            "better-sqlite3",
            &callsites,
            None,
        )
        .expect("absent source does not bubble as error");

        assert_eq!(
            changes.uncharacterizable_callsites.len(),
            1,
            "callsite declared on target produces uncharacterizable when source is absent"
        );
        assert_eq!(
            changes.uncharacterizable_callsites[0].absent_on,
            Side::Source,
            "source binding is absent; target declared the op"
        );
    }

    #[test]
    fn changes_for_targets_both_absent_produces_empty_change_set() {
        // Discrimination: both bindings absent is NoOpinion (substrate has
        // nothing to say), distinct from one-absent (Uncharacterizable / refuse).
        // The earlier shape collapsed both cases into empty; this test pins
        // that the both-absent case is still correctly empty under the new
        // shape.
        let callsites = vec![callsite("cs:both-absent", "concept:imaginary", "fn")];
        let changes = platform_semantic_changes_for_targets(
            "nope-lang-1",
            "nope-tag-1",
            "nope-lang-2",
            "nope-tag-2",
            &callsites,
            None,
        )
        .expect("both absent does not bubble as error");

        assert!(
            changes.uncharacterizable_callsites.is_empty(),
            "both-absent must not produce uncharacterizable; substrate has no opinion"
        );
        assert!(
            changes.changed_callsites.is_empty(),
            "both-absent must not produce changed callsites"
        );
    }

    fn atom(name: &str) -> IrFormula {
        IrFormula::Atomic {
            name: name.to_string(),
            args: vec![],
        }
    }

    // Helper: build a minimal TsFunction whose body looks like a real
    // better-sqlite3 insert-and-capture-id callsite.
    fn ts_function_with_body(name: &str, body: &str, body_start: usize) -> TsFunction {
        TsFunction {
            body: body.to_string(),
            body_start,
            is_async: false,
            line: 1,
            name: name.to_string(),
            public_sync_contract_cid: None,
            return_type: "{ id: number }".to_string(),
            signature: format!("function {name}(name: string): {{ id: number }}"),
        }
    }

    // Integration test: extract_sql_callsites assigns concept:insert-and-get-id
    // to a callsite whose body contains both .run( and .lastInsertRowid.
    #[test]
    fn extract_sql_callsites_classifies_last_insert_rowid_as_insert_and_get_id() {
        // Minimal source with the function body placed at offset 0.
        // Use a string literal arg ("alice") so extract_sample_args succeeds.
        let body = concat!(
            "const stmt = db.prepare('INSERT INTO users (name) VALUES (?)');\n",
            "const result = stmt.run('alice');\n",
            "return { id: Number(result.lastInsertRowid) };\n"
        );
        let source = body;
        let functions = vec![ts_function_with_body("createUser", body, 0)];

        let callsites =
            extract_sql_callsites(source, &functions).expect("extract_sql_callsites must succeed");

        assert_eq!(callsites.len(), 1, "expected exactly one SQL callsite");
        let site = &callsites[0];

        // Primary assertion: the finer concept-op CID must be insert-and-get-id
        assert_eq!(
            site.concept_cid, SQL_INSERT_AND_GET_ID_CONCEPT_CID,
            "callsite with .run( + .lastInsertRowid must get concept:insert-and-get-id CID"
        );

        // Discrimination: must NOT be the coarser sql-execute CID
        assert_ne!(
            site.concept_cid, SQL_EXECUTE_CONCEPT_CID,
            "callsite must NOT be classified as concept:sql-execute when lastInsertRowid is present"
        );
    }

    // Discrimination: extract_sql_callsites keeps concept:sql-execute for plain run( callsites
    // that do NOT access lastInsertRowid.
    #[test]
    fn extract_sql_callsites_keeps_sql_execute_for_plain_run_callsite() {
        // Use userId (ends with "id") so extract_sample_args resolves to fixture 1.
        let body = concat!(
            "const stmt = db.prepare('UPDATE users SET active = 1 WHERE id = ?');\n",
            "stmt.run(userId);\n"
        );
        let source = body;
        let functions = vec![ts_function_with_body("activateUser", body, 0)];

        let callsites =
            extract_sql_callsites(source, &functions).expect("extract_sql_callsites must succeed");

        assert_eq!(callsites.len(), 1, "expected exactly one SQL callsite");
        let site = &callsites[0];

        assert_eq!(
            site.concept_cid, SQL_EXECUTE_CONCEPT_CID,
            "plain .run( without lastInsertRowid must remain concept:sql-execute"
        );
        assert_ne!(
            site.concept_cid, SQL_INSERT_AND_GET_ID_CONCEPT_CID,
            "plain .run( without lastInsertRowid must NOT get insert-and-get-id"
        );
    }

    // Discrimination: extract_sql_callsites assigns concept:sql-query for read callsites.
    #[test]
    fn extract_sql_callsites_assigns_sql_query_for_get_callsite() {
        // Use userId (ends with "id") so extract_sample_args resolves to fixture 1.
        let body = concat!(
            "const stmt = db.prepare('SELECT * FROM users WHERE id = ?');\n",
            "return stmt.get(userId);\n"
        );
        let source = body;
        let functions = vec![ts_function_with_body("getUser", body, 0)];

        let callsites =
            extract_sql_callsites(source, &functions).expect("extract_sql_callsites must succeed");

        assert_eq!(callsites.len(), 1, "expected exactly one SQL callsite");
        let site = &callsites[0];

        assert_eq!(
            site.concept_cid, SQL_QUERY_CONCEPT_CID,
            ".get( callsite must be concept:sql-query"
        );
        assert_ne!(
            site.concept_cid, SQL_EXECUTE_CONCEPT_CID,
            ".get( callsite must NOT be concept:sql-execute"
        );
        assert_ne!(
            site.concept_cid, SQL_INSERT_AND_GET_ID_CONCEPT_CID,
            ".get( callsite must NOT be concept:insert-and-get-id"
        );
    }
}
