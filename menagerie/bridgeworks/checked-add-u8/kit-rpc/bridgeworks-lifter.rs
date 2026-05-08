// SPDX-License-Identifier: Apache-2.0

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const CONTRACTS: &[&str] = &[
    "checked_add_u8.postcondition",
    "compiler.lowering.preserves_checked_add",
    "isa.add8.carry_semantics",
    "rtl.alu.refines_add8",
    "gates.full_adder.equations",
    "cells.boolean_gates.valid_in_envelope",
    "device_physics.mosfet_switch.valid",
    "experiment.material_parameters.within_tolerance",
];

const IMPLICATIONS: &[(&str, &str, &str)] = &[
    (
        "experiment-supports-device-physics",
        "experiment.material_parameters.within_tolerance",
        "device_physics.mosfet_switch.valid",
    ),
    (
        "device-physics-supports-cells",
        "device_physics.mosfet_switch.valid",
        "cells.boolean_gates.valid_in_envelope",
    ),
    (
        "cells-support-gates",
        "cells.boolean_gates.valid_in_envelope",
        "gates.full_adder.equations",
    ),
    (
        "gates-support-rtl",
        "gates.full_adder.equations",
        "rtl.alu.refines_add8",
    ),
    (
        "rtl-supports-isa",
        "rtl.alu.refines_add8",
        "isa.add8.carry_semantics",
    ),
    (
        "isa-supports-compiler-lowering",
        "isa.add8.carry_semantics",
        "compiler.lowering.preserves_checked_add",
    ),
    (
        "compiler-supports-software-postcondition",
        "compiler.lowering.preserves_checked_add",
        "checked_add_u8.postcondition",
    ),
];

const CONTRACT_AUTHORITIES: &[(&str, &str)] = &[
    ("checked_add_u8.postcondition", "bridgeworks.software"),
    (
        "compiler.lowering.preserves_checked_add",
        "bridgeworks.compiler",
    ),
    ("isa.add8.carry_semantics", "bridgeworks.isa"),
    ("rtl.alu.refines_add8", "bridgeworks.rtl"),
    ("gates.full_adder.equations", "bridgeworks.gates"),
    ("cells.boolean_gates.valid_in_envelope", "bridgeworks.cells"),
    ("device_physics.mosfet_switch.valid", "bridgeworks.physics"),
    (
        "experiment.material_parameters.within_tolerance",
        "bridgeworks.experiment",
    ),
];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--self-test") {
        match lift_project(&std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))) {
            Ok(result) => {
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
                std::process::exit(0);
            }
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    }
    if !args.iter().any(|arg| arg == "--rpc") {
        eprintln!("Usage: bridgeworks-checked-add-lifter --rpc | --self-test");
        std::process::exit(1);
    }
    run_rpc();
}

fn run_rpc() {
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(error) => {
                println!(
                    "{}",
                    json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":error.to_string()}})
                );
                continue;
            }
        };
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        match method {
            "initialize" => respond(
                id,
                json!({
                    "name": "bridgeworks-checked-add",
                    "version": "0.1.0",
                    "protocol_version": "provekit-lift/1",
                    "capabilities": {
                        "authoring_surfaces": ["bridgeworks-checked-add"],
                        "ir_version": "v1.1.0",
                        "emits_signed_mementos": false
                    }
                }),
            ),
            "lift" => {
                let workspace = request
                    .pointer("/params/workspace_root")
                    .and_then(Value::as_str)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("."));
                let identify_only = request
                    .pointer("/params/options/identifyOnly")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                    || request
                        .pointer("/params/options/identify_only")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                    || request
                        .pointer("/params/options/layer")
                        .and_then(Value::as_str)
                        == Some("identify-only");
                let result = if identify_only {
                    identify_project(&workspace)
                } else {
                    lift_project(&workspace)
                };
                match result {
                    Ok(result) => respond(id, result),
                    Err(error) => respond_error(id, 1005, &error),
                }
            }
            "shutdown" => {
                respond(id, Value::Null);
                break;
            }
            _ => respond_error(id, -32601, "unknown method"),
        }
    }
}

fn identify_project(project: &Path) -> Result<Value, String> {
    let mut identities = Vec::new();
    identities.push(identity(
        "experiment",
        "experiment.material_parameters.within_tolerance",
        "artifacts/experiment/bandgap-measurements.csv",
        find_line(
            project,
            "artifacts/experiment/bandgap-measurements.csv",
            "bridgeworks:claim experiment.material_parameters.within_tolerance",
        )?,
    ));
    identities.push(identity(
        "device-physics",
        "device_physics.mosfet_switch.valid",
        "artifacts/device-physics/mosfet-switch-paper.md",
        find_line(
            project,
            "artifacts/device-physics/mosfet-switch-paper.md",
            "bridgeworks:claim device_physics.mosfet_switch.valid",
        )?,
    ));
    identities.push(identity(
        "cells",
        "cells.boolean_gates.valid_in_envelope",
        "artifacts/cells/cells.sp",
        find_line(
            project,
            "artifacts/cells/cells.sp",
            "bridgeworks:claim cells.boolean_gates.valid_in_envelope",
        )?,
    ));
    identities.push(identity(
        "gates",
        "gates.full_adder.equations",
        "artifacts/gates/full_adder.blif",
        find_line(
            project,
            "artifacts/gates/full_adder.blif",
            "bridgeworks:claim gates.full_adder.equations",
        )?,
    ));
    identities.push(identity(
        "rtl",
        "rtl.alu.refines_add8",
        "artifacts/rtl/alu.v",
        find_line(
            project,
            "artifacts/rtl/alu.v",
            "bridgeworks:claim rtl.alu.refines_add8",
        )?,
    ));
    identities.push(identity(
        "isa",
        "isa.add8.carry_semantics",
        "artifacts/isa/toy8.isa",
        find_line(
            project,
            "artifacts/isa/toy8.isa",
            "claim_id: isa.add8.carry_semantics",
        )?,
    ));
    identities.push(identity(
        "compiler",
        "compiler.lowering.preserves_checked_add",
        "artifacts/compiler/lowering.trace",
        find_line(
            project,
            "artifacts/compiler/lowering.trace",
            "claim_id: compiler.lowering.preserves_checked_add",
        )?,
    ));
    identities.push(identity(
        "software",
        "checked_add_u8.postcondition",
        "artifacts/software/checked_add_u8.c",
        find_line(
            project,
            "artifacts/software/checked_add_u8.c",
            "provekit:contract checked_add_u8.postcondition",
        )?,
    ));

    Ok(json!({
        "kind": "identity-document",
        "surface": "bridgeworks-checked-add",
        "identities": identities,
        "diagnostics": []
    }))
}

fn identity(domain: &str, claim: &str, artifact: &str, marker: Value) -> Value {
    json!({
        "domain": domain,
        "claim": claim,
        "artifact": artifact,
        "marker": marker,
    })
}

fn find_line(project: &Path, rel: &str, needle: &str) -> Result<Value, String> {
    let text = read(project, rel)?;
    for (index, line) in text.lines().enumerate() {
        if line.contains(needle) {
            return Ok(json!({
                "line": index + 1,
                "text": line.trim(),
            }));
        }
    }
    Err(format!("identity marker not found in {rel}: `{needle}`"))
}

fn respond(id: Value, result: Value) {
    println!("{}", json!({"jsonrpc":"2.0","id":id,"result":result}));
    let _ = io::stdout().flush();
}

fn respond_error(id: Value, code: i64, message: &str) {
    println!(
        "{}",
        json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
    );
    let _ = io::stdout().flush();
}

fn lift_project(project: &Path) -> Result<Value, String> {
    let repo_root = find_repo_root(project)?;
    let mut ir = Vec::new();
    ir.extend(call_c_lifter(project, &repo_root)?);
    attach_authority(
        &mut ir,
        "checked_add_u8.postcondition",
        "bridgeworks.software",
    );

    validate_compiler(project)?;
    validate_isa(project)?;
    validate_rtl(project)?;
    validate_gates(project)?;
    validate_cells(project)?;
    validate_device_physics(project)?;
    validate_experiment(project)?;

    for name in CONTRACTS
        .iter()
        .copied()
        .filter(|name| *name != "checked_add_u8.postcondition")
    {
        ir.push(contract_decl(name));
    }

    Ok(json!({
        "kind": "ir-document",
        "ir": ir,
        "authorities": authority_decls(),
        "witnesses": witness_requirements(),
        "implications": IMPLICATIONS.iter().map(|(name, antecedent, consequent)| {
            json!({
                "name": name,
                "antecedent": antecedent,
                "consequent": consequent,
                "antecedentSlot": "post",
                "consequentSlot": "post",
                "authority": implication_authority_id(name),
                "prover": "bridgeworks-white-room",
                "proofWitness": format!("{antecedent} -> {consequent}")
            })
        }).collect::<Vec<_>>(),
        "diagnostics": []
    }))
}

fn witness_requirements() -> Vec<Value> {
    vec![json!({
        "id": "checked-add-u8-c-value-witness",
        "mode": "witness",
        "surface": "c",
        "attachTo": "checked_add_u8.postcondition",
        "obligation": {
            "kind": "predicate",
            "name": "checked_add_u8.postcondition",
            "proofIr": {
                "kind": "atomic",
                "name": "checked_add_u8.postcondition",
                "args": [
                    {"kind": "var", "name": "a"},
                    {"kind": "var", "name": "b"},
                    {"kind": "var", "name": "out"}
                ]
            }
        },
        "host": {
            "kit": "c",
            "contextKind": "source-artifact",
            "artifact": "artifacts/software/checked_add_u8.c",
            "entrypoint": "checked_add_u8"
        },
        "bindings": [
            {"proofVar": "a", "hostPath": "parameter:a", "typeHint": "uint8_t"},
            {"proofVar": "b", "hostPath": "parameter:b", "typeHint": "uint8_t"},
            {"proofVar": "out", "hostPath": "return", "typeHint": "checked_add_u8_result"}
        ],
        "policy": {
            "policyCid": "builtin:bridgeworks.checked-add-u8.exhaustive-u8",
            "mode": "exhaustive-u8"
        }
    })]
}

fn contract_decl(name: &str) -> Value {
    json!({
        "kind": "contract",
        "name": name,
        "authority": contract_authority_id(name),
        "outBinding": "out",
        "post": {
            "kind": "atomic",
            "name": name,
            "args": []
        }
    })
}

fn attach_authority(ir: &mut [Value], contract_name: &str, authority: &str) {
    for decl in ir {
        if decl.get("name").and_then(Value::as_str) == Some(contract_name) {
            decl["authority"] = json!(authority);
        }
    }
}

fn contract_authority_id(contract_name: &str) -> &'static str {
    CONTRACT_AUTHORITIES
        .iter()
        .find_map(|(name, authority)| (*name == contract_name).then_some(*authority))
        .unwrap_or("bridgeworks.root")
}

fn implication_authority_id(name: &str) -> String {
    format!("bridgeworks.edge.{name}")
}

fn authority_decls() -> Vec<Value> {
    let mut authorities = vec![json!({
        "id": "bridgeworks.root",
        "principal": "bridgeworks.root",
        "scopeKind": "proof",
        "scope": "bridgeworks-checked-add-u8"
    })];
    authorities.extend(CONTRACT_AUTHORITIES.iter().map(|(contract, authority)| {
        json!({
            "id": authority,
            "principal": authority,
            "scopeKind": "contract",
            "scope": contract,
            "parent": "bridgeworks.root"
        })
    }));
    authorities.extend(IMPLICATIONS.iter().map(|(name, _, _)| {
        let authority = implication_authority_id(name);
        json!({
            "id": authority,
            "principal": authority,
            "scopeKind": "implication",
            "scope": name,
            "parent": "bridgeworks.root"
        })
    }));
    authorities
}

fn call_c_lifter(project: &Path, repo_root: &Path) -> Result<Vec<Value>, String> {
    let make_status = Command::new("make")
        .arg("-C")
        .arg(repo_root.join("implementations/c/provekit-lift"))
        .arg("all")
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| format!("spawn C lifter build: {e}"))?;
    if !make_status.success() {
        return Err("C lifter build failed".into());
    }

    let c_lifter = repo_root.join("implementations/c/provekit-lift/bin/provekit-lift-c");
    let software_root = project.join("artifacts/software");
    let mut child = Command::new(&c_lifter)
        .arg("--rpc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn C lifter {}: {e}", c_lifter.display()))?;
    {
        let stdin = child.stdin.as_mut().ok_or("C lifter stdin unavailable")?;
        writeln!(
            stdin,
            "{}",
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}})
        )
        .map_err(|e| format!("write C initialize: {e}"))?;
        writeln!(
            stdin,
            "{}",
            json!({"jsonrpc":"2.0","id":2,"method":"lift","params":{"workspace_root":software_root,"surface":"c","source_paths":["."]}})
        )
        .map_err(|e| format!("write C lift: {e}"))?;
        writeln!(
            stdin,
            "{}",
            json!({"jsonrpc":"2.0","id":3,"method":"shutdown"})
        )
        .map_err(|e| format!("write C shutdown: {e}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("wait C lifter: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        return Err(format!(
            "C lifter exited {}\nstdout:\n{stdout}",
            output.status
        ));
    }
    for line in stdout.lines() {
        let response: Value = serde_json::from_str(line)
            .map_err(|e| format!("parse C lifter response: {e}: {line}"))?;
        if response.get("id").and_then(Value::as_i64) == Some(2) {
            if let Some(error) = response.pointer("/error/message").and_then(Value::as_str) {
                return Err(format!("C lifter refused software artifact: {error}"));
            }
            let ir = response
                .get("result")
                .and_then(|r| r.get("ir"))
                .and_then(Value::as_array)
                .ok_or_else(|| "C lifter lift response missing result.ir".to_string())?;
            let mut contracts = ir.clone();
            let emitted: Vec<&str> = contracts
                .iter()
                .filter(|decl| decl.get("kind").and_then(Value::as_str) == Some("contract"))
                .filter_map(|decl| decl.get("name").and_then(Value::as_str))
                .collect();
            if !emitted.contains(&"checked_add_u8.postcondition") {
                return Err(format!(
                    "bridge edge failed: compiler.lowering.preserves_checked_add -> checked_add_u8.postcondition\nneeded: checked_add_u8.postcondition\nsoftware emitted: {}",
                    format_emitted_contracts(&emitted)
                ));
            }
            return Ok(std::mem::take(&mut contracts));
        }
    }
    Err("C lifter did not return lift response id=2".into())
}

fn format_emitted_contracts(emitted: &[&str]) -> String {
    if emitted.is_empty() {
        "<none>".to_string()
    } else {
        emitted.join(", ")
    }
}

fn validate_compiler(project: &Path) -> Result<(), String> {
    const CLAIM: &str = "compiler.lowering.preserves_checked_add";
    let trace = read(project, "artifacts/compiler/lowering.trace")?;
    let asm = read(project, "artifacts/compiler/toy8.asm")?;
    require_claim(
        &trace,
        "claim_id: compiler.lowering.preserves_checked_add",
        "compiler claim marker",
        CLAIM,
    )?;
    require_claim(
        &trace,
        "overflow_branch: BR_CARRY overflow",
        "compiler overflow branch",
        CLAIM,
    )?;
    require_claim(&asm, "ADD8 r2, r0, r1", "compiler ADD8 instruction", CLAIM)?;
    require_claim(&asm, "BR_CARRY overflow", "compiler branch on carry", CLAIM)?;
    Ok(())
}

fn validate_isa(project: &Path) -> Result<(), String> {
    const CLAIM: &str = "isa.add8.carry_semantics";
    let isa = read(project, "artifacts/isa/toy8.isa")?;
    require_claim(
        &isa,
        "claim_id: isa.add8.carry_semantics",
        "ISA claim marker",
        CLAIM,
    )?;
    if isa.contains("operation: signed_add") || isa.contains("overflow_model: signed_overflow") {
        return Err(
            "isa.add8.carry_semantics refused: signed overflow is not unsigned carry".into(),
        );
    }
    require_claim(&isa, "operation: unsigned_add", "ISA unsigned add", CLAIM)?;
    require_claim(
        &isa,
        "carry_relation: carry == (lhs + rhs >= 256)",
        "ISA carry relation",
        CLAIM,
    )?;
    Ok(())
}

fn validate_rtl(project: &Path) -> Result<(), String> {
    const CLAIM: &str = "rtl.alu.refines_add8";
    let rtl = read(project, "artifacts/rtl/alu.v")?;
    require_claim(
        &rtl,
        "bridgeworks:claim rtl.alu.refines_add8",
        "RTL claim marker",
        CLAIM,
    )?;
    require_claim(
        &rtl,
        "assign carry = op_add8 ? wide[8] : 1'b0;",
        "RTL carry bit",
        CLAIM,
    )?;
    Ok(())
}

fn validate_gates(project: &Path) -> Result<(), String> {
    const CLAIM: &str = "gates.full_adder.equations";
    let gates = read(project, "artifacts/gates/full_adder.blif")?;
    require_claim(
        &gates,
        "bridgeworks:claim gates.full_adder.equations",
        "gate claim marker",
        CLAIM,
    )?;
    require_blif_table(
        &gates,
        ".names a b axb",
        &["01 1", "10 1"],
        "a XOR b truth table",
        CLAIM,
    )?;
    require_blif_table(
        &gates,
        ".names axb cin sum",
        &["01 1", "10 1"],
        "sum XOR truth table",
        CLAIM,
    )?;
    require_blif_table(
        &gates,
        ".names a b cin cout",
        &["11- 1", "1-1 1", "-11 1"],
        "majority carry truth table",
        CLAIM,
    )?;
    Ok(())
}

fn validate_cells(project: &Path) -> Result<(), String> {
    const CLAIM: &str = "cells.boolean_gates.valid_in_envelope";
    let cells = read(project, "artifacts/cells/cells.sp")?;
    require_claim(
        &cells,
        "bridgeworks:claim cells.boolean_gates.valid_in_envelope",
        "cell claim marker",
        CLAIM,
    )?;
    require_claim(
        &cells,
        "NOISE_MARGIN_HIGH_MIN=0.22",
        "high noise margin",
        CLAIM,
    )?;
    require_claim(
        &cells,
        "NOISE_MARGIN_LOW_MIN=0.22",
        "low noise margin",
        CLAIM,
    )?;
    Ok(())
}

fn validate_device_physics(project: &Path) -> Result<(), String> {
    const CLAIM: &str = "device_physics.mosfet_switch.valid";
    let paper = read(project, "artifacts/device-physics/mosfet-switch-paper.md")?;
    require_claim(
        &paper,
        "bridgeworks:claim device_physics.mosfet_switch.valid",
        "paper claim marker",
        CLAIM,
    )?;
    require_claim(
        &paper,
        "| nmos_vth_v | 0.43 | +/- 0.03 |",
        "NMOS threshold envelope",
        CLAIM,
    )?;
    require_claim(
        &paper,
        "| bandgap_ev | 1.12 | +/- 0.02 |",
        "bandgap envelope",
        CLAIM,
    )?;
    Ok(())
}

fn validate_experiment(project: &Path) -> Result<(), String> {
    const CLAIM: &str = "experiment.material_parameters.within_tolerance";
    let csv = read(project, "artifacts/experiment/bandgap-measurements.csv")?;
    let note = read(project, "artifacts/experiment/calibration-note.md")?;
    let csv_signature = calibration_signature(&csv)
        .ok_or_else(|| format!("{CLAIM} refused: missing CSV calibration signature"))?;
    let note_signature = calibration_signature(&note)
        .ok_or_else(|| format!("{CLAIM} refused: missing calibration note signature"))?;
    require_claim(
        &csv,
        "bridgeworks:claim experiment.material_parameters.within_tolerance",
        "experiment claim marker",
        CLAIM,
    )?;
    if csv_signature != note_signature {
        return Err(format!(
            "{CLAIM} refused: calibration signature mismatch between CSV and note"
        ));
    }
    let expected_suffix = format!("SHA256-{}", measurement_content_sha256_8(&csv));
    if !csv_signature.ends_with(&expected_suffix) {
        return Err(format!(
            "{CLAIM} refused: calibration signature does not bind measurement content; expected suffix `{expected_suffix}`, observed `{csv_signature}`"
        ));
    }
    for line in csv
        .lines()
        .filter(|line| !line.starts_with('#') && !line.starts_with("sample_id"))
    {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() < 7 {
            return Err(format!(
                "experiment row has {} fields, expected 7",
                fields.len()
            ));
        }
        let bandgap: f64 = parse_f64(fields[2], "bandgap_ev")?;
        let nmos: f64 = parse_f64(fields[3], "nmos_vth_v")?;
        let pmos: f64 = parse_f64(fields[4], "pmos_abs_vth_v")?;
        let oxide: f64 = parse_f64(fields[5], "oxide_thickness_nm")?;
        if !(1.10..=1.14).contains(&bandgap)
            || !(0.40..=0.46).contains(&nmos)
            || !(0.42..=0.48).contains(&pmos)
            || !(3.90..=4.30).contains(&oxide)
        {
            return Err("experiment.material_parameters.within_tolerance refused: measurement outside calibrated envelope".into());
        }
    }
    Ok(())
}

fn calibration_signature(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.split_once("bridgeworks:calibration_signature"))
        .map(|(_, signature)| {
            signature
                .trim()
                .trim_start_matches("-->")
                .trim_end_matches("-->")
                .trim()
                .to_string()
        })
}

fn measurement_content_sha256_8(csv: &str) -> String {
    let canonical = csv
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n");
    let digest = Sha256::digest(canonical.as_bytes());
    digest[..4]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn parse_f64(value: &str, field: &str) -> Result<f64, String> {
    value
        .parse::<f64>()
        .map_err(|e| format!("parse {field} value `{value}`: {e}"))
}

fn require(text: &str, needle: &str, label: &str) -> Result<(), String> {
    if text.contains(needle) {
        Ok(())
    } else {
        Err(format!("missing {label}: `{needle}`"))
    }
}

fn require_claim(text: &str, needle: &str, label: &str, claim: &str) -> Result<(), String> {
    require(text, needle, label).map_err(|error| format!("{claim} refused: {error}"))
}

fn require_blif_table(
    text: &str,
    header: &str,
    expected_rows: &[&str],
    label: &str,
    claim: &str,
) -> Result<(), String> {
    let mut lines = text.lines().map(str::trim);
    loop {
        let Some(line) = lines.next() else {
            return Err(format!("{claim} refused: missing {label}: `{header}`"));
        };
        if line != header {
            continue;
        }
        let rows: Vec<&str> = lines
            .by_ref()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .take_while(|line| !line.starts_with('.'))
            .collect();
        if rows.len() != expected_rows.len()
            || expected_rows
                .iter()
                .any(|expected| !rows.contains(expected))
        {
            return Err(format!(
                "{claim} refused: {label} expected {:?}, observed {:?}",
                expected_rows, rows
            ));
        }
        return Ok(());
    }
}

fn read(project: &Path, rel: &str) -> Result<String, String> {
    let path = project.join(rel);
    std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))
}

fn find_repo_root(start: &Path) -> Result<PathBuf, String> {
    let mut cursor = if start.is_absolute() {
        start.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| format!("current dir: {e}"))?
            .join(start)
    };
    cursor = cursor.canonicalize().unwrap_or(cursor);
    loop {
        if cursor
            .join("implementations/rust/provekit-cli/Cargo.toml")
            .exists()
        {
            return Ok(cursor);
        }
        if !cursor.pop() {
            return Err(format!(
                "could not find repo root above {}",
                start.display()
            ));
        }
    }
}
