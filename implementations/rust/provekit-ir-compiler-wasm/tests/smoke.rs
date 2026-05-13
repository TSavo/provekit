// SPDX-License-Identifier: Apache-2.0

use std::io::Write;
use std::process::Command;

use serde_json::json;
use serde_json::Value as Json;

use provekit_ir_compiler::IrCompiler;
use provekit_ir_compiler_wasm::{compile_wat, WasmCompiler, DIALECT};

#[test]
fn compiles_foo_term_to_checked_fixture() {
    let input: Json = serde_json::from_str(include_str!(
        "../../../../menagerie/c11-language-signature/example/foo.term.json"
    ))
    .expect("foo term json parses");
    let expected = include_str!("../fixtures/foo.wat");

    let wat = compile_wat(&input).expect("compile foo term");

    assert_eq!(wat, expected);
}

#[test]
fn lowers_arithmetic_and_comparison_ops() {
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
                        "name": "lt",
                        "args": [
                            {"kind": "const", "value": 1, "sort": {"kind": "ctor", "name": "Int", "args": []}},
                            {"kind": "const", "value": 2, "sort": {"kind": "ctor", "name": "Int", "args": []}}
                        ]
                    }
                ]
            }]
        }
    });

    let wat = compile_wat(&input).expect("compile op term");

    assert!(wat.contains("i32.add"));
    assert!(wat.contains("i32.eq"));
    assert!(wat.contains("i32.lt_s"));
    assert!(wat.contains("i32.and"));
}

#[test]
fn lowers_c11_logical_ops_as_short_circuit_truthy_ops() {
    let input = json!({
        "kind": "c11-algebra-term",
        "source": "logical.c",
        "term": {
            "kind": "op",
            "name": "return",
            "args": [{
                "kind": "op",
                "name": "bop_logand",
                "args": [
                    {"kind": "const", "value": 2, "sort": {"kind": "ctor", "name": "Int", "args": []}},
                    {
                        "kind": "op",
                        "name": "bop_logor",
                        "args": [
                            {"kind": "const", "value": 0, "sort": {"kind": "ctor", "name": "Int", "args": []}},
                            {"kind": "const", "value": 4, "sort": {"kind": "ctor", "name": "Int", "args": []}}
                        ]
                    }
                ]
            }]
        }
    });

    let wat = compile_wat(&input).expect("compile concrete logical op term");

    assert!(wat.contains("if (result i32)"));
    assert!(wat.contains("i32.const 1"));
    assert!(wat.contains("i32.eqz\n      i32.eqz"));
    assert!(
        !wat.contains("i32.and"),
        "C logical && must not lower to eager bitwise and"
    );
    assert!(
        !wat.contains("i32.or"),
        "C logical || must not lower to eager bitwise or"
    );
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
                    "name": "bop_add",
                    "args": [
                        {"kind": "var", "name": "x"},
                        {"kind": "const", "value": 1, "sort": {"kind": "ctor", "name": "Int", "args": []}}
                    ]
                }]
            }
        ]
    });

    let wat = compile_wat(&input).expect("compile source-unit operational projection");

    assert!(wat.contains("(param $x i32)"));
    assert!(wat.contains("i32.add"));
}

#[test]
fn cast_uses_value_slot_not_target_type_as_runtime_input() {
    let input = json!({
        "kind": "c11-algebra-term",
        "source": "cast.c",
        "term": {
            "kind": "op",
            "name": "return",
            "args": [{
                "kind": "op",
                "name": "cast",
                "args": [
                    {"kind": "var", "name": "int"},
                    {"kind": "var", "name": "x"}
                ]
            }]
        }
    });

    let wat = compile_wat(&input).expect("compile typed cast projection");

    assert!(wat.contains("(param $x i32)"));
    assert!(!wat.contains("(param $int i32)"));
    assert!(wat.contains("local.get $x"));
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

    let err = compile_wat(&input).expect_err("asm-link-edge is a link boundary");
    assert!(err
        .to_string()
        .contains("unsupported predicate: asm-link-edge"));
}

#[test]
fn lowers_memory_ops_with_exported_memory() {
    let input = json!({
        "kind": "c11-algebra-term",
        "source": "mem.c",
        "term": {
            "kind": "op",
            "name": "seq",
            "args": [
                {
                    "kind": "op",
                    "name": "assign",
                    "args": [
                        {"kind": "const", "value": 0, "sort": {"kind": "ctor", "name": "Int", "args": []}},
                        {"kind": "const", "value": 7, "sort": {"kind": "ctor", "name": "Int", "args": []}}
                    ]
                },
                {
                    "kind": "op",
                    "name": "return",
                    "args": [{
                        "kind": "op",
                        "name": "deref",
                        "args": [
                            {"kind": "const", "value": 0, "sort": {"kind": "ctor", "name": "Int", "args": []}}
                        ]
                    }]
                }
            ]
        }
    });

    let wat = compile_wat(&input).expect("compile memory term");

    assert!(wat.contains("(memory (export \"memory\") 1)"));
    assert!(wat.contains("i32.store"));
    assert!(wat.contains("i32.load"));
}

#[test]
fn refuses_unknown_operations() {
    let input = json!({
        "kind": "c11-algebra-term",
        "source": "bad.c",
        "term": {"kind": "op", "name": "switch", "args": []}
    });

    let err = compile_wat(&input).expect_err("switch is outside the core subset");
    assert!(err.to_string().contains("unsupported predicate: switch"));
}

#[test]
fn implements_ir_compiler_trait_for_wat() {
    let input: Json = serde_json::from_str(include_str!(
        "../../../../menagerie/c11-language-signature/example/foo.term.json"
    ))
    .expect("foo term json parses");
    let compiler = WasmCompiler::new();

    let compiled = compiler
        .compile(&input, DIALECT)
        .expect("compile through trait");

    assert_eq!(compiled.preamble, "");
    assert_eq!(compiled.body, include_str!("../fixtures/foo.wat"));
}

#[ignore]
#[test]
fn runs_foo_with_wasmtime_when_available() {
    if Command::new("wasmtime").arg("--version").output().is_err() {
        eprintln!("wasmtime not available, skipping runtime smoke");
        return;
    }

    let input: Json = serde_json::from_str(include_str!(
        "../../../../menagerie/c11-language-signature/example/foo.term.json"
    ))
    .expect("foo term json parses");
    let wat = compile_wat(&input).expect("compile foo term");
    let path = std::env::temp_dir().join(format!(
        "provekit-ir-compiler-wasm-foo-{}.wat",
        std::process::id()
    ));

    {
        let mut file = std::fs::File::create(&path).expect("create temp wat");
        file.write_all(wat.as_bytes()).expect("write temp wat");
    }

    let zero = Command::new("wasmtime")
        .arg("--invoke")
        .arg("foo")
        .arg(&path)
        .arg("0")
        .output()
        .expect("run wasmtime foo 0");
    let forty_two = Command::new("wasmtime")
        .arg("--invoke")
        .arg("foo")
        .arg(&path)
        .arg("42")
        .output()
        .expect("run wasmtime foo 42");

    let _ = std::fs::remove_file(&path);

    assert!(zero.status.success(), "foo(0) failed");
    assert!(forty_two.status.success(), "foo(42) failed");
    assert_eq!(String::from_utf8_lossy(&zero.stdout).trim(), "-22");
    assert_eq!(String::from_utf8_lossy(&forty_two.stdout).trim(), "42");
}
