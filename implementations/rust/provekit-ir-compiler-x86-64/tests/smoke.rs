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
