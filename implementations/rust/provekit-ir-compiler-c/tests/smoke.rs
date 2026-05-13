// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use provekit_ir_compiler::IrCompiler;
use provekit_ir_compiler_c::{compile_c, CCompiler, DIALECT};
use serde_json::json;
use serde_json::Value as Json;

#[test]
fn smoke_compiles_foo_term_byte_for_byte() {
    let input: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/foo.term.json")).expect("fixture json");
    let expected = include_str!("fixtures/foo.expected.c");

    let c_source = compile_c(&input).expect("foo term compiles");

    assert_eq!(c_source, expected);
}

#[test]
fn lowers_core_expression_ops() {
    let input = json!({
        "kind": "c11-algebra-term",
        "source": "ops.c",
        "term": {
            "kind": "op",
            "name": "return",
            "args": [{
                "kind": "op",
                "name": "and",
                "args": [
                    {
                        "kind": "op",
                        "name": "eq",
                        "args": [
                            {
                                "kind": "op",
                                "name": "add",
                                "args": [
                                    {"kind": "const", "value": 40, "sort": {"kind": "ctor", "name": "Int", "args": []}},
                                    {"kind": "const", "value": 2, "sort": {"kind": "ctor", "name": "Int", "args": []}}
                                ]
                            },
                            {"kind": "const", "value": 42, "sort": {"kind": "ctor", "name": "Int", "args": []}}
                        ]
                    },
                    {
                        "kind": "op",
                        "name": "not",
                        "args": [{
                            "kind": "op",
                            "name": "le",
                            "args": [
                                {"kind": "const", "value": 9, "sort": {"kind": "ctor", "name": "Int", "args": []}},
                                {"kind": "const", "value": 3, "sort": {"kind": "ctor", "name": "Int", "args": []}}
                            ]
                        }]
                    }
                ]
            }]
        }
    });

    let c_source = compile_c(&input).expect("compile op term");

    assert!(c_source.contains("((40) + (2))"));
    assert!(c_source.contains("(((40) + (2)) == (42))"));
    assert!(c_source.contains("(!((9) <= (3)))"));
    assert!(c_source.contains("&&"));
}

#[test]
fn projects_source_unit_to_operational_term() {
    let input = json!({
        "kind": "op",
        "name": "source-unit",
        "args": [
            {"kind": "bytes", "encoding": "hex", "value": "696e74206628696e742078297b72657475726e20782b313b7d0a"},
            {
                "kind": "op",
                "name": "return",
                "args": [{
                    "kind": "op",
                    "name": "add",
                    "args": [
                        {"kind": "var", "name": "x"},
                        {"kind": "const", "value": 1, "sort": {"kind": "ctor", "name": "Int", "args": []}}
                    ]
                }]
            }
        ]
    });

    let c_source = compile_c(&input).expect("compile source-unit operational projection");

    assert!(c_source.contains("int proofir_term(int x) {"));
    assert!(c_source.contains("return (((x) + (1)));"));
}

#[test]
fn lowers_concrete_c11_ops_and_casts() {
    let input = json!({
        "kind": "c11-algebra-term",
        "source": "cast.c",
        "term": {
            "kind": "op",
            "name": "return",
            "args": [{
                "kind": "op",
                "name": "bop_add",
                "args": [
                    {
                        "kind": "op",
                        "name": "cast",
                        "args": [
                            {"kind": "var", "name": "int"},
                            {"kind": "var", "name": "x"}
                        ]
                    },
                    {"kind": "const", "value": 1, "sort": {"kind": "ctor", "name": "Int", "args": []}}
                ]
            }]
        }
    });

    let c_source = compile_c(&input).expect("compile concrete op and typed cast");

    assert!(c_source.contains("int cast(int x) {"));
    assert!(!c_source.contains("int int"));
    assert!(c_source.contains("((int)(x))"));
    assert!(c_source.contains("+ (1)"));
}

#[test]
fn asm_link_edge_is_cleanly_rejected_as_a_boundary_op() {
    let input = json!({
        "kind": "c11-algebra-term",
        "source": "asm.c",
        "term": {
            "kind": "op",
            "name": "seq",
            "args": [
                {
                    "kind": "op",
                    "name": "asm-link-edge",
                    "args": [
                        {"kind": "var", "name": "blake3-512:path"},
                        {"kind": "var", "name": "blake3-512:asm"},
                        {"kind": "var", "name": "x86-64:sysv"},
                        {"kind": "var", "name": "provekit-lift-asm-x86-64"},
                        {"kind": "var", "name": "provekit_inline_asm"},
                        {"kind": "var", "name": "gnu-inline-asm"},
                        {"kind": "literal", "value": "nop"},
                        {"kind": "literal", "value": ".text\n"},
                        {"kind": "op", "name": "set", "args": []},
                        {"kind": "op", "name": "set", "args": []},
                        {"kind": "op", "name": "set", "args": []}
                    ]
                },
                {"kind": "op", "name": "return", "args": [{"kind": "var", "name": "x"}]}
            ]
        }
    });

    let err = compile_c(&input).expect_err("asm-link-edge is a link boundary");
    assert!(err
        .to_string()
        .contains("unsupported predicate: asm-link-edge"));
}

#[test]
fn lowers_statement_if_and_expression_if_differently() {
    let input = json!({
        "kind": "c11-algebra-term",
        "source": "choose.c",
        "term": {
            "kind": "op",
            "name": "return",
            "args": [{
                "kind": "op",
                "name": "if",
                "args": [
                    {"kind": "op", "name": "lt", "args": [
                        {"kind": "var", "name": "x"},
                        {"kind": "const", "value": 0, "sort": {"kind": "ctor", "name": "Int", "args": []}}
                    ]},
                    {"kind": "op", "name": "neg", "args": [{"kind": "var", "name": "x"}]},
                    {"kind": "var", "name": "x"}
                ]
            }]
        }
    });

    let c_source = compile_c(&input).expect("compile expression if");

    assert!(c_source.contains("?"));
    assert!(c_source.contains(":"));
    assert!(!c_source.contains("if ((x) < (0)) {"));
}

#[test]
fn refuses_unknown_operations() {
    let input = json!({
        "kind": "c11-algebra-term",
        "source": "bad.c",
        "term": {"kind": "op", "name": "switch", "args": []}
    });

    let err = compile_c(&input).expect_err("switch is outside the core subset");
    assert!(err.to_string().contains("unsupported predicate: switch"));
}

#[test]
fn implements_ir_compiler_trait_for_c11() {
    let input: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/foo.term.json")).expect("fixture json");
    let compiler = CCompiler::new();

    let compiled = compiler
        .compile(&input, DIALECT)
        .expect("compile through trait");

    assert_eq!(compiled.preamble, "");
    assert_eq!(compiled.body, include_str!("fixtures/foo.expected.c"));
}

#[ignore = "requires cc, gcc, or clang on PATH"]
#[test]
fn ignored_compiles_links_and_runs_foo_when_c_compiler_is_available() {
    let Some(compiler) = c_compiler() else {
        eprintln!("C compiler not available, skipping runtime smoke");
        return;
    };

    let input: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/foo.term.json")).expect("fixture json");
    let c_source = compile_c(&input).expect("foo term compiles");

    let dir = tempfile::tempdir().expect("tempdir");
    let source_path = dir.path().join("foo.c");
    let harness_path = dir.path().join("harness.c");
    let bin_path = dir.path().join("harness");

    fs::write(&source_path, c_source).expect("write source");
    fs::write(
        &harness_path,
        r#"
#include <stdint.h>

int foo(int x);

int main(void) {
    if (foo(0) != -22) {
        return 1;
    }
    if (foo(42) != 42) {
        return 2;
    }
    return 0;
}
"#,
    )
    .expect("write harness");

    let status = Command::new(&compiler)
        .arg("-std=c11")
        .arg(&source_path)
        .arg(&harness_path)
        .arg("-o")
        .arg(&bin_path)
        .status()
        .expect("run C compiler");
    assert!(status.success(), "{compiler} failed with {status}");

    let status = Command::new(&bin_path).status().expect("run harness");
    assert!(status.success(), "harness failed with {status}");
}

#[ignore = "requires a prebuilt provekit-lift-c-collectors-defensive binary"]
#[test]
fn ignored_round_trips_foo_contract_when_c_lifter_is_available() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .expect("repo root");
    let lifter = repo_root
        .join("implementations/c/provekit-lift-c-collectors-defensive")
        .join("provekit-lift-c-collectors-defensive");
    if Command::new(&lifter).arg("--help").output().is_err() {
        eprintln!("C lifter binary not available, skipping round-trip smoke");
        return;
    }

    let input: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/foo.term.json")).expect("fixture json");
    let c_source = compile_c(&input).expect("foo term compiles");

    let dir = tempfile::tempdir().expect("tempdir");
    let source_path = dir.path().join("foo.c");
    fs::write(&source_path, c_source).expect("write source");

    let output = Command::new(&lifter)
        .arg(&source_path)
        .output()
        .expect("run C lifter");
    assert!(output.status.success(), "C lifter failed");

    let re_lifted = String::from_utf8_lossy(&output.stdout);
    let actual_contract = extract_lifted_contract(&re_lifted).expect("lifted function contract");
    let expected_contracts: Json = serde_json::from_str(include_str!(
        "../../../../menagerie/c11-language-signature/example/foo.contract.json"
    ))
    .expect("direct foo contract fixture parses");
    let expected_contract = expected_contracts
        .as_array()
        .and_then(|items| items.first())
        .expect("direct foo contract fixture has one contract");

    let actual_cid = cid_of_json(&actual_contract);
    let expected_cid = cid_of_json(expected_contract);

    assert_eq!(actual_cid, expected_cid);
}

fn c_compiler() -> Option<String> {
    ["cc", "gcc", "clang"]
        .into_iter()
        .find(|cmd| Command::new(cmd).arg("--version").output().is_ok())
        .map(str::to_string)
}

fn extract_lifted_contract(output: &str) -> Option<Json> {
    for line in output.lines() {
        let parsed: Json = serde_json::from_str(line).ok()?;
        if let Some(contract) = parsed.pointer("/result/declarations/0") {
            return Some(contract.clone());
        }
        if let Some(contract) = parsed.pointer("/result/ir/0") {
            return Some(contract.clone());
        }
    }
    None
}

fn cid_of_json(value: &Json) -> String {
    let canonical = to_cvalue(value);
    let jcs = encode_jcs(&canonical);
    blake3_512_of(jcs.as_bytes())
}

fn to_cvalue(value: &Json) -> Arc<CValue> {
    match value {
        Json::Null => CValue::null(),
        Json::Bool(value) => CValue::boolean(*value),
        Json::Number(value) => {
            if let Some(value) = value.as_i64() {
                CValue::integer(value)
            } else if let Some(value) = value.as_u64() {
                CValue::string(value.to_string())
            } else if let Some(value) = value.as_f64() {
                CValue::string(value.to_string())
            } else {
                CValue::null()
            }
        }
        Json::String(value) => CValue::string(value.clone()),
        Json::Array(values) => CValue::array(values.iter().map(to_cvalue).collect()),
        Json::Object(values) => CValue::object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), to_cvalue(value))),
        ),
    }
}
