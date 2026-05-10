use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::ffi::OsStr;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CanonicalValue};
use provekit_ir_types::{IrFormula, IrTerm, Sort};
use serde::Serialize;
use serde_json::json;

static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub const SURFACE: &str = "x86-64:sysv";

pub const CORE_INSTRUCTION_SUBSET: &[&str] = &[
    "mov", "movzx", "movsx", "lea", "add", "sub", "inc", "dec", "and", "or", "xor", "shl", "shr",
    "sar", "cmp", "test", "push", "pop", "call", "ret", "leave", "jmp", "je", "jne", "jl", "jle",
    "jg", "jge", "jz", "jnz", "js", "jns", "nop",
];

#[derive(Debug, thiserror::Error)]
pub enum LiftError {
    #[error("unsupported source extension for {0}")]
    UnsupportedSource(String),
    #[error("failed to run {program}: {message}")]
    Tool { program: String, message: String },
    #[error("disassembly parse error: {0}")]
    Disassembly(String),
    #[error("unsupported instruction {mnemonic} at 0x{address:x}")]
    UnsupportedInstruction { mnemonic: String, address: u64 },
    #[error("unsupported operand {0}")]
    UnsupportedOperand(String),
    #[error("unsupported control flow in {function}: {reason}")]
    UnsupportedControlFlow { function: String, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Refusal {
    pub kind: String,
    pub function: String,
    pub address: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct LiftResult {
    pub contracts: Vec<FunctionContractMemento>,
    pub diagnostics: Vec<String>,
    pub refusals: Vec<Refusal>,
}

#[derive(Debug, Clone)]
pub struct FunctionContractMemento {
    pub fn_name: String,
    pub formals: Vec<String>,
    pub formal_sorts: Vec<Sort>,
    pub formal_regions: Vec<Option<String>>,
    pub return_sort: Sort,
    pub return_region: Option<String>,
    pub pre: IrFormula,
    pub post: IrFormula,
    pub body_cid: Option<String>,
    pub effects: EffectSet,
    pub locus: Locus,
    pub canonical_bytes: Vec<u8>,
    pub cid: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EffectSet {
    pub effects: Vec<Effect>,
}

impl EffectSet {
    fn empty() -> Self {
        Self {
            effects: Vec::new(),
        }
    }

    fn add(&mut self, effect: Effect) {
        if !self.effects.iter().any(|existing| existing == &effect) {
            self.effects.push(effect);
        }
    }

    fn to_value(&self) -> Arc<CanonicalValue> {
        let mut effects = self.effects.clone();
        effects.sort_by_key(Effect::sort_key);
        CanonicalValue::array(effects.iter().map(Effect::to_value).collect())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Reads { target: String },
    Writes { target: String },
    Io,
    Panics,
    UnresolvedCall { name: String },
}

impl Effect {
    fn to_value(&self) -> Arc<CanonicalValue> {
        match self {
            Self::Reads { target } => CanonicalValue::object([
                ("kind", CanonicalValue::string("reads")),
                ("target", CanonicalValue::string(target.clone())),
            ]),
            Self::Writes { target } => CanonicalValue::object([
                ("kind", CanonicalValue::string("writes")),
                ("target", CanonicalValue::string(target.clone())),
            ]),
            Self::Io => CanonicalValue::object([("kind", CanonicalValue::string("io"))]),
            Self::Panics => CanonicalValue::object([("kind", CanonicalValue::string("panics"))]),
            Self::UnresolvedCall { name } => CanonicalValue::object([
                ("kind", CanonicalValue::string("unresolved_call")),
                ("name", CanonicalValue::string(name.clone())),
            ]),
        }
    }

    fn sort_key(&self) -> String {
        match self {
            Self::Reads { target } => format!("0:reads:{target}"),
            Self::Writes { target } => format!("1:writes:{target}"),
            Self::Io => "2:io".to_string(),
            Self::Panics => "3:panics".to_string(),
            Self::UnresolvedCall { name } => format!("4:unresolved:{name}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Locus {
    pub file: Option<String>,
    pub line: usize,
    pub col: usize,
}

impl Locus {
    fn to_value(&self) -> Arc<CanonicalValue> {
        CanonicalValue::object([
            (
                "file",
                match &self.file {
                    Some(file) => CanonicalValue::string(file.clone()),
                    None => CanonicalValue::null(),
                },
            ),
            ("line", CanonicalValue::integer(self.line as i64)),
            ("col", CanonicalValue::integer(self.col as i64)),
        ])
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    pub address: u64,
    pub mnemonic: String,
    pub operands: Vec<String>,
    pub text: String,
    pub target: Option<u64>,
}

impl Instruction {
    pub fn new_for_test(mnemonic: &str, operands: &[&str]) -> Self {
        Self {
            address: 0,
            mnemonic: mnemonic.to_ascii_lowercase(),
            operands: operands.iter().map(|operand| operand.to_string()).collect(),
            text: format!("{} {}", mnemonic, operands.join(", ")),
            target: None,
        }
    }
}

#[derive(Debug, Clone)]
struct FunctionStream {
    name: String,
    source_path: PathBuf,
    instructions: Vec<Instruction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expr(String);

impl Expr {
    fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    fn call(name: &str, args: &[Expr]) -> Self {
        let rendered = args
            .iter()
            .map(|arg| arg.0.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        Self(format!("{name}({rendered})"))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone)]
pub struct MachineState {
    registers: BTreeMap<String, Expr>,
    written_views: BTreeMap<String, Expr>,
    flags: BTreeMap<String, Expr>,
    flag_formulas: BTreeMap<String, IrFormula>,
    memory: Expr,
    effects: BTreeSet<String>,
    preconditions: Vec<IrFormula>,
}

impl MachineState {
    pub fn entry() -> Self {
        let mut registers = BTreeMap::new();
        for reg in GP_REGS {
            registers.insert((*reg).to_string(), Expr::new(*reg));
        }

        let mut flags = BTreeMap::new();
        for flag in ["CF", "PF", "AF", "ZF", "SF", "OF"] {
            flags.insert(flag.to_string(), Expr::new(flag));
        }

        Self {
            registers,
            written_views: BTreeMap::new(),
            flags,
            flag_formulas: BTreeMap::new(),
            memory: Expr::new("memory"),
            effects: BTreeSet::new(),
            preconditions: Vec::new(),
        }
    }

    pub fn register_expr(&self, register: &str) -> &Expr {
        let canonical = canonical_register(register).unwrap_or(register);
        self.registers
            .get(canonical)
            .unwrap_or_else(|| panic!("unknown register {register}"))
    }

    pub fn flag_expr(&self, flag: &str) -> &Expr {
        self.flags
            .get(flag)
            .unwrap_or_else(|| panic!("unknown flag {flag}"))
    }

    pub fn effects(&self) -> &BTreeSet<String> {
        &self.effects
    }

    fn write_register(&mut self, register: &str, value: Expr) -> Result<(), LiftError> {
        let info = reg_info(register)
            .ok_or_else(|| LiftError::UnsupportedOperand(register.to_string()))?;
        let next = match info.bits {
            64 => value.clone(),
            32 => Expr::call("zext32", std::slice::from_ref(&value)),
            16 => Expr::call(
                "write16",
                &[
                    self.registers
                        .get(info.canonical)
                        .cloned()
                        .unwrap_or_else(|| Expr::new(info.canonical)),
                    value.clone(),
                ],
            ),
            8 => Expr::call(
                "write8",
                &[
                    self.registers
                        .get(info.canonical)
                        .cloned()
                        .unwrap_or_else(|| Expr::new(info.canonical)),
                    value.clone(),
                ],
            ),
            _ => return Err(LiftError::UnsupportedOperand(register.to_string())),
        };
        self.registers.insert(info.canonical.to_string(), next);
        self.written_views
            .insert(info.view.to_string(), simplify_view_expr(&value));
        Ok(())
    }

    fn read_register(&self, register: &str) -> Result<Expr, LiftError> {
        let info = reg_info(register)
            .ok_or_else(|| LiftError::UnsupportedOperand(register.to_string()))?;
        let base = self
            .registers
            .get(info.canonical)
            .cloned()
            .unwrap_or_else(|| Expr::new(info.canonical));
        match info.bits {
            64 => Ok(base),
            32 => Ok(Expr::call("low32", &[base])),
            16 => Ok(Expr::call("low16", &[base])),
            8 => Ok(Expr::call("low8", &[base])),
            _ => Err(LiftError::UnsupportedOperand(register.to_string())),
        }
    }

    fn set_flag_expr(&mut self, flag: &str, expr: Expr) {
        self.flags.insert(flag.to_string(), expr);
        self.flag_formulas.remove(flag);
    }

    fn set_flag_formula(&mut self, flag: &str, expr: Expr, formula: IrFormula) {
        self.flags.insert(flag.to_string(), expr);
        self.flag_formulas.insert(flag.to_string(), formula);
    }

    fn flag_formula(&self, flag: &str) -> IrFormula {
        self.flag_formulas.get(flag).cloned().unwrap_or_else(|| {
            atomic_predicate("x86.flag", vec![string_term(self.flag_expr(flag).as_str())])
        })
    }
}

pub fn lift_paths(
    workspace_root: impl AsRef<Path>,
    source_paths: &[String],
) -> Result<LiftResult, LiftError> {
    let workspace_root = workspace_root.as_ref();
    let mut diagnostics = Vec::new();
    let mut refusals = Vec::new();
    let mut contracts = Vec::new();

    for source_path in source_paths {
        let absolute = resolve_path(workspace_root, source_path);
        let streams = match disassemble_path(&absolute) {
            Ok(streams) => streams,
            Err(err) => {
                refusals.push(Refusal {
                    kind: "disassembly-failed".to_string(),
                    function: source_path.clone(),
                    address: None,
                    reason: err.to_string(),
                });
                continue;
            }
        };

        let lifted = lift_streams(source_path, streams);
        contracts.extend(lifted.contracts);
        diagnostics.extend(lifted.diagnostics);
        refusals.extend(lifted.refusals);
    }

    Ok(LiftResult {
        contracts,
        diagnostics,
        refusals,
    })
}

pub fn lift_disassembly_text(
    source_path: impl AsRef<Path>,
    disassembly: &str,
) -> Result<LiftResult, LiftError> {
    let source_path = source_path.as_ref();
    let streams = parse_objdump(disassembly, source_path)?;
    let source_path = source_path.to_string_lossy();
    Ok(lift_streams(&source_path, streams))
}

fn lift_streams(source_path: &str, streams: Vec<FunctionStream>) -> LiftResult {
    let mut diagnostics = Vec::new();
    let mut refusals = Vec::new();
    let mut contracts = Vec::new();

    if streams.is_empty() {
        diagnostics.push(format!("no x86-64 functions found in {source_path}"));
    }

    for stream in streams {
        match lift_function(&stream) {
            Ok(contract) => contracts.push(contract),
            Err(err) => refusals.push(Refusal {
                kind: "function-refused".to_string(),
                function: stream.name,
                address: None,
                reason: err.to_string(),
            }),
        }
    }

    LiftResult {
        contracts,
        diagnostics,
        refusals,
    }
}

pub fn apply_instruction(
    state: &MachineState,
    instruction: &Instruction,
) -> Result<MachineState, LiftError> {
    let mut next = state.clone();
    apply_instruction_mut(&mut next, instruction)?;
    Ok(next)
}

fn apply_instruction_mut(
    state: &mut MachineState,
    instruction: &Instruction,
) -> Result<(), LiftError> {
    let mnemonic = normalize_mnemonic(&instruction.mnemonic);

    match mnemonic.as_str() {
        "nop" | "ret" | "jmp" | "je" | "jz" | "jne" | "jnz" | "jl" | "jle" | "jg" | "jge"
        | "js" | "jns" => Ok(()),

        // Intel SDM MOV. Future auto-gen source: sail-x86, MOV.
        "mov" => {
            let (dst, src) = two_operands(instruction)?;
            let bits = operand_width_hint(dst, Some(src)).unwrap_or(64);
            let value = read_operand(state, src, bits)?;
            write_operand(state, dst, value, bits)
        }

        // Intel SDM MOVZX. Future auto-gen source: sail-x86, MOVZX.
        "movzx" => {
            let (dst, src) = two_operands(instruction)?;
            let dst_bits = operand_width_hint(dst, Some(src)).unwrap_or(64);
            let src_bits = operand_width_hint(src, None).unwrap_or(8);
            let value = read_operand(state, src, src_bits)?;
            let extended = Expr::call(&format!("zext{src_bits}_to_{dst_bits}"), &[value]);
            write_operand(state, dst, extended, dst_bits)
        }

        // Intel SDM MOVSX. Future auto-gen source: sail-x86, MOVSX.
        "movsx" | "movsxd" => {
            let (dst, src) = two_operands(instruction)?;
            let dst_bits = operand_width_hint(dst, Some(src)).unwrap_or(64);
            let src_bits = operand_width_hint(src, None).unwrap_or(32);
            let value = read_operand(state, src, src_bits)?;
            let extended = Expr::call(&format!("sext{src_bits}_to_{dst_bits}"), &[value]);
            write_operand(state, dst, extended, dst_bits)
        }

        // Intel SDM LEA. Future auto-gen source: sail-x86, LEA.
        "lea" => {
            let (dst, src) = two_operands(instruction)?;
            let bits = operand_width_hint(dst, Some(src)).unwrap_or(64);
            let address = address_expr(src)?;
            write_operand(state, dst, address, bits)
        }

        // Intel SDM arithmetic and logic ops. Future auto-gen source: sail-x86, ADD/SUB/AND/OR/XOR/SHL/SHR/SAR.
        "add" | "sub" | "and" | "or" | "xor" | "shl" | "shr" | "sar" => {
            let (dst, src) = two_operands(instruction)?;
            let bits = operand_width_hint(dst, Some(src)).unwrap_or(64);
            let lhs = read_operand(state, dst, bits)?;
            let rhs = read_operand(state, src, bits)?;
            let result = binary_expr(mnemonic.as_str(), bits, lhs.clone(), rhs.clone());
            write_operand(state, dst, result.clone(), bits)?;
            update_flags_for_binary(state, mnemonic.as_str(), bits, lhs, rhs, result);
            Ok(())
        }

        // Intel SDM INC. Future auto-gen source: sail-x86, INC.
        "inc" => {
            let dst = one_operand(instruction)?;
            let bits = operand_width_hint(dst, None).unwrap_or(64);
            let lhs = read_operand(state, dst, bits)?;
            let one = Expr::new("0x1");
            let result = binary_expr("add", bits, lhs.clone(), one.clone());
            write_operand(state, dst, result.clone(), bits)?;
            update_flags_for_inc_dec(state, "inc", bits, lhs, one, result);
            Ok(())
        }

        // Intel SDM DEC. Future auto-gen source: sail-x86, DEC.
        "dec" => {
            let dst = one_operand(instruction)?;
            let bits = operand_width_hint(dst, None).unwrap_or(64);
            let lhs = read_operand(state, dst, bits)?;
            let one = Expr::new("0x1");
            let result = binary_expr("sub", bits, lhs.clone(), one.clone());
            write_operand(state, dst, result.clone(), bits)?;
            update_flags_for_inc_dec(state, "dec", bits, lhs, one, result);
            Ok(())
        }

        // Intel SDM CMP. Future auto-gen source: sail-x86, CMP.
        "cmp" => {
            let (lhs_op, rhs_op) = two_operands(instruction)?;
            let bits = operand_width_hint(lhs_op, Some(rhs_op)).unwrap_or(64);
            let lhs = read_operand(state, lhs_op, bits)?;
            let rhs = read_operand(state, rhs_op, bits)?;
            let result = binary_expr("sub", bits, lhs.clone(), rhs.clone());
            update_flags_for_binary(state, "cmp", bits, lhs.clone(), rhs, result);
            if let Some(formula) = eq_formula_for_operands(lhs_op, None, state, bits) {
                state.flag_formulas.insert("ZF".to_string(), formula);
            } else if let Some(formula) = eq_formula_for_two_operands(lhs_op, rhs_op) {
                state.flag_formulas.insert("ZF".to_string(), formula);
            }
            Ok(())
        }

        // Intel SDM TEST. Future auto-gen source: sail-x86, TEST.
        "test" => {
            let (lhs_op, rhs_op) = two_operands(instruction)?;
            let bits = operand_width_hint(lhs_op, Some(rhs_op)).unwrap_or(64);
            let lhs = read_operand(state, lhs_op, bits)?;
            let rhs = read_operand(state, rhs_op, bits)?;
            let result = binary_expr("and", bits, lhs, rhs);
            let zf_expr = Expr::call(&format!("eq{bits}"), &[result.clone(), Expr::new("0x0")]);
            if operands_same_register(lhs_op, rhs_op) {
                let formula = eq_zero_formula_for_operand(lhs_op);
                state.set_flag_formula("ZF", zf_expr, formula);
            } else {
                state.set_flag_expr("ZF", zf_expr);
            }
            state.set_flag_expr("SF", Expr::call(&format!("sign{bits}"), &[result]));
            state.set_flag_expr("CF", Expr::new("false"));
            state.set_flag_expr("OF", Expr::new("false"));
            Ok(())
        }

        // Intel SDM PUSH. Future auto-gen source: sail-x86, PUSH.
        "push" => {
            let src = one_operand(instruction)?;
            let value = read_operand(state, src, 64)?;
            let rsp = state.read_register("rsp")?;
            let next_rsp = Expr::call("sub64", &[rsp, Expr::new("0x8")]);
            state.write_register("rsp", next_rsp.clone())?;
            state.memory = Expr::call("store64", &[state.memory.clone(), next_rsp, value]);
            state.effects.insert("MemWrite".to_string());
            Ok(())
        }

        // Intel SDM POP. Future auto-gen source: sail-x86, POP.
        "pop" => {
            let dst = one_operand(instruction)?;
            let rsp = state.read_register("rsp")?;
            let value = Expr::call("load64", &[state.memory.clone(), rsp.clone()]);
            state.write_register(dst, value)?;
            let next_rsp = Expr::call("add64", &[rsp, Expr::new("0x8")]);
            state.write_register("rsp", next_rsp)?;
            state.effects.insert("MemRead".to_string());
            Ok(())
        }

        // Intel SDM CALL. Future auto-gen source: sail-x86, CALL.
        "call" => {
            let target = one_operand(instruction)?;
            if is_indirect_operand(target) {
                return Err(LiftError::UnsupportedOperand(target.to_string()));
            }
            state
                .effects
                .insert(format!("Call({})", target.trim().replace(' ', "")));
            Ok(())
        }

        // Intel SDM LEAVE. Future auto-gen source: sail-x86, LEAVE.
        "leave" => {
            let rbp = state.read_register("rbp")?;
            state.write_register("rsp", rbp.clone())?;
            let value = Expr::call("load64", &[state.memory.clone(), rbp.clone()]);
            state.write_register("rbp", value)?;
            let next_rsp = Expr::call("add64", &[rbp, Expr::new("0x8")]);
            state.write_register("rsp", next_rsp)?;
            state.effects.insert("MemRead".to_string());
            Ok(())
        }

        _ => Err(LiftError::UnsupportedInstruction {
            mnemonic: instruction.mnemonic.clone(),
            address: instruction.address,
        }),
    }
}

pub fn contract_to_json(contract: &FunctionContractMemento) -> serde_json::Value {
    let value = build_memento_value(contract);
    let bytes = jcs_bytes_of_value(&value);
    let mut json: serde_json::Value =
        serde_json::from_slice(&bytes).expect("canonical memento JSON parses");
    if let serde_json::Value::Object(map) = &mut json {
        map.insert(
            "cid".to_string(),
            serde_json::Value::String(contract.cid.clone()),
        );
    }
    json
}

pub fn ir_document_json(result: &LiftResult) -> serde_json::Value {
    let declarations = result
        .contracts
        .iter()
        .map(contract_to_json)
        .collect::<Vec<_>>();

    json!({
        "kind": "ir-document",
        "declarations": declarations.clone(),
        "ir": declarations,
        "diagnostics": &result.diagnostics,
        "refusals": &result.refusals
    })
}

pub fn lift_success_response_json(id: serde_json::Value, result: &LiftResult) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": ir_document_json(result)
    })
}

fn build_memento_value(contract: &FunctionContractMemento) -> Arc<CanonicalValue> {
    build_value(
        &contract.fn_name,
        &contract.formals,
        &contract.formal_sorts,
        &contract.return_sort,
        &contract.pre,
        &contract.post,
        contract.body_cid.as_deref(),
        &contract.effects,
        &contract.locus,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_value(
    fn_name: &str,
    formals: &[String],
    formal_sorts: &[Sort],
    return_sort: &Sort,
    pre: &IrFormula,
    post: &IrFormula,
    body_cid: Option<&str>,
    effects: &EffectSet,
    locus: &Locus,
) -> Arc<CanonicalValue> {
    let formal_values = formals
        .iter()
        .map(|formal| CanonicalValue::string(formal.clone()))
        .collect::<Vec<_>>();
    let sort_values = formal_sorts.iter().map(sort_to_value).collect::<Vec<_>>();
    let body_cid_value = body_cid
        .map(|cid| CanonicalValue::string(cid.to_string()))
        .unwrap_or_else(CanonicalValue::null);

    CanonicalValue::object([
        ("schemaVersion", CanonicalValue::string("1")),
        ("kind", CanonicalValue::string("function-contract")),
        ("fnName", CanonicalValue::string(fn_name.to_string())),
        ("formals", CanonicalValue::array(formal_values)),
        ("formalSorts", CanonicalValue::array(sort_values)),
        ("returnSort", sort_to_value(return_sort)),
        ("pre", formula_to_value(pre)),
        ("post", formula_to_value(post)),
        ("bodyCid", body_cid_value),
        ("effects", effects.to_value()),
        ("locus", locus.to_value()),
        ("autoMintedMementos", CanonicalValue::array(Vec::new())),
    ])
}

fn jcs_bytes_of_value(value: &CanonicalValue) -> Vec<u8> {
    encode_jcs(value).into_bytes()
}

fn cid_of_value(value: &CanonicalValue) -> String {
    blake3_512_of(&jcs_bytes_of_value(value))
}

fn sort_to_value(sort: &Sort) -> Arc<CanonicalValue> {
    serde_json_to_canonical(&serde_json::to_value(sort).expect("sort serializes"))
}

fn formula_to_value(formula: &IrFormula) -> Arc<CanonicalValue> {
    serde_json_to_canonical(&serde_json::to_value(formula).expect("formula serializes"))
}

fn serde_json_to_canonical(value: &serde_json::Value) -> Arc<CanonicalValue> {
    match value {
        serde_json::Value::Null => CanonicalValue::null(),
        serde_json::Value::Bool(value) => CanonicalValue::boolean(*value),
        serde_json::Value::Number(value) => {
            let integer = value.as_i64().unwrap_or_else(|| {
                value
                    .as_u64()
                    .and_then(|unsigned| i64::try_from(unsigned).ok())
                    .expect("canonical values must fit i64")
            });
            CanonicalValue::integer(integer)
        }
        serde_json::Value::String(value) => CanonicalValue::string(value.clone()),
        serde_json::Value::Array(values) => {
            CanonicalValue::array(values.iter().map(serde_json_to_canonical).collect())
        }
        serde_json::Value::Object(map) => CanonicalValue::object(
            map.iter()
                .map(|(key, value)| (key.clone(), serde_json_to_canonical(value)))
                .collect::<Vec<_>>(),
        ),
    }
}

fn lift_function(stream: &FunctionStream) -> Result<FunctionContractMemento, LiftError> {
    if stream.instructions.is_empty() {
        return Err(LiftError::UnsupportedControlFlow {
            function: stream.name.clone(),
            reason: "empty function".to_string(),
        });
    }

    let address_to_index = stream
        .instructions
        .iter()
        .enumerate()
        .map(|(index, instruction)| (instruction.address, index))
        .collect::<BTreeMap<_, _>>();

    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    explore_paths(
        stream,
        &address_to_index,
        0,
        MachineState::entry(),
        Vec::new(),
        &mut seen,
        &mut paths,
    )?;

    if paths.is_empty() {
        return Err(LiftError::UnsupportedControlFlow {
            function: stream.name.clone(),
            reason: "no return path found".to_string(),
        });
    }

    let pre = build_precondition(&paths);
    let post = build_postcondition(&paths);
    let effects = build_effect_set(&paths);
    let locus = Locus {
        file: Some(stream.source_path.to_string_lossy().to_string()),
        line: 0,
        col: 0,
    };
    let formals = machine_formals();
    let formal_sorts = machine_formal_sorts();
    let return_sort = primitive_sort("MachineState");

    let value: Arc<CanonicalValue> = build_value(
        &stream.name,
        &formals,
        &formal_sorts,
        &return_sort,
        &pre,
        &post,
        None,
        &effects,
        &locus,
    );
    let canonical_bytes = jcs_bytes_of_value(&value);
    let cid = cid_of_value(&value);

    Ok(FunctionContractMemento {
        fn_name: stream.name.clone(),
        formals,
        formal_sorts,
        formal_regions: vec![],
        return_sort,
        return_region: None,
        pre,
        post,
        body_cid: None,
        effects,
        locus,
        canonical_bytes,
        cid,
    })
}

#[derive(Debug, Clone)]
struct ReturnPath {
    conditions: Vec<IrFormula>,
    state: MachineState,
}

fn explore_paths(
    stream: &FunctionStream,
    address_to_index: &BTreeMap<u64, usize>,
    index: usize,
    state: MachineState,
    conditions: Vec<IrFormula>,
    seen: &mut HashSet<(usize, usize)>,
    paths: &mut Vec<ReturnPath>,
) -> Result<(), LiftError> {
    if index >= stream.instructions.len() {
        return Err(LiftError::UnsupportedControlFlow {
            function: stream.name.clone(),
            reason: "fell off the end of a function".to_string(),
        });
    }

    if !seen.insert((index, conditions.len())) {
        return Err(LiftError::UnsupportedControlFlow {
            function: stream.name.clone(),
            reason: "cycle requires a loop invariant memento".to_string(),
        });
    }

    let instruction = &stream.instructions[index];
    let mnemonic = normalize_mnemonic(&instruction.mnemonic);

    match mnemonic.as_str() {
        "ret" => {
            paths.push(ReturnPath { conditions, state });
            Ok(())
        }
        "jmp" => {
            let target = instruction
                .target
                .ok_or_else(|| LiftError::UnsupportedControlFlow {
                    function: stream.name.clone(),
                    reason: format!("indirect or unresolved jmp at 0x{:x}", instruction.address),
                })?;
            if target <= instruction.address {
                return Err(LiftError::UnsupportedControlFlow {
                    function: stream.name.clone(),
                    reason: "backward jmp requires a loop invariant memento".to_string(),
                });
            }
            let target_index = *address_to_index.get(&target).ok_or_else(|| {
                LiftError::UnsupportedControlFlow {
                    function: stream.name.clone(),
                    reason: format!("jmp target 0x{target:x} is outside function"),
                }
            })?;
            explore_paths(
                stream,
                address_to_index,
                target_index,
                state,
                conditions,
                seen,
                paths,
            )
        }
        "je" | "jz" | "jne" | "jnz" | "jl" | "jle" | "jg" | "jge" | "js" | "jns" => {
            let target = instruction
                .target
                .ok_or_else(|| LiftError::UnsupportedControlFlow {
                    function: stream.name.clone(),
                    reason: format!(
                        "unresolved conditional branch at 0x{:x}",
                        instruction.address
                    ),
                })?;
            if target <= instruction.address {
                return Err(LiftError::UnsupportedControlFlow {
                    function: stream.name.clone(),
                    reason:
                        "backward conditional branch recovered as a loop but needs an invariant"
                            .to_string(),
                });
            }
            let target_index = *address_to_index.get(&target).ok_or_else(|| {
                LiftError::UnsupportedControlFlow {
                    function: stream.name.clone(),
                    reason: format!("branch target 0x{target:x} is outside function"),
                }
            })?;
            let cond = branch_condition(&state, mnemonic.as_str());

            let mut taken_conditions = conditions.clone();
            taken_conditions.push(cond.clone());
            explore_paths(
                stream,
                address_to_index,
                target_index,
                state.clone(),
                taken_conditions,
                seen,
                paths,
            )?;

            let mut fallthrough_conditions = conditions;
            fallthrough_conditions.push(negate_formula(cond));
            explore_paths(
                stream,
                address_to_index,
                index + 1,
                state,
                fallthrough_conditions,
                seen,
                paths,
            )
        }
        "call" => {
            let mut next = state;
            apply_instruction_mut(&mut next, instruction)?;
            explore_paths(
                stream,
                address_to_index,
                index + 1,
                next,
                conditions,
                seen,
                paths,
            )
        }
        _ => {
            let mut next = state;
            apply_instruction_mut(&mut next, instruction)?;
            explore_paths(
                stream,
                address_to_index,
                index + 1,
                next,
                conditions,
                seen,
                paths,
            )
        }
    }
}

fn build_precondition(paths: &[ReturnPath]) -> IrFormula {
    let obligations = paths
        .iter()
        .flat_map(|path| {
            let guard = formula_and(path.conditions.clone());
            path.state
                .preconditions
                .iter()
                .cloned()
                .map(move |pre| implies_formula(guard.clone(), pre))
        })
        .collect::<Vec<_>>();
    formula_and(obligations)
}

fn build_postcondition(paths: &[ReturnPath]) -> IrFormula {
    let clauses = paths
        .iter()
        .flat_map(|path| {
            let guard = formula_and(path.conditions.clone());
            let outputs = if path.state.written_views.is_empty() {
                vec![true_formula()]
            } else {
                path.state
                    .written_views
                    .iter()
                    .filter(|(view, _)| is_return_view(view))
                    .map(|(view, value)| {
                        atomic_eq(
                            IrTerm::Var {
                                name: format!("{view}_post"),
                            },
                            expr_to_term(value),
                        )
                    })
                    .collect::<Vec<_>>()
            };
            outputs
                .into_iter()
                .map(move |post| implies_formula(guard.clone(), post))
        })
        .collect::<Vec<_>>();
    formula_and(clauses)
}

fn build_effect_set(paths: &[ReturnPath]) -> EffectSet {
    let mut set = EffectSet::empty();
    for path in paths {
        for effect in &path.state.effects {
            match effect.as_str() {
                "MemRead" => set.add(Effect::Reads {
                    target: "memory".to_string(),
                }),
                "MemWrite" => set.add(Effect::Writes {
                    target: "memory".to_string(),
                }),
                "IO" => set.add(Effect::Io),
                "Trap" => set.add(Effect::Panics),
                other if other.starts_with("Call(") => set.add(Effect::UnresolvedCall {
                    name: other.to_string(),
                }),
                _ => {}
            }
        }
    }
    set
}

fn branch_condition(state: &MachineState, mnemonic: &str) -> IrFormula {
    match mnemonic {
        "je" | "jz" => state.flag_formula("ZF"),
        "jne" | "jnz" => negate_formula(state.flag_formula("ZF")),
        "js" => state.flag_formula("SF"),
        "jns" => negate_formula(state.flag_formula("SF")),
        "jl" => atomic_predicate(
            "x86.cond.jl",
            vec![
                string_term(state.flag_expr("SF").as_str()),
                string_term(state.flag_expr("OF").as_str()),
            ],
        ),
        "jle" => atomic_predicate(
            "x86.cond.jle",
            vec![
                string_term(state.flag_expr("ZF").as_str()),
                string_term(state.flag_expr("SF").as_str()),
                string_term(state.flag_expr("OF").as_str()),
            ],
        ),
        "jg" => atomic_predicate(
            "x86.cond.jg",
            vec![
                string_term(state.flag_expr("ZF").as_str()),
                string_term(state.flag_expr("SF").as_str()),
                string_term(state.flag_expr("OF").as_str()),
            ],
        ),
        "jge" => atomic_predicate(
            "x86.cond.jge",
            vec![
                string_term(state.flag_expr("SF").as_str()),
                string_term(state.flag_expr("OF").as_str()),
            ],
        ),
        _ => atomic_predicate("x86.cond.unsupported", vec![string_term(mnemonic)]),
    }
}

fn disassemble_path(path: &Path) -> Result<Vec<FunctionStream>, LiftError> {
    let object_path;
    let ext = path.extension().and_then(OsStr::to_str).unwrap_or_default();
    let disassembly_input = match ext {
        "s" | "S" => {
            object_path = assemble_source_to_elf(path)?;
            object_path.as_path()
        }
        "o" => path,
        _ => {
            return Err(LiftError::UnsupportedSource(
                path.to_string_lossy().to_string(),
            ))
        }
    };

    let output = Command::new("objdump")
        .arg("-d")
        .arg("--x86-asm-syntax=intel")
        .arg("--no-show-raw-insn")
        .arg(disassembly_input)
        .output()
        .map_err(|err| LiftError::Tool {
            program: "objdump".to_string(),
            message: err.to_string(),
        })?;

    if !output.status.success() {
        return Err(LiftError::Tool {
            program: "objdump".to_string(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    let text = String::from_utf8_lossy(&output.stdout);
    parse_objdump(&text, path)
}

fn assemble_source_to_elf(path: &Path) -> Result<PathBuf, LiftError> {
    let id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "provekit-lift-asm-x86-64-{}-{id}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).map_err(|err| LiftError::Tool {
        program: "mkdir".to_string(),
        message: err.to_string(),
    })?;
    let object = dir.join("input.o");
    let output = Command::new("clang")
        .arg("-target")
        .arg("x86_64-linux-gnu")
        .arg("-c")
        .arg("-x")
        .arg("assembler")
        .arg("-m64")
        .arg(path)
        .arg("-o")
        .arg(&object)
        .output()
        .map_err(|err| LiftError::Tool {
            program: "clang".to_string(),
            message: err.to_string(),
        })?;

    if !output.status.success() {
        return Err(LiftError::Tool {
            program: "clang".to_string(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    Ok(object)
}

fn parse_objdump(text: &str, source_path: &Path) -> Result<Vec<FunctionStream>, LiftError> {
    let mut streams = Vec::new();
    let mut current: Option<FunctionStream> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some((address, symbol)) = parse_symbol_header(trimmed) {
            if let Some(stream) = current.take() {
                if !stream.instructions.is_empty() {
                    streams.push(stream);
                }
            }
            if symbol.starts_with('.') {
                current = None;
            } else {
                current = Some(FunctionStream {
                    name: symbol,
                    source_path: source_path.to_path_buf(),
                    instructions: Vec::new(),
                });
                let _ = address;
            }
            continue;
        }

        if let Some(instruction) = parse_instruction_line(trimmed)? {
            if let Some(stream) = &mut current {
                stream.instructions.push(instruction);
            }
        }
    }

    if let Some(stream) = current {
        if !stream.instructions.is_empty() {
            streams.push(stream);
        }
    }

    Ok(streams)
}

fn parse_symbol_header(line: &str) -> Option<(u64, String)> {
    let (addr, rest) = line.split_once(" <")?;
    let symbol = rest.strip_suffix(">:")?;
    let address = u64::from_str_radix(addr, 16).ok()?;
    Some((address, symbol.to_string()))
}

fn parse_instruction_line(line: &str) -> Result<Option<Instruction>, LiftError> {
    let Some((addr, rest)) = line.split_once(':') else {
        return Ok(None);
    };
    let address = match u64::from_str_radix(addr.trim(), 16) {
        Ok(address) => address,
        Err(_) => return Ok(None),
    };
    let rest = rest.trim();
    if rest.is_empty() {
        return Ok(None);
    }

    let (mnemonic, operand_text) = split_mnemonic(rest);
    let operands = split_operands(operand_text);
    let target = operands
        .first()
        .and_then(|operand| parse_branch_target(operand));

    Ok(Some(Instruction {
        address,
        mnemonic: normalize_mnemonic(mnemonic),
        operands,
        text: rest.to_string(),
        target,
    }))
}

fn split_mnemonic(text: &str) -> (&str, &str) {
    let mut parts = text.splitn(2, char::is_whitespace);
    let mnemonic = parts.next().unwrap_or_default();
    let operands = parts.next().unwrap_or_default().trim();
    (mnemonic, operands)
}

fn split_operands(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut operands = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (index, ch) in text.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => depth -= 1,
            ',' if depth == 0 => {
                operands.push(text[start..index].trim().to_string());
                start = index + 1;
            }
            _ => {}
        }
    }
    operands.push(text[start..].trim().to_string());
    operands
}

fn parse_branch_target(operand: &str) -> Option<u64> {
    let first = operand.split_whitespace().next()?;
    let hex = first.strip_prefix("0x").unwrap_or(first);
    if hex.is_empty() || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    u64::from_str_radix(hex, 16).ok()
}

fn resolve_path(workspace_root: &Path, source_path: &str) -> PathBuf {
    let path = Path::new(source_path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace_root.join(path)
    }
}

fn normalize_mnemonic(mnemonic: &str) -> String {
    let lower = mnemonic.trim().to_ascii_lowercase();
    for suffix in ["q", "l", "w", "b"] {
        if let Some(base) = lower.strip_suffix(suffix) {
            if CORE_INSTRUCTION_SUBSET.contains(&base) {
                return base.to_string();
            }
        }
    }
    lower
}

fn one_operand(instruction: &Instruction) -> Result<&str, LiftError> {
    instruction
        .operands
        .first()
        .map(String::as_str)
        .ok_or_else(|| LiftError::Disassembly(format!("{} missing operand", instruction.text)))
}

fn two_operands(instruction: &Instruction) -> Result<(&str, &str), LiftError> {
    if instruction.operands.len() != 2 {
        return Err(LiftError::Disassembly(format!(
            "{} expected two operands",
            instruction.text
        )));
    }
    Ok((&instruction.operands[0], &instruction.operands[1]))
}

fn read_operand(state: &mut MachineState, operand: &str, bits: u16) -> Result<Expr, LiftError> {
    let operand = clean_operand(operand);
    if is_register(&operand) {
        return state.read_register(&operand);
    }
    if is_memory_operand(&operand) {
        let address = address_expr(&operand)?;
        state
            .preconditions
            .push(atomic_predicate("valid", vec![expr_to_term(&address)]));
        state.effects.insert("MemRead".to_string());
        return Ok(Expr::call(
            &format!("load{bits}"),
            &[state.memory.clone(), address],
        ));
    }
    parse_immediate(&operand, bits).ok_or(LiftError::UnsupportedOperand(operand))
}

fn write_operand(
    state: &mut MachineState,
    operand: &str,
    value: Expr,
    bits: u16,
) -> Result<(), LiftError> {
    let operand = clean_operand(operand);
    if is_register(&operand) {
        return state.write_register(&operand, value);
    }
    if is_memory_operand(&operand) {
        let address = address_expr(&operand)?;
        state
            .preconditions
            .push(atomic_predicate("valid", vec![expr_to_term(&address)]));
        state.memory = Expr::call(
            &format!("store{bits}"),
            &[state.memory.clone(), address, value],
        );
        state.effects.insert("MemWrite".to_string());
        return Ok(());
    }
    Err(LiftError::UnsupportedOperand(operand))
}

fn clean_operand(operand: &str) -> String {
    operand
        .trim()
        .trim_start_matches("ptr ")
        .trim_start_matches("dword ptr ")
        .trim_start_matches("qword ptr ")
        .trim_start_matches("word ptr ")
        .trim_start_matches("byte ptr ")
        .to_ascii_lowercase()
}

fn is_register(operand: &str) -> bool {
    reg_info(operand).is_some()
}

fn is_memory_operand(operand: &str) -> bool {
    operand.contains('[') && operand.contains(']')
}

fn is_indirect_operand(operand: &str) -> bool {
    let operand = clean_operand(operand);
    is_register(&operand) || is_memory_operand(&operand) || operand.starts_with('*')
}

fn address_expr(operand: &str) -> Result<Expr, LiftError> {
    let operand = clean_operand(operand);
    if let Some(start) = operand.find('[') {
        let end = operand.rfind(']').ok_or_else(|| {
            LiftError::UnsupportedOperand(format!("unterminated memory operand {operand}"))
        })?;
        let inner = operand[start + 1..end].trim();
        if is_register(inner) {
            return Ok(Expr::new(inner));
        }
        return Ok(Expr::new(format!("addr({inner})")));
    }
    Err(LiftError::UnsupportedOperand(operand))
}

fn parse_immediate(operand: &str, bits: u16) -> Option<Expr> {
    let operand = operand.trim().trim_start_matches('$').trim();
    if operand.starts_with("0x") {
        return Some(Expr::new(operand.to_ascii_lowercase()));
    }
    let parsed = operand.parse::<i128>().ok()?;
    let modulus = 1i128.checked_shl(u32::from(bits))?;
    let normalized = if parsed < 0 { modulus + parsed } else { parsed };
    Some(Expr::new(format!("0x{normalized:x}")))
}

fn operand_width_hint(operand: &str, other: Option<&str>) -> Option<u16> {
    let operand = clean_operand(operand);
    if let Some(info) = reg_info(&operand) {
        return Some(info.bits);
    }
    if operand.starts_with("byte ptr") {
        return Some(8);
    }
    if operand.starts_with("word ptr") {
        return Some(16);
    }
    if operand.starts_with("dword ptr") {
        return Some(32);
    }
    if operand.starts_with("qword ptr") {
        return Some(64);
    }
    other.and_then(|other| operand_width_hint(other, None))
}

fn binary_expr(op: &str, bits: u16, lhs: Expr, rhs: Expr) -> Expr {
    let name = match op {
        "cmp" => "sub",
        other => other,
    };
    Expr::call(&format!("{name}{bits}"), &[lhs, rhs])
}

fn update_flags_for_binary(
    state: &mut MachineState,
    op: &str,
    bits: u16,
    lhs: Expr,
    rhs: Expr,
    result: Expr,
) {
    state.set_flag_expr(
        "ZF",
        Expr::call(&format!("eq{bits}"), &[result.clone(), Expr::new("0x0")]),
    );
    state.set_flag_expr(
        "SF",
        Expr::call(&format!("sign{bits}"), std::slice::from_ref(&result)),
    );

    match op {
        "add" => {
            state.set_flag_expr(
                "CF",
                Expr::call(&format!("carry_add{bits}"), &[lhs.clone(), rhs.clone()]),
            );
            state.set_flag_expr(
                "OF",
                Expr::call(&format!("overflow_add{bits}"), &[lhs, rhs]),
            );
        }
        "sub" | "cmp" => {
            state.set_flag_expr(
                "CF",
                Expr::call(&format!("borrow_sub{bits}"), &[lhs.clone(), rhs.clone()]),
            );
            state.set_flag_expr(
                "OF",
                Expr::call(&format!("overflow_sub{bits}"), &[lhs, rhs]),
            );
        }
        "and" | "or" | "xor" => {
            state.set_flag_expr("CF", Expr::new("false"));
            state.set_flag_expr("OF", Expr::new("false"));
        }
        "shl" | "shr" | "sar" => {
            state.set_flag_expr(
                "CF",
                Expr::call(&format!("{op}_carry{bits}"), &[lhs.clone(), rhs.clone()]),
            );
            state.set_flag_expr(
                "OF",
                Expr::call(&format!("{op}_overflow{bits}"), &[lhs, rhs]),
            );
        }
        _ => {}
    }
}

fn update_flags_for_inc_dec(
    state: &mut MachineState,
    op: &str,
    bits: u16,
    lhs: Expr,
    rhs: Expr,
    result: Expr,
) {
    state.set_flag_expr(
        "ZF",
        Expr::call(&format!("eq{bits}"), &[result.clone(), Expr::new("0x0")]),
    );
    state.set_flag_expr("SF", Expr::call(&format!("sign{bits}"), &[result]));
    let flag_op = if op == "inc" { "add" } else { "sub" };
    state.set_flag_expr(
        "OF",
        Expr::call(&format!("overflow_{flag_op}{bits}"), &[lhs, rhs]),
    );
}

fn operands_same_register(lhs: &str, rhs: &str) -> bool {
    clean_operand(lhs) == clean_operand(rhs) && is_register(&clean_operand(lhs))
}

fn eq_formula_for_operands(
    lhs: &str,
    rhs: Option<&str>,
    _state: &MachineState,
    _bits: u16,
) -> Option<IrFormula> {
    rhs.map(|right| eq_formula_for_two_operands(lhs, right))?
}

fn eq_formula_for_two_operands(lhs: &str, rhs: &str) -> Option<IrFormula> {
    Some(atomic_eq(
        operand_formula_term(lhs)?,
        operand_formula_term(rhs)?,
    ))
}

fn eq_zero_formula_for_operand(operand: &str) -> IrFormula {
    match operand_formula_term(operand) {
        Some(term) => atomic_eq(term, const_int_term(0)),
        None => atomic_predicate("x86.eq_zero", vec![string_term(operand)]),
    }
}

fn operand_formula_term(operand: &str) -> Option<IrTerm> {
    let operand = clean_operand(operand);
    if let Some(info) = reg_info(&operand) {
        return Some(IrTerm::Var {
            name: info.view.to_string(),
        });
    }
    if let Some(immediate) = parse_immediate(&operand, 64) {
        return Some(expr_to_term(&immediate));
    }
    None
}

fn simplify_view_expr(expr: &Expr) -> Expr {
    for info in REG_INFOS {
        let low = format!("low{}({})", info.bits, info.canonical);
        if expr.as_str() == low {
            return Expr::new(info.view);
        }
    }
    expr.clone()
}

fn is_return_view(view: &str) -> bool {
    matches!(view, "rax" | "eax" | "ax" | "al")
}

fn expr_to_term(expr: &Expr) -> IrTerm {
    if let Some(value) = parse_hex_i64(expr.as_str()) {
        return IrTerm::Const {
            value: json!(format!("0x{value:x}")),
            sort: primitive_sort("BitVector"),
        };
    }
    if is_simple_identifier(expr.as_str()) {
        return IrTerm::Var {
            name: expr.as_str().to_string(),
        };
    }
    IrTerm::Ctor {
        name: "x86.expr".to_string(),
        args: vec![string_term(expr.as_str())],
    }
}

fn parse_hex_i64(input: &str) -> Option<i64> {
    let hex = input.strip_prefix("0x")?;
    i64::from_str_radix(hex, 16).ok()
}

fn is_simple_identifier(input: &str) -> bool {
    input
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        && input
            .chars()
            .next()
            .map(|ch| ch.is_ascii_alphabetic() || ch == '_')
            .unwrap_or(false)
}

fn string_term(value: &str) -> IrTerm {
    IrTerm::Const {
        value: json!(value),
        sort: primitive_sort("String"),
    }
}

fn const_int_term(value: i64) -> IrTerm {
    IrTerm::Const {
        value: json!(value),
        sort: primitive_sort("Int"),
    }
}

fn primitive_sort(name: &str) -> Sort {
    Sort::Primitive {
        name: name.to_string(),
    }
}

fn atomic_eq(lhs: IrTerm, rhs: IrTerm) -> IrFormula {
    IrFormula::Atomic {
        name: "=".to_string(),
        args: vec![lhs, rhs],
    }
}

fn atomic_predicate(name: &str, args: Vec<IrTerm>) -> IrFormula {
    IrFormula::Atomic {
        name: name.to_string(),
        args,
    }
}

fn true_formula() -> IrFormula {
    IrFormula::Atomic {
        name: "true".to_string(),
        args: vec![],
    }
}

fn formula_and(mut operands: Vec<IrFormula>) -> IrFormula {
    operands.retain(|formula| !matches!(formula, IrFormula::Atomic { name, args } if name == "true" && args.is_empty()));
    match operands.len() {
        0 => true_formula(),
        1 => operands.remove(0),
        _ => IrFormula::And { operands },
    }
}

fn implies_formula(lhs: IrFormula, rhs: IrFormula) -> IrFormula {
    if matches!(lhs, IrFormula::Atomic { ref name, ref args } if name == "true" && args.is_empty())
    {
        rhs
    } else {
        IrFormula::Implies {
            operands: vec![lhs, rhs],
        }
    }
}

fn negate_formula(formula: IrFormula) -> IrFormula {
    match formula {
        IrFormula::Not { mut operands } if operands.len() == 1 => operands.remove(0),
        other => IrFormula::Not {
            operands: vec![other],
        },
    }
}

fn machine_formals() -> Vec<String> {
    GP_REGS
        .iter()
        .map(|reg| (*reg).to_string())
        .chain(["rflags".to_string(), "memory".to_string()])
        .collect()
}

fn machine_formal_sorts() -> Vec<Sort> {
    GP_REGS
        .iter()
        .map(|_| primitive_sort("Reg64"))
        .chain([primitive_sort("RFLAGS"), primitive_sort("Memory")])
        .collect()
}

#[derive(Debug, Clone, Copy)]
struct RegInfo {
    view: &'static str,
    canonical: &'static str,
    bits: u16,
}

const GP_REGS: &[&str] = &[
    "rax", "rbx", "rcx", "rdx", "rsi", "rdi", "rbp", "rsp", "r8", "r9", "r10", "r11", "r12", "r13",
    "r14", "r15",
];

const REG_INFOS: &[RegInfo] = &[
    RegInfo {
        view: "rax",
        canonical: "rax",
        bits: 64,
    },
    RegInfo {
        view: "eax",
        canonical: "rax",
        bits: 32,
    },
    RegInfo {
        view: "ax",
        canonical: "rax",
        bits: 16,
    },
    RegInfo {
        view: "al",
        canonical: "rax",
        bits: 8,
    },
    RegInfo {
        view: "rbx",
        canonical: "rbx",
        bits: 64,
    },
    RegInfo {
        view: "ebx",
        canonical: "rbx",
        bits: 32,
    },
    RegInfo {
        view: "bx",
        canonical: "rbx",
        bits: 16,
    },
    RegInfo {
        view: "bl",
        canonical: "rbx",
        bits: 8,
    },
    RegInfo {
        view: "rcx",
        canonical: "rcx",
        bits: 64,
    },
    RegInfo {
        view: "ecx",
        canonical: "rcx",
        bits: 32,
    },
    RegInfo {
        view: "cx",
        canonical: "rcx",
        bits: 16,
    },
    RegInfo {
        view: "cl",
        canonical: "rcx",
        bits: 8,
    },
    RegInfo {
        view: "rdx",
        canonical: "rdx",
        bits: 64,
    },
    RegInfo {
        view: "edx",
        canonical: "rdx",
        bits: 32,
    },
    RegInfo {
        view: "dx",
        canonical: "rdx",
        bits: 16,
    },
    RegInfo {
        view: "dl",
        canonical: "rdx",
        bits: 8,
    },
    RegInfo {
        view: "rsi",
        canonical: "rsi",
        bits: 64,
    },
    RegInfo {
        view: "esi",
        canonical: "rsi",
        bits: 32,
    },
    RegInfo {
        view: "si",
        canonical: "rsi",
        bits: 16,
    },
    RegInfo {
        view: "sil",
        canonical: "rsi",
        bits: 8,
    },
    RegInfo {
        view: "rdi",
        canonical: "rdi",
        bits: 64,
    },
    RegInfo {
        view: "edi",
        canonical: "rdi",
        bits: 32,
    },
    RegInfo {
        view: "di",
        canonical: "rdi",
        bits: 16,
    },
    RegInfo {
        view: "dil",
        canonical: "rdi",
        bits: 8,
    },
    RegInfo {
        view: "rbp",
        canonical: "rbp",
        bits: 64,
    },
    RegInfo {
        view: "ebp",
        canonical: "rbp",
        bits: 32,
    },
    RegInfo {
        view: "bp",
        canonical: "rbp",
        bits: 16,
    },
    RegInfo {
        view: "bpl",
        canonical: "rbp",
        bits: 8,
    },
    RegInfo {
        view: "rsp",
        canonical: "rsp",
        bits: 64,
    },
    RegInfo {
        view: "esp",
        canonical: "rsp",
        bits: 32,
    },
    RegInfo {
        view: "sp",
        canonical: "rsp",
        bits: 16,
    },
    RegInfo {
        view: "spl",
        canonical: "rsp",
        bits: 8,
    },
    RegInfo {
        view: "r8",
        canonical: "r8",
        bits: 64,
    },
    RegInfo {
        view: "r8d",
        canonical: "r8",
        bits: 32,
    },
    RegInfo {
        view: "r8w",
        canonical: "r8",
        bits: 16,
    },
    RegInfo {
        view: "r8b",
        canonical: "r8",
        bits: 8,
    },
    RegInfo {
        view: "r9",
        canonical: "r9",
        bits: 64,
    },
    RegInfo {
        view: "r9d",
        canonical: "r9",
        bits: 32,
    },
    RegInfo {
        view: "r9w",
        canonical: "r9",
        bits: 16,
    },
    RegInfo {
        view: "r9b",
        canonical: "r9",
        bits: 8,
    },
    RegInfo {
        view: "r10",
        canonical: "r10",
        bits: 64,
    },
    RegInfo {
        view: "r10d",
        canonical: "r10",
        bits: 32,
    },
    RegInfo {
        view: "r10w",
        canonical: "r10",
        bits: 16,
    },
    RegInfo {
        view: "r10b",
        canonical: "r10",
        bits: 8,
    },
    RegInfo {
        view: "r11",
        canonical: "r11",
        bits: 64,
    },
    RegInfo {
        view: "r11d",
        canonical: "r11",
        bits: 32,
    },
    RegInfo {
        view: "r11w",
        canonical: "r11",
        bits: 16,
    },
    RegInfo {
        view: "r11b",
        canonical: "r11",
        bits: 8,
    },
    RegInfo {
        view: "r12",
        canonical: "r12",
        bits: 64,
    },
    RegInfo {
        view: "r12d",
        canonical: "r12",
        bits: 32,
    },
    RegInfo {
        view: "r12w",
        canonical: "r12",
        bits: 16,
    },
    RegInfo {
        view: "r12b",
        canonical: "r12",
        bits: 8,
    },
    RegInfo {
        view: "r13",
        canonical: "r13",
        bits: 64,
    },
    RegInfo {
        view: "r13d",
        canonical: "r13",
        bits: 32,
    },
    RegInfo {
        view: "r13w",
        canonical: "r13",
        bits: 16,
    },
    RegInfo {
        view: "r13b",
        canonical: "r13",
        bits: 8,
    },
    RegInfo {
        view: "r14",
        canonical: "r14",
        bits: 64,
    },
    RegInfo {
        view: "r14d",
        canonical: "r14",
        bits: 32,
    },
    RegInfo {
        view: "r14w",
        canonical: "r14",
        bits: 16,
    },
    RegInfo {
        view: "r14b",
        canonical: "r14",
        bits: 8,
    },
    RegInfo {
        view: "r15",
        canonical: "r15",
        bits: 64,
    },
    RegInfo {
        view: "r15d",
        canonical: "r15",
        bits: 32,
    },
    RegInfo {
        view: "r15w",
        canonical: "r15",
        bits: 16,
    },
    RegInfo {
        view: "r15b",
        canonical: "r15",
        bits: 8,
    },
];

fn reg_info(register: &str) -> Option<RegInfo> {
    let register = register.trim().to_ascii_lowercase();
    REG_INFOS.iter().copied().find(|info| info.view == register)
}

fn canonical_register(register: &str) -> Option<&'static str> {
    reg_info(register).map(|info| info.canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_branch_target_accepts_gnu_objdump_hex_without_prefix() {
        assert_eq!(parse_branch_target("a <foo+0xa>"), Some(0xa));
    }

    #[test]
    fn lift_function_accepts_gnu_objdump_output_for_foo() {
        let streams = parse_objdump(
            include_str!("../tests/fixtures/foo.gnu-objdump.txt"),
            Path::new("tests/fixtures/foo.s"),
        )
        .expect("GNU objdump output parses");

        assert_eq!(streams.len(), 1);
        let contract = lift_function(&streams[0]).expect("GNU objdump output lifts");

        assert_eq!(contract.fn_name, "foo");
        assert!(contract.effects.effects.is_empty());
    }
}
