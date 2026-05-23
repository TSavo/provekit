// SPDX-License-Identifier: Apache-2.0
//
// End-to-end: a real contract (over a real function under test) goes IN as
// neutral predicates; this kit emits a cargo-test module; we splice that
// module beside the function, compile it with `rustc --test`, run the test
// binary, and assert it builds AND passes (exit 0).
//
// This proves the emitted assertions are not just textually plausible -- they
// COMPILE and the contract holds green for a correct implementation.

use std::io::Write;
use std::process::Command;

use provekit_emit_rust_cargo_test::{emit_test_module, FunctionSignature};
use provekit_ir_types::{IrFormula, IrTerm, Sort};

fn var(name: &str) -> IrTerm {
    IrTerm::Var {
        name: name.to_string(),
    }
}

fn int_const(n: i64) -> IrTerm {
    IrTerm::Const {
        value: serde_json::json!(n),
        sort: Sort::Primitive {
            name: "i64".to_string(),
        },
    }
}

fn atom(name: &str, args: Vec<IrTerm>) -> IrFormula {
    IrFormula::Atomic {
        name: name.to_string(),
        args,
    }
}

/// Compile `source` with `rustc --test`, run the resulting binary, and return
/// whether (a) it compiled and (b) the tests passed.
fn compile_and_run_tests(source: &str) -> (bool, bool, String) {
    let dir = tempfile::Builder::new()
        .prefix("emit-cargo-test-e2e")
        .tempdir()
        .expect("tempdir");
    let src_path = dir.path().join("contract_under_test.rs");
    {
        let mut f = std::fs::File::create(&src_path).expect("create src");
        f.write_all(source.as_bytes()).expect("write src");
    }
    let bin_path = dir.path().join("contract_test_bin");
    let compile = Command::new("rustc")
        .arg("--test")
        .arg("--edition=2021")
        .arg("-o")
        .arg(&bin_path)
        .arg(&src_path)
        .output()
        .expect("invoke rustc");
    if !compile.status.success() {
        return (
            false,
            false,
            format!(
                "rustc failed:\nsource:\n{source}\nstderr:\n{}",
                String::from_utf8_lossy(&compile.stderr)
            ),
        );
    }
    let run = Command::new(&bin_path).output().expect("run test bin");
    (
        true,
        run.status.success(),
        format!(
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        ),
    )
}

#[test]
fn e2e_eq_contract_compiles_and_passes() {
    // Function under test: `add(a, b) -> i64`. Contract: `add(2, 3) == 5`.
    let module = emit_test_module(
        &FunctionSignature {
            function: "add".to_string(),
            params: vec!["a".to_string(), "b".to_string()],
            ..Default::default()
        },
        &[atom(
            "=",
            vec![
                IrTerm::Ctor {
                    name: "add".to_string(),
                    args: vec![int_const(2), int_const(3)],
                },
                int_const(5),
            ],
        )],
    );

    let source = format!(
        "pub fn add(a: i64, b: i64) -> i64 {{ a + b }}\n\n{}",
        module.source
    );

    let (compiled, passed, detail) = compile_and_run_tests(&source);
    assert!(compiled, "emitted contract test must COMPILE.\n{detail}");
    assert!(passed, "emitted contract test must PASS (green).\n{detail}");
    assert_eq!(module.tests.len(), 1);
    assert!(module.skipped.is_empty(), "skipped: {:?}", module.skipped);
}

#[test]
fn e2e_option_is_some_contract_compiles_and_passes() {
    // Function under test: `lookup(k) -> Option<i64>`. Contract: result is Some.
    let some_wild = IrTerm::Ctor {
        name: "Some".to_string(),
        args: vec![var("_")],
    };
    let module = emit_test_module(
        &FunctionSignature {
            function: "lookup".to_string(),
            params: vec!["k".to_string()],
            ..Default::default()
        },
        &[atom(
            "=",
            vec![
                IrTerm::Ctor {
                    name: "lookup".to_string(),
                    args: vec![int_const(1)],
                },
                some_wild,
            ],
        )],
    );

    // Sanity: the option shape collapsed to `.is_some()`.
    assert!(
        module.source.contains("lookup(1).is_some()"),
        "got: {}",
        module.source
    );

    let source = format!(
        "pub fn lookup(k: i64) -> Option<i64> {{ if k == 1 {{ Some(10) }} else {{ None }} }}\n\n{}",
        module.source
    );

    let (compiled, passed, detail) = compile_and_run_tests(&source);
    assert!(compiled, "emitted option contract test must COMPILE.\n{detail}");
    assert!(passed, "emitted option contract test must PASS (green).\n{detail}");
}

#[test]
fn e2e_multi_predicate_conjunction_compiles_and_passes() {
    // Contract: `clamp(5) >= 0` AND `clamp(5) <= 10` AND `clamp(5) == 5`.
    let call = || IrTerm::Ctor {
        name: "clamp".to_string(),
        args: vec![int_const(5)],
    };
    let conj = IrFormula::And {
        operands: vec![
            atom("\u{2265}", vec![call(), int_const(0)]),
            atom("\u{2264}", vec![call(), int_const(10)]),
            atom("=", vec![call(), int_const(5)]),
        ],
    };
    let module = emit_test_module(
        &FunctionSignature {
            function: "clamp".to_string(),
            ..Default::default()
        },
        &[conj],
    );
    assert_eq!(module.tests.len(), 3);

    let source = format!(
        "pub fn clamp(x: i64) -> i64 {{ x.max(0).min(10) }}\n\n{}",
        module.source
    );
    let (compiled, passed, detail) = compile_and_run_tests(&source);
    assert!(compiled, "conjunction contract must COMPILE.\n{detail}");
    assert!(passed, "conjunction contract must PASS (green).\n{detail}");
}
