// SPDX-License-Identifier: Apache-2.0

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

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
                    "protocol_version": "pep/1.7.0",
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
    let mut ir = Vec::new();
    ir.extend(lift_checked_add_software_contract(project)?);
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

fn lift_checked_add_software_contract(project: &Path) -> Result<Vec<Value>, String> {
    let source = read(project, "artifacts/software/checked_add_u8.c")?;
    Ok(vec![lift_checked_add_software_contract_from_source(
        &source,
    )?])
}

fn lift_checked_add_software_contract_from_source(source: &str) -> Result<Value, String> {
    const CLAIM: &str = "checked_add_u8.postcondition";

    require_claim(
        source,
        "provekit:contract checked_add_u8.postcondition",
        "software contract marker",
        CLAIM,
    )?;
    require_claim(
        source,
        "checked_add_u8_result checked_add_u8(uint8_t a, uint8_t b)",
        "checked_add_u8 entrypoint signature",
        CLAIM,
    )?;
    require_claim(
        source,
        "uint16_t wide = (uint16_t)a + (uint16_t)b;",
        "wide unsigned addition",
        CLAIM,
    )?;
    require_claim(source, "if (wide >= 256)", "overflow guard", CLAIM)?;
    require_claim(
        source,
        ".overflow = true",
        "overflow result for guarded branch",
        CLAIM,
    )?;
    require_claim(
        source,
        ".overflow = false",
        "non-overflow result for fallthrough branch",
        CLAIM,
    )?;
    require_claim(
        source,
        ".value = (uint8_t)wide",
        "truncated result value for non-overflow branch",
        CLAIM,
    )?;

    Ok(json!({
        "kind": "contract",
        "name": CLAIM,
        "authority": "bridgeworks.software",
        "outBinding": "out",
        "post": {
            "kind": "atomic",
            "name": CLAIM,
            "args": [
                {"kind": "var", "name": "a"},
                {"kind": "var", "name": "b"},
                {"kind": "var", "name": "out"}
            ]
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridgeworks_lifts_checked_add_contract_without_generic_c_surface() {
        let source = r#"
#include <stdbool.h>
#include <stdint.h>
typedef struct { bool overflow; uint8_t value; } checked_add_u8_result;
/* provekit:contract checked_add_u8.postcondition */
checked_add_u8_result checked_add_u8(uint8_t a, uint8_t b) {
    uint16_t wide = (uint16_t)a + (uint16_t)b;
    if (wide >= 256) {
        return (checked_add_u8_result){ .overflow = true, .value = 0 };
    }
    return (checked_add_u8_result){ .overflow = false, .value = (uint8_t)wide };
}
"#;

        let contract = lift_checked_add_software_contract_from_source(source)
            .expect("valid checked-add source should lift");

        assert_eq!(contract["kind"], "contract");
        assert_eq!(contract["name"], "checked_add_u8.postcondition");
        assert_eq!(contract["outBinding"], "out");
    }

    #[test]
    fn bridgeworks_rejects_counterfeit_checked_add_contract() {
        let source = r#"
#include <stdbool.h>
#include <stdint.h>
typedef struct { bool overflow; uint8_t value; } checked_add_u8_result;
/* provekit:contract checked_add_u8.postcondition */
checked_add_u8_result checked_add_u8(uint8_t a, uint8_t b) {
    uint16_t wide = (uint16_t)a + (uint16_t)b;
    return (checked_add_u8_result){ .overflow = false, .value = (uint8_t)wide };
}
"#;

        let error = lift_checked_add_software_contract_from_source(source)
            .expect_err("counterfeit checked-add source should be refused");

        assert!(error.contains("checked_add_u8.postcondition"));
        assert!(error.contains("overflow guard"));
    }
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
