use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use provekit_ir_types::{IrFormula, IrTerm, Sort};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};

pub const SURFACE: &str = "jvm-bytecode";
pub const DIALECT: &str = "jvm-jasmin";

#[derive(Debug, thiserror::Error)]
pub enum LiftError {
    #[error("read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("parse {path}: {message}")]
    Parse { path: String, message: String },
}

#[derive(Debug, Clone)]
pub struct JasminUnit {
    pub path: String,
    pub class_name: Option<String>,
    pub methods: Vec<Method>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Method {
    pub name: String,
    pub descriptor: String,
    pub arg_count: usize,
    pub returns_int: bool,
    pub labels: BTreeMap<String, usize>,
    pub instructions: Vec<Instruction>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    pub opcode: String,
    pub operands: Vec<String>,
    pub text: String,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Refusal {
    pub kind: String,
    pub function: Option<String>,
    pub line: Option<usize>,
    pub instruction: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct LiftResult {
    pub declarations: Vec<Json>,
    pub diagnostics: Vec<Json>,
    pub opacity_report: Vec<Json>,
    pub refusals: Vec<Refusal>,
}

#[derive(Debug, Clone)]
struct SymbolicState {
    stack: Vec<IrTerm>,
    locals: BTreeMap<usize, IrTerm>,
    effects: BTreeSet<JvmEffect>,
}

#[derive(Debug, Clone)]
struct PathState {
    idx: usize,
    state: SymbolicState,
    condition: Condition,
    preconditions: Vec<IrFormula>,
    seen: HashSet<(usize, String)>,
}

#[derive(Debug, Clone)]
struct PathOutcome {
    condition: Condition,
    state: SymbolicState,
    return_value: Option<IrTerm>,
    preconditions: Vec<IrFormula>,
}

#[derive(Debug, Clone)]
struct ExplorationResult {
    outcomes: Vec<PathOutcome>,
    refusals: Vec<Refusal>,
}

#[derive(Debug, Clone)]
struct Condition {
    formula: IrFormula,
    term: IrTerm,
    truth: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum JvmEffect {
    StaticRead(String),
    StaticWrite(String),
    ArrayRead,
    ArrayWrite,
    InvokeStatic(String),
    Trap(String),
}

enum Transfer {
    Next,
    Return(Option<IrTerm>),
    Branch {
        target: String,
        condition: Option<Condition>,
    },
    Refuse(String),
}

struct Step {
    transfer: Transfer,
    preconditions: Vec<IrFormula>,
}

pub fn run_cli() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--rpc") {
        run_rpc();
        return;
    }

    eprintln!("usage: provekit-lift-jvm-bytecode --rpc");
    std::process::exit(1);
}

pub fn parse_jasmin_text(path: &str, source: &str) -> Result<JasminUnit, LiftError> {
    parse_lines(
        path,
        source
            .lines()
            .enumerate()
            .map(|(idx, line)| (idx + 1, line)),
    )
}

pub fn lift_source_text(path: &str, source: &str) -> Result<LiftResult, LiftError> {
    let unit = parse_jasmin_text(path, source)?;
    Ok(lift_unit(&unit))
}

pub fn lift_paths(workspace_root: &Path, source_paths: &[String]) -> Result<LiftResult, LiftError> {
    let mut merged = LiftResult {
        declarations: Vec::new(),
        diagnostics: Vec::new(),
        opacity_report: Vec::new(),
        refusals: Vec::new(),
    };

    for source_path in source_paths {
        let path = resolve_path(workspace_root, source_path);
        for path in expand_source_path(&path)? {
            let display_path = path.to_string_lossy().to_string();
            let source = std::fs::read_to_string(&path).map_err(|source| LiftError::Read {
                path: display_path.clone(),
                source,
            })?;
            let lifted = lift_source_text(&display_path, &source)?;
            merged.declarations.extend(lifted.declarations);
            merged.diagnostics.extend(lifted.diagnostics);
            merged.opacity_report.extend(lifted.opacity_report);
            merged.refusals.extend(lifted.refusals);
        }
    }

    Ok(merged)
}

fn run_rpc() {
    let stdin = std::io::stdin();
    let reader = BufReader::new(stdin);
    let mut stdout = std::io::stdout();

    for line in reader.lines() {
        let Ok(line) = line else {
            eprintln!("rpc: read error");
            break;
        };
        if line.trim().is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<RpcRequest>(&line) {
            Ok(req) => dispatch(req),
            Err(err) => json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": {
                    "code": -32700,
                    "message": format!("PARSE_ERROR: {err}")
                }
            }),
        };

        if let Ok(line) = serde_json::to_string(&response) {
            let _ = writeln!(stdout, "{line}");
            let _ = stdout.flush();
        }
    }
}

#[derive(Debug, Deserialize)]
struct RpcRequest {
    id: Json,
    method: String,
    #[serde(default)]
    params: Json,
}

fn dispatch(req: RpcRequest) -> Json {
    match req.method.as_str() {
        "initialize" => initialize(req.id),
        "lift" => lift_rpc(req.id, req.params),
        "shutdown" => json!({"jsonrpc":"2.0","id":req.id,"result":null}),
        other => error_response(req.id, -32601, format!("METHOD_NOT_FOUND: {other}")),
    }
}

fn initialize(id: Json) -> Json {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "name": "provekit-lift-jvm-bytecode",
            "version": "0.1.0",
            "protocol_version": "provekit-lift/1",
            "capabilities": {
                "authoring_surfaces": [SURFACE, DIALECT],
                "ir_version": "v1.1.0",
                "emits_signed_mementos": false
            }
        }
    })
}

fn lift_rpc(id: Json, params: Json) -> Json {
    let surface = params
        .get("surface")
        .and_then(Json::as_str)
        .unwrap_or(SURFACE);
    if surface != SURFACE && surface != DIALECT {
        return error_response(id, 1003, format!("SURFACE_NOT_SUPPORTED: {surface}"));
    }

    let Some(paths) = params.get("source_paths").and_then(Json::as_array) else {
        return error_response(id, -32602, "source_paths must be an array".to_string());
    };
    let source_paths: Vec<String> = paths
        .iter()
        .filter_map(Json::as_str)
        .map(ToOwned::to_owned)
        .collect();
    if source_paths.len() != paths.len() || source_paths.is_empty() {
        return error_response(
            id,
            -32602,
            "source_paths must be a non-empty array of strings".to_string(),
        );
    }

    let workspace = params
        .get("workspace_root")
        .and_then(Json::as_str)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    match lift_paths(&workspace, &source_paths) {
        Ok(result) => lift_success_response_json(id, &result),
        Err(err) => error_response(id, -32603, err.to_string()),
    }
}

pub fn lift_success_response_json(id: Json, result: &LiftResult) -> Json {
    let refusals: Vec<Json> = result
        .refusals
        .iter()
        .map(|refusal| serde_json::to_value(refusal).unwrap_or(Json::Null))
        .collect();
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "kind": "ir-document",
            "ir": result.declarations,
            "callEdges": [],
            "diagnostics": result.diagnostics,
            "opacityReport": result.opacity_report,
            "refusals": refusals
        }
    })
}

fn error_response(id: Json, code: i64, message: String) -> Json {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

fn parse_lines<'a>(
    path: &str,
    lines: impl Iterator<Item = (usize, &'a str)>,
) -> Result<JasminUnit, LiftError> {
    let mut unit = JasminUnit {
        path: path.to_string(),
        class_name: None,
        methods: Vec::new(),
        diagnostics: Vec::new(),
    };
    let mut current: Option<Method> = None;

    for (line_no, raw) in lines {
        let trimmed = strip_comment(raw).trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with(".class ") {
            unit.class_name = trimmed.split_whitespace().last().map(ToOwned::to_owned);
            continue;
        }

        if trimmed.starts_with(".method ") {
            if current.is_some() {
                return Err(parse_error(path, line_no, "nested .method"));
            }
            let (name, descriptor) = parse_method_header(path, line_no, trimmed)?;
            let arg_count = descriptor_arg_count(path, line_no, &descriptor)?;
            let returns_int = descriptor_return(&descriptor) == Some("I");
            current = Some(Method {
                name,
                descriptor,
                arg_count,
                returns_int,
                labels: BTreeMap::new(),
                instructions: Vec::new(),
                line: line_no,
            });
            continue;
        }

        if trimmed == ".end method" {
            let Some(method) = current.take() else {
                return Err(parse_error(path, line_no, ".end method outside method"));
            };
            unit.methods.push(method);
            continue;
        }

        let Some(method) = &mut current else {
            continue;
        };

        if trimmed.starts_with('.') {
            continue;
        }

        if let Some(label) = trimmed.strip_suffix(':') {
            let label = label.trim();
            if !label.is_empty() {
                method
                    .labels
                    .insert(label.to_string(), method.instructions.len());
            }
            continue;
        }

        let (opcode, operands) = split_instruction(trimmed);
        if opcode.is_empty() {
            continue;
        }
        method.instructions.push(Instruction {
            opcode: opcode.to_ascii_lowercase(),
            operands,
            text: trimmed.to_string(),
            line: line_no,
        });
    }

    if current.is_some() {
        return Err(LiftError::Parse {
            path: path.to_string(),
            message: "unterminated .method".to_string(),
        });
    }

    Ok(unit)
}

fn strip_comment(line: &str) -> &str {
    if line.trim_start().starts_with(';') {
        return "";
    }
    for (idx, ch) in line.char_indices() {
        if ch == ';'
            && line[..idx]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace)
        {
            return &line[..idx];
        }
    }
    line
}

fn parse_method_header(
    path: &str,
    line: usize,
    trimmed: &str,
) -> Result<(String, String), LiftError> {
    let token = trimmed
        .split_whitespace()
        .rev()
        .find(|part| part.contains('('))
        .ok_or_else(|| parse_error(path, line, "method header missing descriptor"))?;
    let open = token
        .find('(')
        .ok_or_else(|| parse_error(path, line, "method descriptor missing '('"))?;
    let name = token[..open].to_string();
    let descriptor = token[open..].to_string();
    if name.is_empty() || !descriptor.starts_with('(') || !descriptor.contains(')') {
        return Err(parse_error(path, line, "malformed method descriptor"));
    }
    Ok((name, descriptor))
}

fn descriptor_arg_count(path: &str, line: usize, descriptor: &str) -> Result<usize, LiftError> {
    let args = descriptor
        .strip_prefix('(')
        .and_then(|rest| rest.split_once(')'))
        .map(|(args, _)| args)
        .ok_or_else(|| parse_error(path, line, "malformed descriptor"))?;
    let mut chars = args.chars().peekable();
    let mut count = 0;
    while let Some(ch) = chars.next() {
        match ch {
            '[' => {
                while chars.peek() == Some(&'[') {
                    chars.next();
                }
                match chars.next() {
                    Some('L') => {
                        for item in chars.by_ref() {
                            if item == ';' {
                                break;
                            }
                        }
                    }
                    Some(_) => {}
                    None => return Err(parse_error(path, line, "truncated array descriptor")),
                }
                count += 1;
            }
            'L' => {
                for item in chars.by_ref() {
                    if item == ';' {
                        break;
                    }
                }
                count += 1;
            }
            'B' | 'C' | 'D' | 'F' | 'I' | 'J' | 'S' | 'Z' => count += 1,
            other => {
                return Err(parse_error(
                    path,
                    line,
                    format!("unsupported descriptor type {other}"),
                ))
            }
        }
    }
    Ok(count)
}

fn descriptor_return(descriptor: &str) -> Option<&str> {
    descriptor.split_once(')').map(|(_, ret)| ret)
}

fn split_instruction(line: &str) -> (&str, Vec<String>) {
    let mut parts = line.splitn(2, char::is_whitespace);
    let opcode = parts.next().unwrap_or_default().trim();
    let operand_text = parts.next().unwrap_or_default().trim();
    if operand_text.is_empty() {
        return (opcode, Vec::new());
    }
    (
        opcode,
        operand_text
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
    )
}

fn lift_unit(unit: &JasminUnit) -> LiftResult {
    let mut result = LiftResult {
        declarations: Vec::new(),
        diagnostics: unit
            .diagnostics
            .iter()
            .map(|message| json!({"severity":"warning","message":message}))
            .collect(),
        opacity_report: Vec::new(),
        refusals: Vec::new(),
    };

    for method in &unit.methods {
        if method.name == "<clinit>" {
            continue;
        }
        if !method.returns_int {
            result.refusals.push(Refusal {
                kind: "unsupported-return-sort".to_string(),
                function: Some(method.name.clone()),
                line: Some(method.line),
                instruction: None,
                reason: format!(
                    "JVM bytecode lifter slice currently expects int-returning methods, got {}",
                    method.descriptor
                ),
            });
            continue;
        }
        let lifted = lift_method(unit, method);
        result.refusals.extend(lifted.refusals);
        if let Some(contract) = lifted.contract {
            result.declarations.push(contract);
        }
    }

    result
}

struct LiftedMethod {
    contract: Option<Json>,
    refusals: Vec<Refusal>,
}

fn lift_method(unit: &JasminUnit, method: &Method) -> LiftedMethod {
    let exploration = explore_method(method);
    let mut refusals = exploration.refusals;

    if exploration.outcomes.is_empty() {
        refusals.push(Refusal {
            kind: "no-return-path".to_string(),
            function: Some(method.name.clone()),
            line: Some(method.line),
            instruction: None,
            reason: "no structured ireturn path was recovered".to_string(),
        });
        return LiftedMethod {
            contract: None,
            refusals,
        };
    }

    let formals = (0..method.arg_count)
        .map(|idx| format!("local{idx}"))
        .collect::<Vec<_>>();
    let formal_sorts = (0..method.arg_count)
        .map(|_| primitive_sort("Int"))
        .collect::<Vec<_>>();
    let pre = build_precondition(&exploration.outcomes);
    let post = build_postcondition(&exploration.outcomes);
    let effects = build_effects_json(&exploration.outcomes);
    let contract = json!({
        "schemaVersion": "1",
        "kind": "function-contract",
        "fnName": method.name,
        "formals": formals,
        "formalSorts": formal_sorts,
        "returnSort": primitive_sort("Int"),
        "pre": pre,
        "post": post,
        "bodyCid": null,
        "effects": effects,
        "locus": {
            "file": unit.path,
            "line": method.line,
            "col": 1
        },
        "autoMintedMementos": []
    });

    LiftedMethod {
        contract: Some(contract),
        refusals,
    }
}

fn explore_method(method: &Method) -> ExplorationResult {
    let mut outcomes = Vec::new();
    let mut refusals = Vec::new();
    let mut work = vec![PathState {
        idx: 0,
        state: SymbolicState::new(method.arg_count),
        condition: true_condition(),
        preconditions: Vec::new(),
        seen: HashSet::new(),
    }];

    while let Some(mut path) = work.pop() {
        if path.idx >= method.instructions.len() {
            refusals.push(Refusal {
                kind: "fallthrough-without-return".to_string(),
                function: Some(method.name.clone()),
                line: Some(method.line),
                instruction: None,
                reason: "instruction stream reached the end without ireturn".to_string(),
            });
            continue;
        }

        let fingerprint = (path.idx, path.state.fingerprint());
        if !path.seen.insert(fingerprint) {
            refusals.push(Refusal {
                kind: "loop-requires-invariant".to_string(),
                function: Some(method.name.clone()),
                line: Some(method.instructions[path.idx].line),
                instruction: Some(method.instructions[path.idx].text.clone()),
                reason: "back edge reached without a loop invariant memento".to_string(),
            });
            continue;
        }

        let instruction = &method.instructions[path.idx];
        let step = match apply_instruction(&mut path.state, instruction) {
            Ok(step) => step,
            Err(reason) => {
                refusals.push(Refusal {
                    kind: "unsupported-instruction".to_string(),
                    function: Some(method.name.clone()),
                    line: Some(instruction.line),
                    instruction: Some(instruction.text.clone()),
                    reason,
                });
                continue;
            }
        };
        path.preconditions.extend(step.preconditions);

        match step.transfer {
            Transfer::Next => {
                path.idx += 1;
                work.push(path);
            }
            Transfer::Return(return_value) => outcomes.push(PathOutcome {
                condition: path.condition,
                state: path.state,
                return_value,
                preconditions: path.preconditions,
            }),
            Transfer::Branch { target, condition } => {
                let Some(target_idx) = method.labels.get(&target).copied() else {
                    refusals.push(Refusal {
                        kind: "missing-branch-target".to_string(),
                        function: Some(method.name.clone()),
                        line: Some(instruction.line),
                        instruction: Some(instruction.text.clone()),
                        reason: format!("branch target {target} was not found in the method"),
                    });
                    continue;
                };
                if target_idx <= path.idx {
                    refusals.push(Refusal {
                        kind: "loop-requires-invariant".to_string(),
                        function: Some(method.name.clone()),
                        line: Some(instruction.line),
                        instruction: Some(instruction.text.clone()),
                        reason:
                            "backward branch was recognized as a loop but no invariant was supplied"
                                .to_string(),
                    });
                    continue;
                }
                match condition {
                    None => {
                        path.idx = target_idx;
                        work.push(path);
                    }
                    Some(condition) => match condition.truth {
                        Some(true) => {
                            path.idx = target_idx;
                            work.push(path);
                        }
                        Some(false) => {
                            path.idx += 1;
                            work.push(path);
                        }
                        None => {
                            let mut fallthrough = path.clone();
                            fallthrough.idx += 1;
                            fallthrough.condition = and_condition(
                                fallthrough.condition,
                                not_condition(condition.clone()),
                            );

                            path.idx = target_idx;
                            path.condition = and_condition(path.condition, condition);
                            work.push(fallthrough);
                            work.push(path);
                        }
                    },
                }
            }
            Transfer::Refuse(reason) => {
                refusals.push(Refusal {
                    kind: "unsupported-control-flow".to_string(),
                    function: Some(method.name.clone()),
                    line: Some(instruction.line),
                    instruction: Some(instruction.text.clone()),
                    reason,
                });
            }
        }
    }

    ExplorationResult { outcomes, refusals }
}

impl SymbolicState {
    fn new(arg_count: usize) -> Self {
        let locals = (0..arg_count)
            .map(|idx| (idx, var(format!("local{idx}"))))
            .collect();
        Self {
            stack: Vec::new(),
            locals,
            effects: BTreeSet::new(),
        }
    }

    fn push(&mut self, term: IrTerm) {
        self.stack.push(term);
    }

    fn pop(&mut self) -> Result<IrTerm, String> {
        self.stack
            .pop()
            .ok_or_else(|| "operand stack underflow".to_string())
    }

    fn peek(&self) -> Result<IrTerm, String> {
        self.stack
            .last()
            .cloned()
            .ok_or_else(|| "operand stack underflow".to_string())
    }

    fn local(&self, idx: usize) -> IrTerm {
        self.locals
            .get(&idx)
            .cloned()
            .unwrap_or_else(|| var(format!("local{idx}")))
    }

    fn fingerprint(&self) -> String {
        let stack = serde_json::to_string(&self.stack).unwrap_or_default();
        let locals = serde_json::to_string(&self.locals).unwrap_or_default();
        format!("{stack}|{locals}")
    }
}

fn apply_instruction(state: &mut SymbolicState, instruction: &Instruction) -> Result<Step, String> {
    let op = instruction.opcode.as_str();
    match op {
        "nop" => Ok(next()),
        "iconst_m1" => {
            state.push(int_const(-1));
            Ok(next())
        }
        "iconst_0" | "iconst_1" | "iconst_2" | "iconst_3" | "iconst_4" | "iconst_5" => {
            let value = op
                .strip_prefix("iconst_")
                .and_then(|value| value.parse::<i32>().ok())
                .ok_or_else(|| format!("malformed {op}"))?;
            state.push(int_const(value));
            Ok(next())
        }
        "bipush" | "sipush" | "ldc" => {
            let value = expect_one(instruction)?
                .parse::<i32>()
                .map_err(|err| format!("malformed integer constant: {err}"))?;
            state.push(int_const(value));
            Ok(next())
        }
        "iload" => {
            let idx = local_index(expect_one(instruction)?)?;
            state.push(state.local(idx));
            Ok(next())
        }
        "iload_0" | "iload_1" | "iload_2" | "iload_3" => {
            let idx = op
                .strip_prefix("iload_")
                .and_then(|value| value.parse::<usize>().ok())
                .ok_or_else(|| format!("malformed {op}"))?;
            state.push(state.local(idx));
            Ok(next())
        }
        "istore" => {
            let idx = local_index(expect_one(instruction)?)?;
            let value = state.pop()?;
            state.locals.insert(idx, value);
            Ok(next())
        }
        "istore_0" | "istore_1" | "istore_2" | "istore_3" => {
            let idx = op
                .strip_prefix("istore_")
                .and_then(|value| value.parse::<usize>().ok())
                .ok_or_else(|| format!("malformed {op}"))?;
            let value = state.pop()?;
            state.locals.insert(idx, value);
            Ok(next())
        }
        "iadd" | "isub" | "imul" | "idiv" | "irem" | "ishl" | "ishr" | "iushr" | "iand" | "ior"
        | "ixor" => {
            let rhs = state.pop()?;
            let lhs = state.pop()?;
            let mut preconditions = Vec::new();
            if op == "idiv" || op == "irem" {
                preconditions.push(not_formula(eq_formula(rhs.clone(), int_const(0))));
            }
            state.push(ctor(format!("jvm:{op}"), vec![lhs, rhs]));
            Ok(Step {
                transfer: Transfer::Next,
                preconditions,
            })
        }
        "ineg" => {
            let value = state.pop()?;
            state.push(ctor("jvm:ineg", vec![value]));
            Ok(next())
        }
        "pop" => {
            state.pop()?;
            Ok(next())
        }
        "dup" => {
            let value = state.peek()?;
            state.push(value);
            Ok(next())
        }
        "dup_x2" => {
            if state.stack.len() < 3 {
                return Err("dup_x2 requires at least three int stack values".to_string());
            }
            let len = state.stack.len();
            let value1 = state.stack[len - 1].clone();
            let value2 = state.stack[len - 2].clone();
            let value3 = state.stack[len - 3].clone();
            state.stack.truncate(len - 3);
            state.stack.push(value1.clone());
            state.stack.push(value3);
            state.stack.push(value2);
            state.stack.push(value1);
            Ok(next())
        }
        "ifeq" | "ifne" | "iflt" | "ifle" | "ifgt" | "ifge" => {
            let target = expect_one(instruction)?.to_string();
            let value = state.pop()?;
            let condition = int_branch_condition(op, value);
            Ok(branch(target, Some(condition)))
        }
        "if_icmpeq" | "if_icmpne" | "if_icmplt" | "if_icmple" | "if_icmpgt" | "if_icmpge" => {
            let target = expect_one(instruction)?.to_string();
            let rhs = state.pop()?;
            let lhs = state.pop()?;
            let condition = icmp_branch_condition(op, lhs, rhs);
            Ok(branch(target, Some(condition)))
        }
        "goto" => Ok(branch(expect_one(instruction)?.to_string(), None)),
        "ireturn" => {
            let value = state.pop()?;
            Ok(Step {
                transfer: Transfer::Return(Some(value)),
                preconditions: Vec::new(),
            })
        }
        "return" => Ok(Step {
            transfer: Transfer::Return(None),
            preconditions: Vec::new(),
        }),
        "getstatic" => {
            let field = expect_one(instruction)?.to_string();
            state.effects.insert(JvmEffect::StaticRead(field.clone()));
            state.push(var(format!("static:{field}")));
            Ok(next())
        }
        "putstatic" => {
            let field = expect_one(instruction)?.to_string();
            state.pop()?;
            state.effects.insert(JvmEffect::StaticWrite(field));
            Ok(next())
        }
        "newarray" => {
            let element = expect_one(instruction)?;
            let size = state.pop()?;
            state.push(ctor(format!("jvm:newarray:{element}"), vec![size]));
            Ok(next())
        }
        "iaload" => {
            let index = state.pop()?;
            let array = state.pop()?;
            state.effects.insert(JvmEffect::ArrayRead);
            state.push(ctor("jvm:iaload", vec![array, index]));
            Ok(next())
        }
        "iastore" => {
            let value = state.pop()?;
            let index = state.pop()?;
            let array = state.pop()?;
            state.effects.insert(JvmEffect::ArrayWrite);
            let _write = ctor("jvm:iastore", vec![array, index, value]);
            Ok(next())
        }
        "invokestatic" => {
            let target = expect_one(instruction)?.to_string();
            let descriptor = target
                .find('(')
                .map(|idx| target[idx..].to_string())
                .ok_or_else(|| "invokestatic target missing descriptor".to_string())?;
            let arg_count = descriptor_arg_count("instruction", instruction.line, &descriptor)
                .map_err(|err| err.to_string())?;
            let mut args = Vec::new();
            for _ in 0..arg_count {
                args.push(state.pop()?);
            }
            args.reverse();
            state
                .effects
                .insert(JvmEffect::InvokeStatic(target.clone()));
            if descriptor_return(&descriptor) == Some("I") {
                let mut call_args = vec![string_const(target)];
                call_args.extend(args);
                state.push(ctor("jvm:invokestatic", call_args));
            }
            Ok(next())
        }
        "athrow" => {
            state.effects.insert(JvmEffect::Trap("athrow".to_string()));
            Ok(Step {
                transfer: Transfer::Refuse(
                    "exceptional control flow requires an exception edge memento".to_string(),
                ),
                preconditions: Vec::new(),
            })
        }
        other => Err(format!("unsupported JVM instruction {other}")),
    }
}

fn next() -> Step {
    Step {
        transfer: Transfer::Next,
        preconditions: Vec::new(),
    }
}

fn branch(target: String, condition: Option<Condition>) -> Step {
    Step {
        transfer: Transfer::Branch { target, condition },
        preconditions: Vec::new(),
    }
}

fn int_branch_condition(op: &str, value: IrTerm) -> Condition {
    let truth = as_i32_const(&value).map(|constant| match op {
        "ifeq" => constant == 0,
        "ifne" => constant != 0,
        "iflt" => constant < 0,
        "ifle" => constant <= 0,
        "ifgt" => constant > 0,
        "ifge" => constant >= 0,
        _ => false,
    });
    let name = match op {
        "ifeq" => "jvm:int_eq_zero",
        "ifne" => "jvm:int_ne_zero",
        "iflt" => "jvm:int_lt_zero",
        "ifle" => "jvm:int_le_zero",
        "ifgt" => "jvm:int_gt_zero",
        "ifge" => "jvm:int_ge_zero",
        _ => "jvm:int_branch",
    };
    Condition {
        formula: IrFormula::Atomic {
            name: name.to_string(),
            args: vec![value.clone()],
        },
        term: ctor(name, vec![value]),
        truth,
    }
}

fn icmp_branch_condition(op: &str, lhs: IrTerm, rhs: IrTerm) -> Condition {
    let truth = as_i32_const(&lhs)
        .zip(as_i32_const(&rhs))
        .map(|(lhs, rhs)| match op {
            "if_icmpeq" => lhs == rhs,
            "if_icmpne" => lhs != rhs,
            "if_icmplt" => lhs < rhs,
            "if_icmple" => lhs <= rhs,
            "if_icmpgt" => lhs > rhs,
            "if_icmpge" => lhs >= rhs,
            _ => false,
        });
    let name = match op {
        "if_icmpeq" => "jvm:icmp_eq",
        "if_icmpne" => "jvm:icmp_ne",
        "if_icmplt" => "jvm:icmp_lt",
        "if_icmple" => "jvm:icmp_le",
        "if_icmpgt" => "jvm:icmp_gt",
        "if_icmpge" => "jvm:icmp_ge",
        _ => "jvm:icmp",
    };
    Condition {
        formula: IrFormula::Atomic {
            name: name.to_string(),
            args: vec![lhs.clone(), rhs.clone()],
        },
        term: ctor(name, vec![lhs, rhs]),
        truth,
    }
}

fn build_precondition(outcomes: &[PathOutcome]) -> IrFormula {
    let mut operands = Vec::new();
    for outcome in outcomes {
        for pre in &outcome.preconditions {
            operands.push(IrFormula::Implies {
                operands: vec![outcome.condition.formula.clone(), pre.clone()],
            });
        }
    }
    and_formula(operands)
}

fn build_postcondition(outcomes: &[PathOutcome]) -> IrFormula {
    let mut operands = Vec::new();
    let return_value = branch_term(
        outcomes
            .iter()
            .filter_map(|outcome| {
                outcome
                    .return_value
                    .clone()
                    .map(|value| (outcome.condition.clone(), value))
            })
            .collect(),
    );
    if let Some(return_value) = return_value {
        operands.push(eq_formula(var("return_value"), return_value));
    }

    let mut locals = BTreeSet::new();
    for outcome in outcomes {
        locals.extend(outcome.state.locals.keys().copied());
    }
    for idx in locals {
        let choices = outcomes
            .iter()
            .filter_map(|outcome| {
                outcome
                    .state
                    .locals
                    .get(&idx)
                    .cloned()
                    .map(|value| (outcome.condition.clone(), value))
            })
            .collect();
        if let Some(value) = branch_term(choices) {
            operands.push(eq_formula(var(format!("local{idx}_out")), value));
        }
    }

    and_formula(operands)
}

fn branch_term(mut choices: Vec<(Condition, IrTerm)>) -> Option<IrTerm> {
    match choices.len() {
        0 => None,
        1 => choices.pop().map(|(_, term)| term),
        _ => {
            let (_, mut current) = choices.pop().expect("non-empty choices");
            for (condition, term) in choices.into_iter().rev() {
                current = ctor("jvm:ite", vec![condition.term, term, current]);
            }
            Some(current)
        }
    }
}

fn build_effects_json(outcomes: &[PathOutcome]) -> Vec<Json> {
    let mut effects = BTreeSet::new();
    for outcome in outcomes {
        effects.extend(outcome.state.effects.iter().cloned());
    }

    effects
        .into_iter()
        .map(|effect| match effect {
            JvmEffect::StaticRead(target) => json!({"kind":"reads","target":target}),
            JvmEffect::StaticWrite(target) => json!({"kind":"writes","target":target}),
            JvmEffect::ArrayRead => json!({"kind":"reads","target":"jvm:array"}),
            JvmEffect::ArrayWrite => json!({"kind":"writes","target":"jvm:array"}),
            JvmEffect::InvokeStatic(target) => json!({"kind":"unresolved_call","name":target}),
            JvmEffect::Trap(reason) => json!({"kind":"unresolved_call","name":reason}),
        })
        .collect()
}

fn and_condition(lhs: Condition, rhs: Condition) -> Condition {
    match (lhs.truth, rhs.truth) {
        (Some(false), _) | (_, Some(false)) => false_condition(),
        (Some(true), _) => rhs,
        (_, Some(true)) => lhs,
        _ => Condition {
            formula: IrFormula::And {
                operands: vec![lhs.formula, rhs.formula],
            },
            term: ctor("jvm:and", vec![lhs.term, rhs.term]),
            truth: None,
        },
    }
}

fn not_condition(condition: Condition) -> Condition {
    match condition.truth {
        Some(true) => false_condition(),
        Some(false) => true_condition(),
        None => Condition {
            formula: not_formula(condition.formula),
            term: ctor("jvm:not", vec![condition.term]),
            truth: None,
        },
    }
}

fn true_condition() -> Condition {
    Condition {
        formula: true_formula(),
        term: bool_const(true),
        truth: Some(true),
    }
}

fn false_condition() -> Condition {
    Condition {
        formula: false_formula(),
        term: bool_const(false),
        truth: Some(false),
    }
}

fn expect_one(instruction: &Instruction) -> Result<&str, String> {
    if instruction.operands.len() == 1 {
        Ok(instruction.operands[0].as_str())
    } else {
        Err(format!(
            "{} expects one operand, got {}",
            instruction.opcode,
            instruction.operands.len()
        ))
    }
}

fn local_index(value: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|err| format!("malformed local index {value}: {err}"))
}

fn as_i32_const(term: &IrTerm) -> Option<i32> {
    match term {
        IrTerm::Const { value, .. } => value
            .as_i64()
            .and_then(|value| i32::try_from(value).ok())
            .or_else(|| value.as_u64().and_then(|value| i32::try_from(value).ok())),
        _ => None,
    }
}

fn primitive_sort(name: &str) -> Sort {
    Sort::Primitive {
        name: name.to_string(),
    }
}

fn var(name: impl Into<String>) -> IrTerm {
    IrTerm::Var { name: name.into() }
}

fn int_const(value: i32) -> IrTerm {
    IrTerm::Const {
        value: json!(value),
        sort: primitive_sort("Int"),
    }
}

fn bool_const(value: bool) -> IrTerm {
    IrTerm::Const {
        value: json!(value),
        sort: primitive_sort("Bool"),
    }
}

fn string_const(value: impl Into<String>) -> IrTerm {
    IrTerm::Const {
        value: json!(value.into()),
        sort: primitive_sort("String"),
    }
}

fn ctor(name: impl Into<String>, args: Vec<IrTerm>) -> IrTerm {
    IrTerm::Ctor {
        name: name.into(),
        args,
    }
}

fn eq_formula(lhs: IrTerm, rhs: IrTerm) -> IrFormula {
    IrFormula::Atomic {
        name: "=".to_string(),
        args: vec![lhs, rhs],
    }
}

fn not_formula(formula: IrFormula) -> IrFormula {
    IrFormula::Not {
        operands: vec![formula],
    }
}

fn and_formula(operands: Vec<IrFormula>) -> IrFormula {
    match operands.len() {
        0 => true_formula(),
        1 => operands.into_iter().next().expect("one operand"),
        _ => IrFormula::And { operands },
    }
}

fn true_formula() -> IrFormula {
    IrFormula::Atomic {
        name: "true".to_string(),
        args: Vec::new(),
    }
}

fn false_formula() -> IrFormula {
    IrFormula::Atomic {
        name: "false".to_string(),
        args: Vec::new(),
    }
}

fn parse_error(path: &str, line: usize, message: impl Into<String>) -> LiftError {
    LiftError::Parse {
        path: path.to_string(),
        message: format!("line {line}: {}", message.into()),
    }
}

fn resolve_path(workspace_root: &Path, source_path: &str) -> PathBuf {
    let path = PathBuf::from(source_path);
    if path.is_absolute() {
        path
    } else {
        workspace_root.join(path)
    }
}

fn expand_source_path(path: &Path) -> Result<Vec<PathBuf>, LiftError> {
    if path.is_dir() {
        let mut paths = Vec::new();
        collect_jasmin_files(path, &mut paths)?;
        paths.sort();
        Ok(paths)
    } else {
        Ok(vec![path.to_path_buf()])
    }
}

fn collect_jasmin_files(dir: &Path, paths: &mut Vec<PathBuf>) -> Result<(), LiftError> {
    let entries = std::fs::read_dir(dir).map_err(|source| LiftError::Read {
        path: dir.to_string_lossy().to_string(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| LiftError::Read {
            path: dir.to_string_lossy().to_string(),
            source,
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_jasmin_files(&path, paths)?;
        } else if is_jasmin_path(&path) {
            paths.push(path);
        }
    }
    Ok(())
}

fn is_jasmin_path(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("j" | "jasm" | "jasmin")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_recognizes_class_method_labels_and_instructions() {
        let source = ".class public Foo\n.super java/lang/Object\n\n.method public static foo(I)I\n  .limit stack 2\nL0:\n  iload 0\n  ireturn\n.end method\n";

        let unit = parse_jasmin_text("Foo.j", source).expect("Jasmin parses");

        assert_eq!(unit.class_name.as_deref(), Some("Foo"));
        assert_eq!(unit.methods.len(), 1);
        assert_eq!(unit.methods[0].name, "foo");
        assert_eq!(unit.methods[0].arg_count, 1);
        assert_eq!(unit.methods[0].labels.get("L0"), Some(&0));
        assert_eq!(unit.methods[0].instructions[0].opcode, "iload");
    }

    #[test]
    fn parser_keeps_descriptor_semicolons_before_trailing_comments() {
        let source = ".class public Foo\n.method public static main([Ljava/lang/String;)V ; entry\n  return\n.end method\n";

        let unit = parse_jasmin_text("Foo.j", source).expect("Jasmin parses");

        assert_eq!(unit.methods[0].name, "main");
        assert_eq!(unit.methods[0].descriptor, "([Ljava/lang/String;)V");
        assert_eq!(unit.methods[0].arg_count, 1);
    }

    #[test]
    fn lifter_recovers_branching_foo_contract_from_jasmin() {
        let source = include_str!("../tests/fixtures/foo.j");
        let result = lift_source_text("Foo.j", source).expect("lift succeeds");

        assert_eq!(result.declarations.len(), 1);
        assert!(
            result.refusals.is_empty(),
            "unexpected refusals: {:?}",
            result.refusals
        );

        let contract = &result.declarations[0];
        assert_eq!(contract["kind"], "function-contract");
        assert_eq!(contract["fnName"], "foo");
        assert_eq!(contract["formals"], json!(["local0"]));
        assert_eq!(contract["effects"], json!([]));

        let post = serde_json::to_string(&contract["post"]).unwrap();
        assert!(post.contains("return_value"));
        assert!(post.contains("jvm:ite"));
        assert!(post.contains("jvm:icmp_eq"));
        assert!(post.contains("jvm:ineg"));
        assert!(post.contains("local0"));
    }

    #[test]
    fn lift_paths_enumerates_jasmin_files_in_directories() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let result = lift_paths(root, &["tests/fixtures".to_string()]).expect("directory lifts");

        assert_eq!(result.declarations.len(), 1);
        assert_eq!(result.declarations[0]["fnName"], "foo");
    }
}
