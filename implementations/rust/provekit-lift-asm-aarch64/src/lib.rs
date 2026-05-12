use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use provekit_ir_types::{IrFormula, IrTerm, Sort};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};

#[derive(Debug, thiserror::Error)]
pub enum LiftError {
    #[error("read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("objdump failed for {path}: {message}")]
    Objdump { path: String, message: String },
    #[error("parse {path}: {message}")]
    Parse { path: String, message: String },
}

#[derive(Debug, Clone)]
pub struct AssemblyUnit {
    pub path: String,
    pub functions: Vec<AsmFunction>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AsmFunction {
    pub name: String,
    pub labels: HashMap<String, usize>,
    pub instructions: Vec<Instruction>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    pub mnemonic: String,
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
pub struct PublicInstructionSemantics {
    pub preconditions: Vec<IrFormula>,
    pub postconditions: Vec<IrFormula>,
    pub effects: Vec<String>,
}

#[derive(Debug, Clone)]
struct InstructionSemantics {
    preconditions: Vec<IrFormula>,
    updates: Vec<StateUpdate>,
    effects: Vec<AsmEffect>,
    transfer: Transfer,
}

#[derive(Debug, Clone)]
struct StateUpdate {
    target: String,
    value: IrTerm,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum AsmEffect {
    MemRead(String),
    MemWrite(String),
    Call(String),
    Trap(String),
}

#[derive(Debug, Clone)]
enum Transfer {
    Next,
    Return,
    Branch(String),
    Conditional {
        condition: Box<Condition>,
        target: String,
    },
    Refuse(String),
}

#[derive(Debug, Clone)]
struct Condition {
    formula: IrFormula,
    term: IrTerm,
}

#[derive(Debug, Clone)]
struct SymbolicState {
    values: BTreeMap<String, IrTerm>,
    inputs: BTreeMap<String, Sort>,
}

#[derive(Debug, Clone)]
struct PathOutcome {
    condition: Condition,
    state: SymbolicState,
    effects: BTreeSet<AsmEffect>,
    preconditions: Vec<IrFormula>,
}

#[derive(Debug, Clone)]
struct ExplorationResult {
    outcomes: Vec<PathOutcome>,
    refusals: Vec<Refusal>,
}

pub fn run_cli() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--rpc") {
        run_rpc();
        return;
    }

    eprintln!("usage: provekit-lift-asm-aarch64 --rpc");
    std::process::exit(1);
}

pub fn parse_assembly_text(path: &str, source: &str) -> Result<AssemblyUnit, LiftError> {
    parse_lines(
        path,
        source
            .lines()
            .enumerate()
            .map(|(idx, line)| (idx + 1, line)),
    )
}

pub fn semantics_for_instruction(
    instruction: &Instruction,
) -> Result<PublicInstructionSemantics, LiftError> {
    let mut state = SymbolicState::new();
    let sem = semantics(instruction, &mut state)?;
    let mut postconditions = Vec::new();
    for update in sem.updates {
        postconditions.push(eq(var(format!("{}_out", update.target)), update.value));
    }
    Ok(PublicInstructionSemantics {
        preconditions: sem.preconditions,
        postconditions,
        effects: sem.effects.iter().map(AsmEffect::display_name).collect(),
    })
}

pub fn lift_source_text(path: &str, source: &str) -> Result<LiftResult, LiftError> {
    let unit = parse_assembly_text(path, source)?;
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
        let display_path = path.to_string_lossy().to_string();
        let unit = if is_assembly_path(&path) {
            let source = std::fs::read_to_string(&path).map_err(|source| LiftError::Read {
                path: display_path.clone(),
                source,
            })?;
            parse_assembly_text(&display_path, &source)?
        } else {
            let disassembly = disassemble_with_objdump(&path)?;
            parse_objdump_text(&display_path, &disassembly)?
        };

        let lifted = lift_unit(&unit);
        merged.declarations.extend(lifted.declarations);
        merged.diagnostics.extend(lifted.diagnostics);
        merged.opacity_report.extend(lifted.opacity_report);
        merged.refusals.extend(lifted.refusals);
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
            "name": "asm-aarch64",
            "version": "0.1.0",
            "protocol_version": "provekit-lift/1",
            "capabilities": {
                "authoring_surfaces": ["asm-aarch64", "aarch64"],
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
        .unwrap_or("asm-aarch64");
    if surface != "asm-aarch64" && surface != "aarch64" {
        return error_response(id, 1003, format!("SURFACE_NOT_SUPPORTED: {surface}"));
    }

    let layer = params
        .get("options")
        .and_then(|v| v.get("layer"))
        .and_then(Json::as_str)
        .unwrap_or("all");
    if layer != "all" {
        return error_response(id, 1006, format!("UNSUPPORTED_LIFT_LAYER: {layer}"));
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
        Ok(result) => {
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
                    "declarations": result.declarations,
                    "callEdges": [],
                    "diagnostics": result.diagnostics,
                    "opacityReport": result.opacity_report,
                    "refusals": refusals
                }
            })
        }
        Err(err) => error_response(id, -32603, err.to_string()),
    }
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

fn lift_unit(unit: &AssemblyUnit) -> LiftResult {
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

    for function in &unit.functions {
        let lifted = lift_function(unit, function);
        result.refusals.extend(lifted.refusals);
        if let Some(contract) = lifted.contract {
            result.declarations.push(contract);
        }
    }

    result
}

struct LiftedFunction {
    contract: Option<Json>,
    refusals: Vec<Refusal>,
}

fn lift_function(unit: &AssemblyUnit, function: &AsmFunction) -> LiftedFunction {
    let initial = SymbolicState::new();
    let exploration = explore_function(function, initial);
    let mut refusals = exploration.refusals;

    if exploration.outcomes.is_empty() {
        refusals.push(Refusal {
            kind: "no-return-path".to_string(),
            function: Some(function.name.clone()),
            line: Some(function.line),
            instruction: None,
            reason: "no structured return path was recovered".to_string(),
        });
        return LiftedFunction {
            contract: None,
            refusals,
        };
    }

    let mut formals = collect_formals(&exploration.outcomes);
    if formals.is_empty() {
        formals.push(("state".to_string(), state_sort()));
    }
    let formal_names: Vec<String> = formals.iter().map(|(name, _)| name.clone()).collect();
    let formal_sorts: Vec<Sort> = formals.into_iter().map(|(_, sort)| sort).collect();

    let pre = build_precondition(&exploration.outcomes);
    let post = build_postcondition(&exploration.outcomes);
    let effects = build_effects_json(&exploration.outcomes);
    let contract = json!({
        "schemaVersion": "1",
        "kind": "function-contract",
        "fnName": function.name,
        "formals": formal_names,
        "formalSorts": formal_sorts,
        "returnSort": state_sort(),
        "pre": pre,
        "post": post,
        "bodyCid": null,
        "effects": effects,
        "locus": {
            "file": unit.path,
            "line": function.line,
            "col": 1
        },
        "autoMintedMementos": []
    });

    LiftedFunction {
        contract: Some(contract),
        refusals,
    }
}

fn explore_function(function: &AsmFunction, initial: SymbolicState) -> ExplorationResult {
    let mut refusals = Vec::new();
    let mut stack = Vec::new();
    let mut outcomes = Vec::new();
    stack.push((
        0usize,
        initial,
        true_condition(),
        BTreeSet::new(),
        Vec::new(),
        HashSet::<(usize, String)>::new(),
    ));

    while let Some((idx, mut state, path_cond, effects, preconditions, mut seen)) = stack.pop() {
        if idx >= function.instructions.len() {
            refusals.push(Refusal {
                kind: "fallthrough-without-return".to_string(),
                function: Some(function.name.clone()),
                line: Some(function.line),
                instruction: None,
                reason: "instruction stream reached the end without ret".to_string(),
            });
            continue;
        }

        let fingerprint = (idx, state.fingerprint());
        if !seen.insert(fingerprint) {
            refusals.push(Refusal {
                kind: "loop-requires-invariant".to_string(),
                function: Some(function.name.clone()),
                line: Some(function.instructions[idx].line),
                instruction: Some(function.instructions[idx].text.clone()),
                reason: "back edge reached without a loop invariant memento".to_string(),
            });
            continue;
        }

        let instruction = &function.instructions[idx];
        let sem = match semantics(instruction, &mut state) {
            Ok(sem) => sem,
            Err(err) => {
                refusals.push(Refusal {
                    kind: "unsupported-instruction".to_string(),
                    function: Some(function.name.clone()),
                    line: Some(instruction.line),
                    instruction: Some(instruction.text.clone()),
                    reason: err.to_string(),
                });
                continue;
            }
        };

        let mut next_preconditions = preconditions;
        next_preconditions.extend(sem.preconditions);
        let mut next_effects = effects;
        for effect in sem.effects {
            next_effects.insert(effect);
        }
        for update in sem.updates {
            state.values.insert(update.target, update.value);
        }

        match sem.transfer {
            Transfer::Next => {
                stack.push((
                    idx + 1,
                    state,
                    path_cond,
                    next_effects,
                    next_preconditions,
                    seen,
                ));
            }
            Transfer::Return => {
                outcomes.push(PathOutcome {
                    condition: path_cond,
                    state,
                    effects: next_effects,
                    preconditions: next_preconditions,
                });
            }
            Transfer::Branch(label) => {
                if let Some(target) = function.labels.get(&label).copied() {
                    if target <= idx {
                        refusals.push(loop_refusal(function, instruction));
                    } else {
                        stack.push((
                            target,
                            state,
                            path_cond,
                            next_effects,
                            next_preconditions,
                            seen,
                        ));
                    }
                } else {
                    refusals.push(missing_label_refusal(function, instruction, &label));
                }
            }
            Transfer::Conditional { condition, target } => {
                let condition = *condition;
                if let Some(target_idx) = function.labels.get(&target).copied() {
                    if target_idx <= idx {
                        refusals.push(loop_refusal(function, instruction));
                        stack.push((
                            idx + 1,
                            state,
                            and_condition(path_cond, not_condition(condition)),
                            next_effects,
                            next_preconditions,
                            seen,
                        ));
                    } else {
                        let taken_cond = and_condition(path_cond.clone(), condition.clone());
                        let fallthrough_cond = and_condition(path_cond, not_condition(condition));
                        stack.push((
                            idx + 1,
                            state.clone(),
                            fallthrough_cond,
                            next_effects.clone(),
                            next_preconditions.clone(),
                            seen.clone(),
                        ));
                        stack.push((
                            target_idx,
                            state,
                            taken_cond,
                            next_effects,
                            next_preconditions,
                            seen,
                        ));
                    }
                } else {
                    refusals.push(missing_label_refusal(function, instruction, &target));
                }
            }
            Transfer::Refuse(reason) => {
                refusals.push(Refusal {
                    kind: "unsupported-control-flow".to_string(),
                    function: Some(function.name.clone()),
                    line: Some(instruction.line),
                    instruction: Some(instruction.text.clone()),
                    reason,
                });
            }
        }
    }

    ExplorationResult { outcomes, refusals }
}

fn loop_refusal(function: &AsmFunction, instruction: &Instruction) -> Refusal {
    Refusal {
        kind: "loop-requires-invariant".to_string(),
        function: Some(function.name.clone()),
        line: Some(instruction.line),
        instruction: Some(instruction.text.clone()),
        reason: "backward branch was recognized as a loop but no invariant was supplied"
            .to_string(),
    }
}

fn missing_label_refusal(
    function: &AsmFunction,
    instruction: &Instruction,
    label: &str,
) -> Refusal {
    Refusal {
        kind: "missing-branch-target".to_string(),
        function: Some(function.name.clone()),
        line: Some(instruction.line),
        instruction: Some(instruction.text.clone()),
        reason: format!("branch target {label} was not found in the function"),
    }
}

fn collect_formals(outcomes: &[PathOutcome]) -> Vec<(String, Sort)> {
    let mut formals: BTreeMap<String, Sort> = BTreeMap::new();
    for outcome in outcomes {
        for (name, sort) in &outcome.state.inputs {
            formals.entry(name.clone()).or_insert_with(|| sort.clone());
        }
    }
    formals.into_iter().collect()
}

fn build_precondition(outcomes: &[PathOutcome]) -> IrFormula {
    let mut parts = Vec::new();
    for outcome in outcomes {
        for pre in &outcome.preconditions {
            parts.push(implies(outcome.condition.formula.clone(), pre.clone()));
        }
    }
    and_formula(parts)
}

fn build_postcondition(outcomes: &[PathOutcome]) -> IrFormula {
    let mut modified = BTreeSet::new();
    for outcome in outcomes {
        for key in outcome.state.values.keys() {
            modified.insert(key.clone());
        }
    }

    let mut parts = Vec::new();
    for reg in modified {
        let sort = sort_for_name(&reg);
        let out = var(format!("{reg}_out"));
        let expr = if outcomes.len() == 2 {
            let then_value = outcomes[0].state.value_or_input(&reg, sort.clone());
            let else_value = outcomes[1].state.value_or_input(&reg, sort.clone());
            ctor(
                "ite",
                vec![outcomes[0].condition.term.clone(), then_value, else_value],
            )
        } else {
            let mut branch_expr = outcomes
                .last()
                .map(|outcome| outcome.state.value_or_input(&reg, sort.clone()))
                .unwrap_or_else(|| var(&reg));
            for outcome in outcomes.iter().rev().skip(1) {
                branch_expr = ctor(
                    "ite",
                    vec![
                        outcome.condition.term.clone(),
                        outcome.state.value_or_input(&reg, sort.clone()),
                        branch_expr,
                    ],
                );
            }
            branch_expr
        };
        parts.push(eq(out, expr));
    }

    and_formula(parts)
}

fn build_effects_json(outcomes: &[PathOutcome]) -> Vec<Json> {
    let mut effects = BTreeSet::new();
    for outcome in outcomes {
        effects.extend(outcome.effects.iter().cloned());
    }

    let mut out = Vec::new();
    for effect in effects {
        let item = match effect {
            AsmEffect::MemRead(target) => json!({"kind":"reads","target":target}),
            AsmEffect::MemWrite(target) => json!({"kind":"writes","target":target}),
            AsmEffect::Call(name) => json!({"kind":"unresolved_call","name":name}),
            AsmEffect::Trap(reason) => {
                out.push(json!({"kind":"panics"}));
                json!({"kind":"unresolved_call","name":reason})
            }
        };
        out.push(item);
    }
    out
}

impl SymbolicState {
    fn new() -> Self {
        Self {
            values: BTreeMap::new(),
            inputs: BTreeMap::new(),
        }
    }

    fn read(&mut self, name: &str) -> IrTerm {
        let canonical = canonical_register(name);
        self.inputs
            .entry(canonical.clone())
            .or_insert_with(|| sort_for_name(&canonical));
        self.values
            .get(&canonical)
            .cloned()
            .unwrap_or_else(|| var(canonical))
    }

    fn value_or_input(&self, name: &str, sort: Sort) -> IrTerm {
        self.values
            .get(name)
            .cloned()
            .unwrap_or_else(|| IrTerm::Var {
                name: name.to_string(),
            })
            .with_sort_hint(sort)
    }

    fn fingerprint(&self) -> String {
        let value = serde_json::to_string(&self.values).unwrap_or_default();
        let inputs = serde_json::to_string(&self.inputs).unwrap_or_default();
        format!("{value}|{inputs}")
    }
}

trait SortHint {
    fn with_sort_hint(self, _sort: Sort) -> Self;
}

impl SortHint for IrTerm {
    fn with_sort_hint(self, _sort: Sort) -> Self {
        self
    }
}

fn semantics(
    instruction: &Instruction,
    state: &mut SymbolicState,
) -> Result<InstructionSemantics, LiftError> {
    let op = instruction.mnemonic.as_str();
    let ops = instruction.operands.as_slice();
    match op {
        "mov" => {
            // ASL: MoveWidePreferred
            let (dst, src) = expect_two(instruction)?;
            let width = register_width(dst);
            Ok(write_next(
                canonical_register(dst),
                operand_term(src, state, width)?,
            ))
        }
        "movz" => {
            // ASL: MoveWideImmediate
            let (dst, src) = expect_at_least_two(instruction)?;
            let width = register_width(dst);
            Ok(write_next(
                canonical_register(dst),
                immediate_term(src, width)?,
            ))
        }
        "movk" => {
            // ASL: MoveWideImmediate
            let (dst, src) = expect_at_least_two(instruction)?;
            let width = register_width(dst);
            let prev = state.read(dst);
            let shift = shift_operand(ops.get(2).map(String::as_str).unwrap_or("lsl #0"))?;
            let value = ctor(
                &format!("movk{width}"),
                vec![prev, immediate_term(src, width)?, const_bv(shift, width)],
            );
            Ok(write_next(canonical_register(dst), value))
        }
        "add" | "adds" => {
            // ASL: AddSubShiftedRegister
            add_sub_semantics(instruction, state, true, op == "adds")
        }
        "sub" | "subs" => {
            // ASL: AddSubShiftedRegister
            add_sub_semantics(instruction, state, false, op == "subs")
        }
        "cmp" => {
            // ASL: AddSubShiftedRegister
            compare_semantics(instruction, state, false)
        }
        "cmn" => {
            // ASL: AddSubShiftedRegister
            compare_semantics(instruction, state, true)
        }
        "and" | "orr" | "eor" => {
            // ASL: LogicalShiftedRegister
            logic_semantics(instruction, state, op)
        }
        "lsl" | "lsr" | "asr" => {
            // ASL: BitfieldMove
            shift_semantics(instruction, state, op)
        }
        "ldr" | "ldrb" => {
            // ASL: LoadStoreRegisterUnsignedImmediate
            load_semantics(instruction, state, op == "ldrb")
        }
        "str" | "strb" => {
            // ASL: LoadStoreRegisterUnsignedImmediate
            store_semantics(instruction, state, op == "strb")
        }
        "b" => {
            // ASL: BranchImmediate
            let label = expect_one(instruction)?.to_string();
            Ok(InstructionSemantics {
                preconditions: Vec::new(),
                updates: Vec::new(),
                effects: Vec::new(),
                transfer: Transfer::Branch(label),
            })
        }
        "bl" => {
            // ASL: BranchWithLink
            let label = expect_one(instruction)?.to_string();
            Ok(InstructionSemantics {
                preconditions: Vec::new(),
                updates: Vec::new(),
                effects: vec![AsmEffect::Call(label)],
                transfer: Transfer::Next,
            })
        }
        "br" | "blr" => {
            // ASL: BranchToRegister
            Ok(InstructionSemantics {
                preconditions: Vec::new(),
                updates: Vec::new(),
                effects: Vec::new(),
                transfer: Transfer::Refuse(format!(
                    "computed branch {op} is not structured in this lifter slice"
                )),
            })
        }
        "ret" => {
            // ASL: Return
            Ok(InstructionSemantics {
                preconditions: Vec::new(),
                updates: Vec::new(),
                effects: Vec::new(),
                transfer: Transfer::Return,
            })
        }
        "brk" | "svc" => {
            // ASL: ExceptionGeneration
            Ok(InstructionSemantics {
                preconditions: Vec::new(),
                updates: Vec::new(),
                effects: vec![AsmEffect::Trap(op.to_string())],
                transfer: Transfer::Return,
            })
        }
        "cbz" | "cbnz" => {
            // ASL: CompareAndBranch
            cbz_semantics(instruction, state, op == "cbz")
        }
        "tbz" | "tbnz" => {
            // ASL: TestBitAndBranch
            tbz_semantics(instruction, state, op == "tbz")
        }
        cond if cond.starts_with("b.") => {
            // ASL: BranchCondition
            branch_condition_semantics(instruction, state)
        }
        _ => Err(LiftError::Parse {
            path: "instruction".to_string(),
            message: format!("unsupported mnemonic {op}"),
        }),
    }
}

fn write_next(target: String, value: IrTerm) -> InstructionSemantics {
    InstructionSemantics {
        preconditions: Vec::new(),
        updates: vec![StateUpdate { target, value }],
        effects: Vec::new(),
        transfer: Transfer::Next,
    }
}

fn add_sub_semantics(
    instruction: &Instruction,
    state: &mut SymbolicState,
    add: bool,
    set_flags: bool,
) -> Result<InstructionSemantics, LiftError> {
    let (dst, lhs, rhs) = expect_three(instruction)?;
    let width = register_width(dst);
    let lhs_term = state.read(lhs);
    let rhs_term = operand_term(rhs, state, width)?;
    let op_name = if add { "bvadd" } else { "bvsub" };
    let value = ctor(
        &format!("{op_name}{width}"),
        vec![lhs_term.clone(), rhs_term.clone()],
    );
    let mut updates = vec![StateUpdate {
        target: canonical_register(dst),
        value: value.clone(),
    }];
    if set_flags {
        updates.extend(flag_updates_for_result(
            &value, &lhs_term, &rhs_term, width, add,
        ));
    }
    Ok(InstructionSemantics {
        preconditions: Vec::new(),
        updates,
        effects: Vec::new(),
        transfer: Transfer::Next,
    })
}

fn compare_semantics(
    instruction: &Instruction,
    state: &mut SymbolicState,
    add: bool,
) -> Result<InstructionSemantics, LiftError> {
    let (lhs, rhs) = expect_two(instruction)?;
    let width = register_width(lhs);
    let lhs_term = state.read(lhs);
    let rhs_term = operand_term(rhs, state, width)?;
    let op_name = if add { "bvadd" } else { "bvsub" };
    let value = ctor(
        &format!("{op_name}{width}"),
        vec![lhs_term.clone(), rhs_term.clone()],
    );
    Ok(InstructionSemantics {
        preconditions: Vec::new(),
        updates: flag_updates_for_result(&value, &lhs_term, &rhs_term, width, add),
        effects: Vec::new(),
        transfer: Transfer::Next,
    })
}

fn flag_updates_for_result(
    value: &IrTerm,
    lhs: &IrTerm,
    rhs: &IrTerm,
    width: u8,
    add: bool,
) -> Vec<StateUpdate> {
    let carry = if add { "carry_add" } else { "carry_sub" };
    let overflow = if add { "overflow_add" } else { "overflow_sub" };
    vec![
        StateUpdate {
            target: "N".to_string(),
            value: ctor(&format!("sign{width}"), vec![value.clone()]),
        },
        StateUpdate {
            target: "Z".to_string(),
            value: ctor(&format!("is_zero{width}"), vec![value.clone()]),
        },
        StateUpdate {
            target: "C".to_string(),
            value: ctor(&format!("{carry}{width}"), vec![lhs.clone(), rhs.clone()]),
        },
        StateUpdate {
            target: "V".to_string(),
            value: ctor(
                &format!("{overflow}{width}"),
                vec![lhs.clone(), rhs.clone()],
            ),
        },
    ]
}

fn logic_semantics(
    instruction: &Instruction,
    state: &mut SymbolicState,
    op: &str,
) -> Result<InstructionSemantics, LiftError> {
    let (dst, lhs, rhs) = expect_three(instruction)?;
    let width = register_width(dst);
    let ctor_name = match op {
        "and" => "bvand",
        "orr" => "bvor",
        "eor" => "bvxor",
        _ => unreachable!(),
    };
    let value = ctor(
        &format!("{ctor_name}{width}"),
        vec![state.read(lhs), operand_term(rhs, state, width)?],
    );
    Ok(write_next(canonical_register(dst), value))
}

fn shift_semantics(
    instruction: &Instruction,
    state: &mut SymbolicState,
    op: &str,
) -> Result<InstructionSemantics, LiftError> {
    let (dst, lhs, rhs) = expect_three(instruction)?;
    let width = register_width(dst);
    let ctor_name = match op {
        "lsl" => "bvshl",
        "lsr" => "bvlshr",
        "asr" => "bvashr",
        _ => unreachable!(),
    };
    let value = ctor(
        &format!("{ctor_name}{width}"),
        vec![state.read(lhs), operand_term(rhs, state, width)?],
    );
    Ok(write_next(canonical_register(dst), value))
}

fn load_semantics(
    instruction: &Instruction,
    state: &mut SymbolicState,
    byte: bool,
) -> Result<InstructionSemantics, LiftError> {
    let (dst, mem) = expect_two(instruction)?;
    let width = if byte { 8 } else { register_width(dst) };
    let addr = memory_address(mem, state)?;
    let load = ctor(
        &format!("mem_load{width}"),
        vec![var("memory"), addr.clone()],
    );
    let value = if byte && register_width(dst) != 8 {
        ctor(&format!("zext8to{}", register_width(dst)), vec![load])
    } else {
        load
    };
    Ok(InstructionSemantics {
        preconditions: vec![
            atomic(&format!("aligned{width}"), vec![addr.clone()]),
            atomic(&format!("valid_read{width}"), vec![addr.clone()]),
        ],
        updates: vec![StateUpdate {
            target: canonical_register(dst),
            value,
        }],
        effects: vec![AsmEffect::MemRead(address_name(mem))],
        transfer: Transfer::Next,
    })
}

fn store_semantics(
    instruction: &Instruction,
    state: &mut SymbolicState,
    byte: bool,
) -> Result<InstructionSemantics, LiftError> {
    let (src, mem) = expect_two(instruction)?;
    let width = if byte { 8 } else { register_width(src) };
    let addr = memory_address(mem, state)?;
    let value = state.read(src);
    let next_mem = ctor(
        &format!("mem_store{width}"),
        vec![var("memory"), addr.clone(), value],
    );
    Ok(InstructionSemantics {
        preconditions: vec![
            atomic(&format!("aligned{width}"), vec![addr.clone()]),
            atomic(&format!("valid_write{width}"), vec![addr.clone()]),
        ],
        updates: vec![StateUpdate {
            target: "memory".to_string(),
            value: next_mem,
        }],
        effects: vec![AsmEffect::MemWrite(address_name(mem))],
        transfer: Transfer::Next,
    })
}

fn cbz_semantics(
    instruction: &Instruction,
    state: &mut SymbolicState,
    branch_on_zero: bool,
) -> Result<InstructionSemantics, LiftError> {
    let (reg, label) = expect_two(instruction)?;
    let width = register_width(reg);
    let base = eq(state.read(reg), const_bv(0, width));
    let condition = if branch_on_zero {
        condition_from_formula(base)
    } else {
        not_condition(condition_from_formula(base))
    };
    Ok(InstructionSemantics {
        preconditions: Vec::new(),
        updates: Vec::new(),
        effects: Vec::new(),
        transfer: Transfer::Conditional {
            condition: Box::new(condition),
            target: label.to_string(),
        },
    })
}

fn tbz_semantics(
    instruction: &Instruction,
    state: &mut SymbolicState,
    branch_on_zero: bool,
) -> Result<InstructionSemantics, LiftError> {
    let (reg, bit, label) = expect_three(instruction)?;
    let width = register_width(reg);
    let bit_term = immediate_term(bit, width)?;
    let test = eq(
        ctor(&format!("bit{width}"), vec![state.read(reg), bit_term]),
        const_bv(0, 1),
    );
    let condition = if branch_on_zero {
        condition_from_formula(test)
    } else {
        not_condition(condition_from_formula(test))
    };
    Ok(InstructionSemantics {
        preconditions: Vec::new(),
        updates: Vec::new(),
        effects: Vec::new(),
        transfer: Transfer::Conditional {
            condition: Box::new(condition),
            target: label.to_string(),
        },
    })
}

fn branch_condition_semantics(
    instruction: &Instruction,
    state: &mut SymbolicState,
) -> Result<InstructionSemantics, LiftError> {
    let label = expect_one(instruction)?.to_string();
    let cond_name = instruction.mnemonic.trim_start_matches("b.");
    let condition = flag_condition(cond_name, state)?;
    Ok(InstructionSemantics {
        preconditions: Vec::new(),
        updates: Vec::new(),
        effects: Vec::new(),
        transfer: Transfer::Conditional {
            condition: Box::new(condition),
            target: label,
        },
    })
}

fn flag_condition(cond: &str, state: &mut SymbolicState) -> Result<Condition, LiftError> {
    let z = state.read("Z");
    let n = state.read("N");
    let v = state.read("V");
    let c = state.read("C");
    let formula = match cond {
        "eq" => eq(z, bool_term(true)),
        "ne" => eq(z, bool_term(false)),
        "cs" | "hs" => eq(c, bool_term(true)),
        "cc" | "lo" => eq(c, bool_term(false)),
        "mi" => eq(n, bool_term(true)),
        "pl" => eq(n, bool_term(false)),
        "vs" => eq(v, bool_term(true)),
        "vc" => eq(v, bool_term(false)),
        "lt" => ne_formula(n, v),
        "ge" => eq(n, v),
        "gt" => and_formula(vec![eq(z, bool_term(false)), eq(n, v)]),
        "le" => or_formula(vec![eq(z, bool_term(true)), ne_formula(n, v)]),
        "al" => true_formula(),
        _ => {
            return Err(LiftError::Parse {
                path: "instruction".to_string(),
                message: format!("unsupported condition code {cond}"),
            })
        }
    };
    Ok(condition_from_formula(formula))
}

fn operand_term(op: &str, state: &mut SymbolicState, width: u8) -> Result<IrTerm, LiftError> {
    let trimmed = op.trim();
    if is_register(trimmed) {
        Ok(state.read(trimmed))
    } else {
        immediate_term(trimmed, width)
    }
}

fn immediate_term(op: &str, width: u8) -> Result<IrTerm, LiftError> {
    let value = parse_immediate(op).ok_or_else(|| LiftError::Parse {
        path: "instruction".to_string(),
        message: format!("expected immediate operand, got {op}"),
    })?;
    Ok(const_bv(value, width))
}

fn parse_immediate(op: &str) -> Option<i64> {
    let trimmed = op.trim().trim_start_matches('#');
    let negative = trimmed.starts_with('-');
    let body = trimmed.trim_start_matches('-');
    let value = if let Some(hex) = body.strip_prefix("0x") {
        i64::from_str_radix(hex, 16).ok()?
    } else {
        body.parse::<i64>().ok()?
    };
    Some(if negative { -value } else { value })
}

fn memory_address(mem: &str, state: &mut SymbolicState) -> Result<IrTerm, LiftError> {
    let inner = mem
        .trim()
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| LiftError::Parse {
            path: "instruction".to_string(),
            message: format!("expected memory operand, got {mem}"),
        })?;
    let parts = split_operands(inner);
    if parts.is_empty() {
        return Err(LiftError::Parse {
            path: "instruction".to_string(),
            message: "empty memory operand".to_string(),
        });
    }
    let base = state.read(&parts[0]);
    if parts.len() == 1 {
        Ok(base)
    } else {
        Ok(ctor(
            "addr_add64",
            vec![base, immediate_term(&parts[1], 64)?],
        ))
    }
}

fn address_name(mem: &str) -> String {
    mem.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .next()
        .unwrap_or(mem)
        .trim()
        .to_string()
}

fn shift_operand(op: &str) -> Result<i64, LiftError> {
    let lower = op.trim().to_ascii_lowercase();
    let value = lower
        .strip_prefix("lsl")
        .map(str::trim)
        .and_then(parse_immediate)
        .ok_or_else(|| LiftError::Parse {
            path: "instruction".to_string(),
            message: format!("expected lsl shift, got {op}"),
        })?;
    Ok(value)
}

fn expect_one(instruction: &Instruction) -> Result<&str, LiftError> {
    if instruction.operands.len() == 1 {
        Ok(&instruction.operands[0])
    } else {
        Err(arity_error(instruction, 1))
    }
}

fn expect_two(instruction: &Instruction) -> Result<(&str, &str), LiftError> {
    if instruction.operands.len() == 2 {
        Ok((&instruction.operands[0], &instruction.operands[1]))
    } else {
        Err(arity_error(instruction, 2))
    }
}

fn expect_at_least_two(instruction: &Instruction) -> Result<(&str, &str), LiftError> {
    if instruction.operands.len() >= 2 {
        Ok((&instruction.operands[0], &instruction.operands[1]))
    } else {
        Err(arity_error(instruction, 2))
    }
}

fn expect_three(instruction: &Instruction) -> Result<(&str, &str, &str), LiftError> {
    if instruction.operands.len() == 3 {
        Ok((
            &instruction.operands[0],
            &instruction.operands[1],
            &instruction.operands[2],
        ))
    } else {
        Err(arity_error(instruction, 3))
    }
}

fn arity_error(instruction: &Instruction, expected: usize) -> LiftError {
    LiftError::Parse {
        path: "instruction".to_string(),
        message: format!(
            "{} expects {expected} operands, got {}",
            instruction.mnemonic,
            instruction.operands.len()
        ),
    }
}

impl AsmEffect {
    fn display_name(&self) -> String {
        match self {
            Self::MemRead(target) => format!("MemRead:{target}"),
            Self::MemWrite(target) => format!("MemWrite:{target}"),
            Self::Call(target) => format!("Call:{target}"),
            Self::Trap(reason) => format!("Trap:{reason}"),
        }
    }
}

fn parse_objdump_text(path: &str, source: &str) -> Result<AssemblyUnit, LiftError> {
    let mut normalized = String::new();
    for line in source.lines() {
        if let Some(label) = objdump_label(line) {
            normalized.push_str(&label);
            normalized.push_str(":\n");
            continue;
        }
        if let Some(instr) = objdump_instruction(line) {
            normalized.push_str("    ");
            normalized.push_str(&instr);
            normalized.push('\n');
        }
    }
    parse_assembly_text(path, &normalized)
}

fn objdump_label(line: &str) -> Option<String> {
    let start = line.find('<')?;
    let end = line[start + 1..].find(">:")? + start + 1;
    Some(line[start + 1..end].to_string())
}

fn objdump_instruction(line: &str) -> Option<String> {
    let colon = line.find(':')?;
    let mut rest = line[colon + 1..].trim();
    while let Some((first, tail)) = rest.split_once(char::is_whitespace) {
        if first.chars().all(|c| c.is_ascii_hexdigit()) && first.len() <= 8 {
            rest = tail.trim_start();
        } else {
            break;
        }
    }
    if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    }
}

fn parse_lines<'a, I>(path: &str, lines: I) -> Result<AssemblyUnit, LiftError>
where
    I: Iterator<Item = (usize, &'a str)>,
{
    let mut functions = Vec::new();
    let mut current: Option<AsmFunction> = None;
    let mut diagnostics = Vec::new();

    for (line_no, raw) in lines {
        let mut line = strip_comment(raw).trim().to_string();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('.') && !line.contains(':') {
            continue;
        }

        while let Some((label, rest)) = split_label(&line) {
            let is_function = !label.starts_with('.');
            if is_function {
                if let Some(func) = current.take() {
                    if !func.instructions.is_empty() {
                        functions.push(func);
                    }
                }
                let mut labels = HashMap::new();
                labels.insert(label.clone(), 0);
                current = Some(AsmFunction {
                    name: label,
                    labels,
                    instructions: Vec::new(),
                    line: line_no,
                });
            } else if let Some(func) = current.as_mut() {
                func.labels.insert(label, func.instructions.len());
            } else {
                diagnostics.push(format!("label without function at {path}:{line_no}"));
            }
            line = rest.trim().to_string();
            if line.is_empty() {
                break;
            }
        }

        if line.is_empty() || line.starts_with('.') {
            continue;
        }

        let instruction = parse_instruction(line_no, &line)?;
        if let Some(func) = current.as_mut() {
            func.instructions.push(instruction);
        } else {
            diagnostics.push(format!(
                "instruction before function label at {path}:{line_no}"
            ));
        }
    }

    if let Some(func) = current.take() {
        if !func.instructions.is_empty() {
            functions.push(func);
        }
    }

    Ok(AssemblyUnit {
        path: path.to_string(),
        functions,
        diagnostics,
    })
}

fn strip_comment(line: &str) -> &str {
    let slash = line.find("//");
    let semicolon = line.find(';');
    match (slash, semicolon) {
        (Some(a), Some(b)) => &line[..a.min(b)],
        (Some(a), None) => &line[..a],
        (None, Some(b)) => &line[..b],
        (None, None) => line,
    }
}

fn split_label(line: &str) -> Option<(String, String)> {
    let colon = line.find(':')?;
    let candidate = line[..colon].trim();
    if candidate.is_empty()
        || candidate
            .chars()
            .any(|c| c.is_whitespace() || c == '[' || c == ']')
    {
        return None;
    }
    Some((candidate.to_string(), line[colon + 1..].to_string()))
}

fn parse_instruction(line: usize, text: &str) -> Result<Instruction, LiftError> {
    let mut parts = text.trim().splitn(2, char::is_whitespace);
    let mnemonic = parts
        .next()
        .ok_or_else(|| LiftError::Parse {
            path: "instruction".to_string(),
            message: "missing mnemonic".to_string(),
        })?
        .trim()
        .to_ascii_lowercase();
    let operands = parts.next().map(split_operands).unwrap_or_default();
    Ok(Instruction {
        mnemonic,
        operands,
        text: text.trim().to_string(),
        line,
    })
}

fn split_operands(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut bracket_depth = 0usize;
    for ch in text.chars() {
        match ch {
            '[' => {
                bracket_depth += 1;
                current.push(ch);
            }
            ']' => {
                bracket_depth = bracket_depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if bracket_depth == 0 => {
                let item = current.trim();
                if !item.is_empty() {
                    out.push(item.to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let item = current.trim();
    if !item.is_empty() {
        out.push(item.to_string());
    }
    out
}

fn disassemble_with_objdump(path: &Path) -> Result<String, LiftError> {
    let output = Command::new("objdump")
        .arg("-d")
        .arg(path)
        .output()
        .map_err(|err| LiftError::Objdump {
            path: path.to_string_lossy().to_string(),
            message: err.to_string(),
        })?;
    if !output.status.success() {
        return Err(LiftError::Objdump {
            path: path.to_string_lossy().to_string(),
            message: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn resolve_path(workspace: &Path, source_path: &str) -> PathBuf {
    let path = PathBuf::from(source_path);
    if path.is_absolute() {
        path
    } else {
        workspace.join(path)
    }
}

fn is_assembly_path(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("s" | "S" | "asm")
    )
}

fn canonical_register(reg: &str) -> String {
    let reg = reg.trim().to_ascii_lowercase();
    match reg.as_str() {
        "fp" => "x29".to_string(),
        "lr" => "x30".to_string(),
        "wzr" => "wzr".to_string(),
        "xzr" => "xzr".to_string(),
        "sp" | "wsp" => reg,
        "n" => "N".to_string(),
        "z" => "Z".to_string(),
        "c" => "C".to_string(),
        "v" => "V".to_string(),
        _ => reg,
    }
}

fn is_register(op: &str) -> bool {
    let lower = op.trim().to_ascii_lowercase();
    matches!(lower.as_str(), "fp" | "lr" | "sp" | "wsp" | "xzr" | "wzr")
        || lower == "n"
        || lower == "z"
        || lower == "c"
        || lower == "v"
        || (lower.len() >= 2
            && (lower.starts_with('x') || lower.starts_with('w'))
            && lower[1..].chars().all(|c| c.is_ascii_digit()))
}

fn register_width(reg: &str) -> u8 {
    let lower = reg.trim().to_ascii_lowercase();
    if lower.starts_with('w') {
        32
    } else if matches!(lower.as_str(), "n" | "z" | "c" | "v") {
        1
    } else {
        64
    }
}

fn sort_for_name(name: &str) -> Sort {
    match name {
        "memory" => memory_sort(),
        "N" | "Z" | "C" | "V" => bool_sort(),
        reg if reg.starts_with('w') => bitvec_sort(32),
        reg if reg.starts_with('x') => bitvec_sort(64),
        _ => state_sort(),
    }
}

fn bitvec_sort(width: u8) -> Sort {
    Sort::Primitive {
        name: format!("BitVector{width}"),
    }
}

fn bool_sort() -> Sort {
    Sort::Primitive {
        name: "Bool".to_string(),
    }
}

fn memory_sort() -> Sort {
    Sort::Primitive {
        name: "Memory".to_string(),
    }
}

fn state_sort() -> Sort {
    Sort::Primitive {
        name: "AArch64State".to_string(),
    }
}

fn var(name: impl Into<String>) -> IrTerm {
    IrTerm::Var { name: name.into() }
}

fn const_bv(value: i64, width: u8) -> IrTerm {
    IrTerm::Const {
        value: Json::Number(value.into()),
        sort: bitvec_sort(width),
    }
}

fn bool_term(value: bool) -> IrTerm {
    IrTerm::Const {
        value: Json::Bool(value),
        sort: bool_sort(),
    }
}

fn ctor(name: &str, args: Vec<IrTerm>) -> IrTerm {
    IrTerm::Ctor {
        name: name.to_string(),
        args,
    }
}

fn atomic(name: &str, args: Vec<IrTerm>) -> IrFormula {
    IrFormula::Atomic {
        name: name.to_string(),
        args,
    }
}

fn eq(lhs: IrTerm, rhs: IrTerm) -> IrFormula {
    atomic("=", vec![lhs, rhs])
}

fn ne_formula(lhs: IrTerm, rhs: IrTerm) -> IrFormula {
    atomic("!=", vec![lhs, rhs])
}

fn true_formula() -> IrFormula {
    atomic("true", vec![])
}

fn and_formula(mut operands: Vec<IrFormula>) -> IrFormula {
    operands.retain(|item| !matches!(item, IrFormula::Atomic { name, args } if name == "true" && args.is_empty()));
    match operands.len() {
        0 => true_formula(),
        1 => operands.remove(0),
        _ => IrFormula::And { operands },
    }
}

fn or_formula(mut operands: Vec<IrFormula>) -> IrFormula {
    match operands.len() {
        0 => IrFormula::Atomic {
            name: "false".to_string(),
            args: vec![],
        },
        1 => operands.remove(0),
        _ => IrFormula::Or { operands },
    }
}

fn not_formula(formula: IrFormula) -> IrFormula {
    IrFormula::Not {
        operands: vec![formula],
    }
}

fn implies(lhs: IrFormula, rhs: IrFormula) -> IrFormula {
    IrFormula::Implies {
        operands: vec![lhs, rhs],
    }
}

fn true_condition() -> Condition {
    Condition {
        formula: true_formula(),
        term: ctor("true", vec![]),
    }
}

fn condition_from_formula(formula: IrFormula) -> Condition {
    let term = predicate_term(&formula);
    Condition { formula, term }
}

fn and_condition(lhs: Condition, rhs: Condition) -> Condition {
    Condition {
        formula: and_formula(vec![lhs.formula, rhs.formula]),
        term: ctor("and", vec![lhs.term, rhs.term]),
    }
}

fn not_condition(condition: Condition) -> Condition {
    Condition {
        formula: not_formula(condition.formula),
        term: ctor("not", vec![condition.term]),
    }
}

fn predicate_term(formula: &IrFormula) -> IrTerm {
    match formula {
        IrFormula::Atomic { name, args } => ctor(name, args.clone()),
        IrFormula::And { operands } => ctor("and", operands.iter().map(predicate_term).collect()),
        IrFormula::Or { operands } => ctor("or", operands.iter().map(predicate_term).collect()),
        IrFormula::Not { operands } => ctor("not", operands.iter().map(predicate_term).collect()),
        IrFormula::Implies { operands } => {
            ctor("implies", operands.iter().map(predicate_term).collect())
        }
        IrFormula::Forall { .. } | IrFormula::Exists { .. } | IrFormula::Choice { .. } => {
            ctor("predicate", vec![])
        }
        // wp-rule schema nodes (spec 2026-05-13-wp-as-formula.md §2.3):
        // `substitute` / `apply` appear only inside an unreduced `wp_rule`
        // term and are eliminated by `libprovekit::wp` before any formula
        // reaches a lifter. The aarch64 lifter never builds such formulas;
        // reaching this arm is a bug.
        IrFormula::Substitute { .. } | IrFormula::Apply { .. } => {
            unreachable!(
                "wp-rule schema node reached the aarch64 predicate-term builder; \
                 must be reduced via libprovekit::wp first"
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_operands_keeps_memory_operand_together() {
        assert_eq!(
            split_operands("x0, [x1, #8]"),
            vec!["x0".to_string(), "[x1, #8]".to_string()]
        );
    }

    #[test]
    fn immediate_parser_handles_negative_decimal_and_hex() {
        assert_eq!(parse_immediate("#-22"), Some(-22));
        assert_eq!(parse_immediate("#0x10"), Some(16));
    }
}
