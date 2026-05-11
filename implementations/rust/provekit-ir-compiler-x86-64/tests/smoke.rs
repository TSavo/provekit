use std::fs;
use std::process::Command;

use provekit_ir_compiler_x86_64::{TermCompiler, X8664Compiler};
use serde_json::json;

#[test]
fn smoke_compiles_foo_term_byte_for_byte() {
    let input: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/foo.term.json")).expect("fixture json");
    let expected = include_str!("fixtures/foo.expected.s");

    let asm = X8664Compiler::new()
        .compile_term_json(&input)
        .expect("foo term compiles");

    assert_eq!(asm, expected);
}

#[test]
fn lowers_arithmetic_and_comparison_ops() {
    let compiler = X8664Compiler::new();
    let term = json!({
        "kind": "op",
        "name": "return",
        "args": [{
            "kind": "op",
            "name": "add",
            "args": [
                {"kind": "var", "name": "x"},
                {
                    "kind": "op",
                    "name": "mul",
                    "args": [
                        {"kind": "const", "value": 2, "sort": {"kind": "ctor", "name": "Int", "args": []}},
                        {"kind": "const", "value": 3, "sort": {"kind": "ctor", "name": "Int", "args": []}}
                    ]
                }
            ]
        }]
    });

    let asm = compiler.compile_term_json(&term).expect("term compiles");

    assert!(asm.contains("imul    eax, ecx"));
    assert!(asm.contains("add     eax, ecx"));
    assert!(asm.contains("ret\n"));
}

#[test]
fn projects_source_unit_to_operational_term() {
    let compiler = X8664Compiler::new();
    let term = json!({
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

    let asm = compiler
        .compile_term_json(&term)
        .expect("compile source-unit operational projection");

    assert!(asm.contains(".globl proofir_term"));
    assert!(asm.contains("add     eax, ecx"));
}

#[test]
fn lowers_concrete_c11_ops_and_casts() {
    let compiler = X8664Compiler::new();
    let term = json!({
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

    let asm = compiler
        .compile_term_json(&term)
        .expect("compile concrete op and typed cast");

    assert!(asm.contains("mov     eax, edi"));
    assert!(!asm.contains("mov     eax, esi"));
    assert!(asm.contains("neg     eax"));
    assert!(asm.contains("imul    eax, ecx"));
}

#[test]
fn asm_link_edge_is_cleanly_rejected_as_a_boundary_op() {
    let compiler = X8664Compiler::new();
    let term = json!({
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

    let err = compiler
        .compile_term_json(&term)
        .expect_err("asm-link-edge is a link boundary");
    assert!(err
        .to_string()
        .contains("unsupported operation asm-link-edge"));
}

#[test]
fn refuses_unknown_operations() {
    let compiler = X8664Compiler::new();
    let term = json!({"kind": "op", "name": "switch", "args": []});

    let err = compiler
        .compile_term_json(&term)
        .expect_err("switch is not core");

    assert!(err.to_string().contains("unsupported operation switch"));
}

#[ignore = "requires GNU/Linux gcc or compatible x86-64 SysV assembler and runner"]
#[test]
fn ignored_assembles_and_runs_foo() {
    if !has_linux_gcc() {
        eprintln!("skipping: gcc is not a GNU/Linux x86-64 SysV toolchain");
        return;
    }

    let input: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/foo.term.json")).expect("fixture json");
    let asm = X8664Compiler::new()
        .compile_term_json(&input)
        .expect("foo term compiles");

    let dir = tempfile::tempdir().expect("tempdir");
    let asm_path = dir.path().join("foo.s");
    let harness_path = dir.path().join("harness.c");
    let bin_path = dir.path().join("harness");

    fs::write(&asm_path, asm).expect("write asm");
    fs::write(
        &harness_path,
        r#"
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

    let status = Command::new("gcc")
        .arg(&harness_path)
        .arg(&asm_path)
        .arg("-o")
        .arg(&bin_path)
        .status()
        .expect("run gcc");
    assert!(status.success(), "gcc failed with {status}");

    let status = Command::new(&bin_path).status().expect("run harness");
    assert!(status.success(), "harness failed with {status}");
}

fn has_linux_gcc() -> bool {
    let Ok(output) = Command::new("gcc").arg("-dumpmachine").output() else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let machine = String::from_utf8_lossy(&output.stdout);
    machine.contains("linux") && (machine.contains("x86_64") || machine.contains("amd64"))
}
