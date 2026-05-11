use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use provekit_ir_types::{IrFormula, IrTerm, Sort};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};
use walkdir::WalkDir;

pub const SURFACE: &str = "evm-bytecode";
pub const ASSEMBLY_SURFACE: &str = "evm-assembly";
pub const HEX_SURFACE: &str = "evm-hex";

#[derive(Debug, thiserror::Error)]
pub enum LiftError {
    #[error("read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("parse {path}: {message}")]
    Parse { path: String, message: String },
    #[error(
        "evm-bytecode lift: path must be relative to workspace_root, got absolute path {path}"
    )]
    AbsolutePath { path: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opcode {
    Stop,
    Add,
    Mul,
    Sub,
    Div,
    Mod,
    Lt,
    Gt,
    Eq,
    IsZero,
    And,
    Or,
    Xor,
    Not,
    Pop,
    Push(u8),
    Dup(u8),
    Swap(u8),
    JumpDest,
    Return,
    Unsupported { mnemonic: String, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    pub pc: usize,
    pub line: usize,
    pub opcode: Opcode,
    pub immediate: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct EvmUnit {
    pub path: String,
    pub instructions: Vec<Instruction>,
    pub diagnostics: Vec<String>,
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
}

enum Step {
    Continue,
    Stop(IrTerm),
    EmptyStackAtStop,
    UnsupportedReturnShape,
}

enum InstructionError {
    StackUnderflow(String),
    UnsupportedOpcode(String),
}

pub fn run_cli() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--rpc") {
        run_rpc();
        return;
    }

    eprintln!("usage: provekit-lift-evm-bytecode --rpc");
    std::process::exit(1);
}

pub fn parse_evm_text(path: &str, source: &str) -> Result<EvmUnit, LiftError> {
    if is_hex_source_path(path) {
        parse_hex_text(path, source)
    } else if is_assembly_source_path(path) {
        parse_assembly_text(path, source)
    } else if looks_like_hex_bytecode(source) {
        parse_hex_text(path, source)
    } else {
        parse_assembly_text(path, source)
    }
}

pub fn lift_source_text(path: &str, source: &str) -> Result<LiftResult, LiftError> {
    let unit = parse_evm_text(path, source)?;
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
        let path = resolve_path(workspace_root, source_path)?;
        for path in expand_source_path(&path)? {
            let display_path = path
                .strip_prefix(workspace_root)
                .unwrap_or(path.as_path())
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/");
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
            "name": "provekit-lift-evm-bytecode",
            "version": "0.1.0-draft",
            "protocol_version": "provekit-lift/1",
            "capabilities": {
                "authoring_surfaces": [SURFACE, ASSEMBLY_SURFACE, HEX_SURFACE],
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
    if surface != SURFACE && surface != ASSEMBLY_SURFACE && surface != HEX_SURFACE {
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

fn looks_like_hex_bytecode(source: &str) -> bool {
    let compact = compact_source_bytes(source);
    let Some(hex) = compact.strip_prefix("0x") else {
        return false;
    };
    !hex.is_empty() && hex.len() % 2 == 0 && hex.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn is_hex_source_path(path: &str) -> bool {
    source_extension(path)
        .map(|extension| matches!(extension.as_str(), "evmhex" | "hex"))
        .unwrap_or(false)
}

fn is_assembly_source_path(path: &str) -> bool {
    source_extension(path)
        .map(|extension| matches!(extension.as_str(), "evmasm" | "asm"))
        .unwrap_or(false)
}

fn source_extension(path: &str) -> Option<String> {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
}

fn compact_source_bytes(source: &str) -> String {
    source
        .lines()
        .map(strip_comment)
        .flat_map(str::chars)
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase()
}

fn parse_hex_text(path: &str, source: &str) -> Result<EvmUnit, LiftError> {
    let compact = compact_source_bytes(source);
    let hex = compact.strip_prefix("0x").unwrap_or(&compact);
    if hex.len() % 2 != 0 {
        return Err(parse_error(
            path,
            1,
            "hex bytecode must contain whole bytes",
        ));
    }
    if !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(parse_error(path, 1, "hex bytecode contains non-hex digits"));
    }
    let mut bytes = Vec::new();
    for idx in (0..hex.len()).step_by(2) {
        let byte = u8::from_str_radix(&hex[idx..idx + 2], 16)
            .map_err(|err| parse_error(path, 1, format!("decode hex byte: {err}")))?;
        bytes.push(byte);
    }
    let instructions = decode_bytes(path, &bytes)?;
    Ok(EvmUnit {
        path: path.to_string(),
        instructions,
        diagnostics: Vec::new(),
    })
}

fn parse_assembly_text(path: &str, source: &str) -> Result<EvmUnit, LiftError> {
    let mut instructions = Vec::new();
    let mut pc = 0;
    for (line_idx, raw) in source.lines().enumerate() {
        let line_no = line_idx + 1;
        let trimmed = strip_comment(raw).trim();
        if trimmed.is_empty() {
            continue;
        }
        let instruction = parse_assembly_instruction(path, line_no, trimmed, pc)?;
        pc += encoded_instruction_len(&instruction);
        instructions.push(instruction);
    }
    Ok(EvmUnit {
        path: path.to_string(),
        instructions,
        diagnostics: Vec::new(),
    })
}

fn strip_comment(line: &str) -> &str {
    let mut end = line.len();
    for marker in ["//", "#", ";"] {
        if let Some(idx) = line.find(marker) {
            end = end.min(idx);
        }
    }
    &line[..end]
}

fn decode_bytes(path: &str, bytes: &[u8]) -> Result<Vec<Instruction>, LiftError> {
    let mut instructions = Vec::new();
    let mut pc = 0;
    while pc < bytes.len() {
        let code = bytes[pc];
        if (0x60..=0x7f).contains(&code) {
            let width = code - 0x5f;
            let start = pc + 1;
            let end = start + usize::from(width);
            if end > bytes.len() {
                return Err(parse_error(
                    path,
                    1,
                    format!("PUSH{width} at byte {pc} is missing immediate bytes"),
                ));
            }
            let immediate = hex_bytes(&bytes[start..end]);
            instructions.push(Instruction {
                pc,
                line: 1,
                opcode: Opcode::Push(width),
                immediate: Some(immediate.clone()),
                text: format!("PUSH{width} {immediate}"),
            });
            pc = end;
            continue;
        }

        let (opcode, mnemonic) = opcode_from_byte(code);
        instructions.push(Instruction {
            pc,
            line: 1,
            opcode,
            immediate: None,
            text: mnemonic,
        });
        pc += 1;
    }
    Ok(instructions)
}

fn parse_assembly_instruction(
    path: &str,
    line: usize,
    text: &str,
    pc: usize,
) -> Result<Instruction, LiftError> {
    let mut parts = text.split_whitespace();
    let mnemonic = parts
        .next()
        .ok_or_else(|| parse_error(path, line, "empty instruction"))?
        .to_ascii_uppercase();
    let operands: Vec<&str> = parts.collect();
    let opcode = opcode_from_mnemonic(&mnemonic);
    let immediate = match opcode {
        Opcode::Push(width) => {
            if width == 0 {
                if !operands.is_empty() {
                    return Err(parse_error(path, line, "PUSH0 takes no immediate operand"));
                }
                Some("0x00".to_string())
            } else {
                if operands.len() != 1 {
                    return Err(parse_error(
                        path,
                        line,
                        format!("{mnemonic} takes exactly one immediate operand"),
                    ));
                }
                Some(normalize_push_immediate(path, line, operands[0], width)?)
            }
        }
        _ => {
            if !operands.is_empty() {
                return Err(parse_error(
                    path,
                    line,
                    format!("{mnemonic} does not take operands in this lifter slice"),
                ));
            }
            None
        }
    };

    Ok(Instruction {
        pc,
        line,
        opcode,
        immediate,
        text: text.to_string(),
    })
}

fn encoded_instruction_len(instruction: &Instruction) -> usize {
    match instruction.opcode {
        Opcode::Push(width) => 1 + usize::from(width),
        _ => 1,
    }
}

fn opcode_from_byte(code: u8) -> (Opcode, String) {
    let mnemonic = match code {
        0x00 => "STOP",
        0x01 => "ADD",
        0x02 => "MUL",
        0x03 => "SUB",
        0x04 => "DIV",
        0x06 => "MOD",
        0x10 => "LT",
        0x11 => "GT",
        0x14 => "EQ",
        0x15 => "ISZERO",
        0x16 => "AND",
        0x17 => "OR",
        0x18 => "XOR",
        0x19 => "NOT",
        0x50 => "POP",
        0x5b => "JUMPDEST",
        0x5f => "PUSH0",
        0xf3 => "RETURN",
        other if (0x80..=0x8f).contains(&other) => {
            return (Opcode::Dup(other - 0x7f), format!("DUP{}", other - 0x7f))
        }
        other if (0x90..=0x9f).contains(&other) => {
            return (Opcode::Swap(other - 0x8f), format!("SWAP{}", other - 0x8f))
        }
        other => {
            let mnemonic = fallback_mnemonic(other);
            return (
                Opcode::Unsupported {
                    mnemonic: mnemonic.clone(),
                    reason: unsupported_reason(&mnemonic),
                },
                mnemonic,
            );
        }
    };
    (opcode_from_mnemonic(mnemonic), mnemonic.to_string())
}

fn opcode_from_mnemonic(mnemonic: &str) -> Opcode {
    match mnemonic {
        "STOP" => Opcode::Stop,
        "ADD" => Opcode::Add,
        "MUL" => Opcode::Mul,
        "SUB" => Opcode::Sub,
        "DIV" => Opcode::Div,
        "MOD" => Opcode::Mod,
        "LT" => Opcode::Lt,
        "GT" => Opcode::Gt,
        "EQ" => Opcode::Eq,
        "ISZERO" => Opcode::IsZero,
        "AND" => Opcode::And,
        "OR" => Opcode::Or,
        "XOR" => Opcode::Xor,
        "NOT" => Opcode::Not,
        "POP" => Opcode::Pop,
        "JUMPDEST" => Opcode::JumpDest,
        "RETURN" => Opcode::Return,
        "PUSH0" => Opcode::Push(0),
        other if other.starts_with("PUSH") => parse_numbered_opcode(other, "PUSH", 1, 32)
            .map(Opcode::Push)
            .unwrap_or_else(|| unsupported_mnemonic(other)),
        other if other.starts_with("DUP") => parse_numbered_opcode(other, "DUP", 1, 16)
            .map(Opcode::Dup)
            .unwrap_or_else(|| unsupported_mnemonic(other)),
        other if other.starts_with("SWAP") => parse_numbered_opcode(other, "SWAP", 1, 16)
            .map(Opcode::Swap)
            .unwrap_or_else(|| unsupported_mnemonic(other)),
        other => unsupported_mnemonic(other),
    }
}

fn parse_numbered_opcode(mnemonic: &str, prefix: &str, min: u8, max: u8) -> Option<u8> {
    let value = mnemonic.strip_prefix(prefix)?.parse::<u8>().ok()?;
    (min..=max).contains(&value).then_some(value)
}

fn unsupported_mnemonic(mnemonic: &str) -> Opcode {
    Opcode::Unsupported {
        mnemonic: mnemonic.to_string(),
        reason: unsupported_reason(mnemonic),
    }
}

fn fallback_mnemonic(code: u8) -> String {
    match code {
        0x20 => "SHA3".to_string(),
        0x30 => "ADDRESS".to_string(),
        0x31 => "BALANCE".to_string(),
        0x33 => "CALLER".to_string(),
        0x34 => "CALLVALUE".to_string(),
        0x35 => "CALLDATALOAD".to_string(),
        0x36 => "CALLDATASIZE".to_string(),
        0x37 => "CALLDATACOPY".to_string(),
        0x39 => "CODECOPY".to_string(),
        0x51 => "MLOAD".to_string(),
        0x52 => "MSTORE".to_string(),
        0x53 => "MSTORE8".to_string(),
        0x54 => "SLOAD".to_string(),
        0x55 => "SSTORE".to_string(),
        0x56 => "JUMP".to_string(),
        0x57 => "JUMPI".to_string(),
        0xf0 => "CREATE".to_string(),
        0xf1 => "CALL".to_string(),
        0xf2 => "CALLCODE".to_string(),
        0xf4 => "DELEGATECALL".to_string(),
        0xf5 => "CREATE2".to_string(),
        0xfa => "STATICCALL".to_string(),
        0xfd => "REVERT".to_string(),
        0xfe => "INVALID".to_string(),
        0xff => "SELFDESTRUCT".to_string(),
        other if (0xa0..=0xa4).contains(&other) => format!("LOG{}", other - 0xa0),
        other => format!("UNKNOWN_0x{other:02x}"),
    }
}

fn unsupported_reason(mnemonic: &str) -> String {
    match mnemonic {
        "JUMP" | "JUMPI" => {
            format!("{mnemonic} requires control-flow graph recovery; dynamic EVM jumps are refused in this slice")
        }
        "SLOAD" => "SLOAD reads contract storage; storage effects are not modeled in this slice"
            .to_string(),
        "SSTORE" => "SSTORE writes contract storage; storage effects are not modeled in this slice"
            .to_string(),
        "CALL" | "CALLCODE" | "DELEGATECALL" | "STATICCALL" => {
            format!("{mnemonic} crosses contract boundaries; call effects are not modeled in this slice")
        }
        "CREATE" | "CREATE2" => {
            format!("{mnemonic} creates contracts; creation effects are not modeled in this slice")
        }
        "REVERT" | "INVALID" | "SELFDESTRUCT" => {
            format!("{mnemonic} has exceptional control effects; it is refused in this slice")
        }
        op if op.starts_with("LOG") => {
            format!("{op} emits logs; event effects are not modeled in this slice")
        }
        "MLOAD" | "MSTORE" | "MSTORE8" => {
            format!("{mnemonic} touches EVM memory; memory effects are not modeled in this slice")
        }
        _ => format!("{mnemonic} is not in the evm-bytecode core straight-line stack subset"),
    }
}

fn normalize_push_immediate(
    path: &str,
    line: usize,
    operand: &str,
    width: u8,
) -> Result<String, LiftError> {
    let mut hex = if let Some(hex) = operand
        .strip_prefix("0x")
        .or_else(|| operand.strip_prefix("0X"))
    {
        hex.to_string()
    } else {
        let value = operand.parse::<u128>().map_err(|err| {
            parse_error(
                path,
                line,
                format!("immediate must be hex or decimal integer: {err}"),
            )
        })?;
        format!("{value:x}")
    };
    if hex.len() % 2 != 0 {
        hex.insert(0, '0');
    }
    if !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(parse_error(path, line, "immediate contains non-hex digits"));
    }
    let expected_len = usize::from(width) * 2;
    if hex.len() > expected_len {
        return Err(parse_error(
            path,
            line,
            format!("PUSH{width} immediate is wider than {width} bytes"),
        ));
    }
    let padded = format!(
        "{:0>width$}",
        hex.to_ascii_lowercase(),
        width = expected_len
    );
    Ok(format!("0x{padded}"))
}

fn lift_unit(unit: &EvmUnit) -> LiftResult {
    let function = function_name_from_path(&unit.path);
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

    match lift_instructions(&function, unit) {
        Ok(contract) => result.declarations.push(contract),
        Err(refusal) => result.refusals.push(refusal),
    }

    result
}

fn lift_instructions(function: &str, unit: &EvmUnit) -> Result<Json, Refusal> {
    let mut state = SymbolicState { stack: Vec::new() };
    let mut terminal: Option<IrTerm> = None;

    for instruction in &unit.instructions {
        match apply_instruction(&mut state, instruction) {
            Ok(Step::Continue) => {}
            Ok(Step::Stop(value)) => {
                terminal = Some(value);
                break;
            }
            Ok(Step::EmptyStackAtStop) => {
                return Err(Refusal {
                    kind: "stop-with-no-return-value".to_string(),
                    function: Some(function.to_string()),
                    line: Some(instruction.line),
                    instruction: Some(instruction.text.clone()),
                    reason: "program terminates via STOP with an empty stack; no return value"
                        .to_string(),
                });
            }
            Ok(Step::UnsupportedReturnShape) => {
                return Err(Refusal {
                    kind: "unsupported-return-shape".to_string(),
                    function: Some(function.to_string()),
                    line: Some(instruction.line),
                    instruction: Some(instruction.text.clone()),
                    reason: "RETURN yields a memory slice; a bytes-return sort + memory-read effect are not yet modeled in this lifter slice".to_string(),
                });
            }
            Err(error) => {
                let (kind, reason) = match error {
                    InstructionError::StackUnderflow(reason) => ("stack-underflow", reason),
                    InstructionError::UnsupportedOpcode(reason) => ("unsupported-opcode", reason),
                };
                return Err(Refusal {
                    kind: kind.to_string(),
                    function: Some(function.to_string()),
                    line: Some(instruction.line),
                    instruction: Some(instruction.text.clone()),
                    reason,
                });
            }
        }
    }

    let post = match terminal {
        Some(value) => eq_formula(var("return_value"), value),
        None => {
            return Err(Refusal {
                kind: "unterminated-bytecode".to_string(),
                function: Some(function.to_string()),
                line: unit.instructions.last().map(|instruction| instruction.line),
                instruction: unit
                    .instructions
                    .last()
                    .map(|instruction| instruction.text.clone()),
                reason: "instruction stream ended without STOP or RETURN".to_string(),
            })
        }
    };

    Ok(json!({
        "schemaVersion": "1",
        "kind": "function-contract",
        "name": function,
        "fnName": function,
        "outBinding": "return_value",
        "formals": [],
        "formalSorts": [],
        "returnSort": evm_word_sort(),
        "pre": true_formula(),
        "post": post,
        "bodyCid": null,
        "effects": [],
        "locus": {
            "file": contract_locus_file(&unit.path),
            "line": 1,
            "col": 1
        },
        "autoMintedMementos": []
    }))
}

fn apply_instruction(
    state: &mut SymbolicState,
    instruction: &Instruction,
) -> Result<Step, InstructionError> {
    match &instruction.opcode {
        Opcode::Stop => Ok(state
            .stack
            .last()
            .cloned()
            .map(Step::Stop)
            .unwrap_or(Step::EmptyStackAtStop)),
        Opcode::Push(_) => {
            let immediate = instruction
                .immediate
                .clone()
                .unwrap_or_else(|| "0x00".to_string());
            state.stack.push(word_const(immediate));
            Ok(Step::Continue)
        }
        Opcode::Add => binary_word_op(state, "evm:add"),
        Opcode::Mul => binary_word_op(state, "evm:mul"),
        Opcode::Sub => binary_word_op(state, "evm:sub"),
        Opcode::Div => binary_word_op(state, "evm:div"),
        Opcode::Mod => binary_word_op(state, "evm:mod"),
        Opcode::Lt => binary_word_op(state, "evm:lt"),
        Opcode::Gt => binary_word_op(state, "evm:gt"),
        Opcode::Eq => binary_word_op(state, "evm:eq"),
        Opcode::And => binary_word_op(state, "evm:and"),
        Opcode::Or => binary_word_op(state, "evm:or"),
        Opcode::Xor => binary_word_op(state, "evm:xor"),
        Opcode::IsZero => unary_word_op(state, "evm:iszero"),
        Opcode::Not => unary_word_op(state, "evm:not"),
        Opcode::Pop => {
            state.pop()?;
            Ok(Step::Continue)
        }
        Opcode::Dup(depth) => {
            let depth = usize::from(*depth);
            if state.stack.len() < depth {
                return Err(InstructionError::StackUnderflow(format!(
                    "DUP{depth} requires {depth} stack values"
                )));
            }
            let value = state.stack[state.stack.len() - depth].clone();
            state.stack.push(value);
            Ok(Step::Continue)
        }
        Opcode::Swap(depth) => {
            let depth = usize::from(*depth);
            if state.stack.len() <= depth {
                return Err(InstructionError::StackUnderflow(format!(
                    "SWAP{depth} requires {} stack values",
                    depth + 1
                )));
            }
            let top = state.stack.len() - 1;
            let other = top - depth;
            state.stack.swap(top, other);
            Ok(Step::Continue)
        }
        Opcode::JumpDest => Ok(Step::Continue),
        Opcode::Return => Ok(Step::UnsupportedReturnShape),
        Opcode::Unsupported { reason, .. } => {
            Err(InstructionError::UnsupportedOpcode(reason.clone()))
        }
    }
}

impl SymbolicState {
    fn pop(&mut self) -> Result<IrTerm, InstructionError> {
        self.stack
            .pop()
            .ok_or_else(|| InstructionError::StackUnderflow("operand stack underflow".to_string()))
    }
}

fn unary_word_op(state: &mut SymbolicState, name: &str) -> Result<Step, InstructionError> {
    let value = state.pop()?;
    state.stack.push(ctor(name, vec![value]));
    Ok(Step::Continue)
}

fn binary_word_op(state: &mut SymbolicState, name: &str) -> Result<Step, InstructionError> {
    let first_popped = state.pop()?;
    let second_popped = state.pop()?;
    state
        .stack
        .push(ctor(name, vec![first_popped, second_popped]));
    Ok(Step::Continue)
}

fn function_name_from_path(path: &str) -> String {
    let path = Path::new(path);
    let mut parts = path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .filter(|part| *part != "." && *part != "/")
        .map(|part| {
            let without_extension = if part
                == path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("")
            {
                Path::new(part)
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or(part)
            } else {
                part
            };
            sanitize_name_part(without_extension)
        })
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        parts.push("evm_program".to_string());
    }
    let mut name = parts.join("__");
    if name.is_empty() {
        name = "evm_program".to_string();
    }
    name
}

fn contract_locus_file(path: &str) -> String {
    match source_extension(path).as_deref() {
        Some("evmasm" | "evmhex") => {
            let mut path = PathBuf::from(path);
            path.set_extension("evm");
            path.to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/")
        }
        _ => path.to_string(),
    }
}

fn sanitize_name_part(part: &str) -> String {
    part.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn resolve_path(workspace_root: &Path, source_path: &str) -> Result<PathBuf, LiftError> {
    let path = PathBuf::from(source_path);
    if path.is_absolute() {
        return Err(LiftError::AbsolutePath {
            path: source_path.to_string(),
        });
    }
    Ok(workspace_root.join(path))
}

fn expand_source_path(path: &Path) -> Result<Vec<PathBuf>, LiftError> {
    if path.is_file() {
        return Ok(is_evm_source(path)
            .then(|| path.to_path_buf())
            .into_iter()
            .collect());
    }
    if !path.is_dir() {
        return Ok(Vec::new());
    }

    let mut paths = Vec::new();
    for entry in WalkDir::new(path).follow_links(true) {
        let entry = entry.map_err(|err| LiftError::Parse {
            path: path.display().to_string(),
            message: err.to_string(),
        })?;
        let entry_path = entry.path();
        if entry.file_type().is_file() && is_evm_source(entry_path) {
            paths.push(entry_path.to_path_buf());
        }
    }
    paths.sort();
    Ok(paths)
}

fn is_evm_source(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "evm" | "evmhex" | "evmasm"
            )
        })
        .unwrap_or(false)
}

fn hex_bytes(bytes: &[u8]) -> String {
    let body = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("0x{body}")
}

fn word_const(value: String) -> IrTerm {
    IrTerm::Const {
        value: Json::String(value),
        sort: evm_word_sort(),
    }
}

fn var(name: impl Into<String>) -> IrTerm {
    IrTerm::Var { name: name.into() }
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

fn true_formula() -> IrFormula {
    IrFormula::Atomic {
        name: "true".to_string(),
        args: Vec::new(),
    }
}

fn evm_word_sort() -> Sort {
    Sort::Primitive {
        name: "EvmWord".to_string(),
    }
}

fn parse_error(path: &str, line: usize, message: impl Into<String>) -> LiftError {
    LiftError::Parse {
        path: path.to_string(),
        message: format!("line {line}: {}", message.into()),
    }
}
