// SPDX-License-Identifier: Apache-2.0
//
// Coq compiler tests for new IR constructs (lambda, let, choice).

use serde_json::json;

use provekit_ir_compiler::IrCompiler;
use provekit_ir_compiler_coq::{CoqCompiler, DIALECT};

#[test]
fn lambda_emits_coq_fun() {
    let ir = json!({
        "kind": "lambda",
        "paramName": "x",
        "paramSort": {"kind": "primitive", "name": "Int"},
        "body": {"kind": "const", "value": 42, "sort": {"kind": "primitive", "name": "Int"}}
    });
    let compiler = CoqCompiler::new();
    let result = compiler.compile(&ir, DIALECT);
    assert!(result.is_ok(), "compile failed: {:?}", result.err());
    let compiled = result.unwrap();
    let coq_code = compiled.body;
    assert!(coq_code.contains("fun (x : Z)"), "Expected Coq lambda syntax");
    assert!(coq_code.contains("42"), "Expected body in output");
}

#[test]
fn let_emits_coq_let_in() {
    let ir = json!({
        "kind": "let",
        "bindings": [
            {"name": "x", "boundTerm": {"kind": "const", "value": 1, "sort": {"kind": "primitive", "name": "Int"}}}
        ],
        "body": {"kind": "var", "name": "x"}
    });
    let compiler = CoqCompiler::new();
    let result = compiler.compile(&ir, DIALECT);
    assert!(result.is_ok(), "compile failed: {:?}", result.err());
    let compiled = result.unwrap();
    let coq_code = compiled.body;
    assert!(coq_code.contains("let x"), "Expected Coq let syntax");
    assert!(coq_code.contains(" in"), "Expected 'in' keyword");
}

#[test]
fn choice_emits_coq_sig() {
    let ir = json!({
        "kind": "choice",
        "varName": "x",
        "sort": {"kind": "primitive", "name": "Int"},
        "body": {"kind": "atomic", "name": "true", "args": []}
    });
    let compiler = CoqCompiler::new();
    let result = compiler.compile(&ir, DIALECT);
    assert!(result.is_ok(), "compile failed: {:?}", result.err());
    let compiled = result.unwrap();
    let coq_code = compiled.body;
    assert!(coq_code.contains("@sig"), "Expected Coq sig for choice");
    assert!(coq_code.contains("fun x"), "Expected fun binder");
}

#[test]
fn lambda_produces_valid_coq_syntax() {
    let ir = json!({
        "kind": "lambda",
        "paramName": "s",
        "paramSort": {"kind": "primitive", "name": "String"},
        "body": {"kind": "const", "value": "hello", "sort": {"kind": "primitive", "name": "String"}}
    });
    let compiler = CoqCompiler::new();
    let result = compiler.compile(&ir, DIALECT);
    assert!(result.is_ok());
    let compiled = result.unwrap();
    let coq_code = compiled.body;
    // Should have Goal
    assert!(coq_code.contains("Goal"), "Expected Goal");
}

#[test]
fn let_with_multiple_bindings() {
    let ir = json!({
        "kind": "let",
        "bindings": [
            {"name": "x", "boundTerm": {"kind": "const", "value": 1, "sort": {"kind": "primitive", "name": "Int"}}},
            {"name": "y", "boundTerm": {"kind": "const", "value": 2, "sort": {"kind": "primitive", "name": "Int"}}}
        ],
        "body": {"kind": "var", "name": "y"}
    });
    let compiler = CoqCompiler::new();
    let result = compiler.compile(&ir, DIALECT);
    assert!(result.is_ok(), "compile failed: {:?}", result.err());
    let compiled = result.unwrap();
    let coq_code = compiled.body;
    // Should have two let bindings
    let let_count = coq_code.matches("let").count();
    assert!(let_count >= 2, "Expected at least 2 let occurrences, got {}", let_count);
}
