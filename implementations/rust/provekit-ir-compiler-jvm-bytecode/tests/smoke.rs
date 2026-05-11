// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::Path;
use std::process::Command;

use provekit_ir_compiler::IrCompiler;
use provekit_ir_compiler_jvm_bytecode::{
    compile_jasmin, JvmBytecodeCompiler, TermCompiler, DIALECT,
};
use serde_json::json;
use serde_json::Value as Json;

#[test]
fn compiles_foo_term_to_checked_fixture() {
    let input: Json =
        serde_json::from_str(include_str!("fixtures/foo.term.json")).expect("foo term json parses");
    let expected = include_str!("fixtures/foo.expected.j");

    let jasmin = compile_jasmin(&input).expect("compile foo term");

    assert_eq!(jasmin, expected);
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
                        "name": "le",
                        "args": [
                            {"kind": "const", "value": 1, "sort": {"kind": "ctor", "name": "Int", "args": []}},
                            {"kind": "const", "value": 2, "sort": {"kind": "ctor", "name": "Int", "args": []}}
                        ]
                    }
                ]
            }]
        }
    });

    let jasmin = compile_jasmin(&input).expect("compile op term");

    assert!(jasmin.contains("iadd"));
    assert!(jasmin.contains("if_icmpeq"));
    assert!(jasmin.contains("if_icmple"));
    assert!(jasmin.contains("ifeq"));
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

    let jasmin = compile_jasmin(&input).expect("compile source-unit operational projection");

    assert!(jasmin.contains(".method public static proofir_term(I)I"));
    assert!(jasmin.contains("iadd"));
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
                "name": "bop_mul",
                "args": [
                    {
                        "kind": "op",
                        "name": "cast",
                        "args": [
                            {"kind": "var", "name": "int"},
                            {"kind": "var", "name": "x"}
                        ]
                    },
                    {
                        "kind": "op",
                        "name": "uop_neg",
                        "args": [{"kind": "const", "value": 2, "sort": {"kind": "ctor", "name": "Int", "args": []}}]
                    }
                ]
            }]
        }
    });

    let jasmin = compile_jasmin(&input).expect("compile concrete op and typed cast");

    assert!(jasmin.contains(".method public static cast(I)I"));
    assert!(!jasmin.contains(".method public static cast(II)I"));
    assert!(jasmin.contains("iload 0"));
    assert!(jasmin.contains("ineg"));
    assert!(jasmin.contains("imul"));
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

    let err = compile_jasmin(&input).expect_err("asm-link-edge is a link boundary");
    assert!(err
        .to_string()
        .contains("unsupported operation asm-link-edge"));
}

#[test]
fn lowers_memory_ops_with_static_int_memory() {
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

    let jasmin = compile_jasmin(&input).expect("compile memory term");

    assert!(jasmin.contains(".field private static memory [I"));
    assert!(jasmin.contains("iastore"));
    assert!(jasmin.contains("iaload"));
}

#[test]
fn refuses_unknown_operations() {
    let input = json!({
        "kind": "c11-algebra-term",
        "source": "bad.c",
        "term": {"kind": "op", "name": "switch", "args": []}
    });

    let err = compile_jasmin(&input).expect_err("switch is outside the core subset");
    assert!(err.to_string().contains("unsupported operation switch"));
}

#[test]
fn implements_ir_compiler_trait_for_jasmin() {
    let input: Json =
        serde_json::from_str(include_str!("fixtures/foo.term.json")).expect("foo term json parses");
    let compiler = JvmBytecodeCompiler::new();

    let compiled = compiler
        .compile(&input, DIALECT)
        .expect("compile through trait");

    assert_eq!(compiled.preamble, "");
    assert_eq!(compiled.body, include_str!("fixtures/foo.expected.j"));
}

#[ignore = "requires jasmin or krak2 plus javac and java"]
#[test]
fn assembles_and_runs_foo_when_toolchain_is_available() {
    if Command::new("javac").arg("-version").output().is_err()
        || Command::new("java").arg("-version").output().is_err()
    {
        eprintln!("javac or java unavailable, skipping runtime smoke");
        return;
    }

    let input: Json =
        serde_json::from_str(include_str!("fixtures/foo.term.json")).expect("foo term json parses");
    let jasmin = JvmBytecodeCompiler::new()
        .compile_term_json(&input)
        .expect("foo term compiles");

    let dir = std::env::temp_dir().join(format!(
        "provekit-ir-compiler-jvm-bytecode-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    let jasmin_path = dir.join("Foo.j");
    let harness_path = dir.join("Harness.java");

    fs::write(&jasmin_path, jasmin).expect("write jasmin");
    fs::write(
        &harness_path,
        "public class Harness {\n  public static void main(String[] args) {\n    if (Foo.foo(0) != -22) { System.exit(1); }\n    if (Foo.foo(42) != 42) { System.exit(2); }\n  }\n}\n",
    )
    .expect("write harness");

    if !assemble_jasmin(&jasmin_path, &dir) {
        eprintln!("jasmin or krak2 unavailable, skipping runtime smoke");
        let _ = fs::remove_dir_all(&dir);
        return;
    }

    let javac = Command::new("javac")
        .arg("-classpath")
        .arg(&dir)
        .arg("-d")
        .arg(&dir)
        .arg(&harness_path)
        .status()
        .expect("run javac");
    assert!(javac.success(), "javac failed with {javac}");

    let run = Command::new("java")
        .arg("-cp")
        .arg(&dir)
        .arg("Harness")
        .status()
        .expect("run harness");

    let _ = fs::remove_dir_all(&dir);

    assert!(run.success(), "harness failed with {run}");
}

fn assemble_jasmin(jasmin_path: &Path, out_dir: &Path) -> bool {
    if let Ok(status) = Command::new("jasmin")
        .arg("-d")
        .arg(out_dir)
        .arg(jasmin_path)
        .status()
    {
        if status.success() {
            return true;
        }
    }

    if let Ok(status) = Command::new("krak2")
        .arg("asm")
        .arg(jasmin_path)
        .arg("-out")
        .arg(out_dir)
        .status()
    {
        return status.success();
    }

    false
}
