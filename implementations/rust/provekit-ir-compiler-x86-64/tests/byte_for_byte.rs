use provekit_ir_compiler::IrCompiler;
use provekit_ir_compiler_x86_64::{emit, X8664Compiler, DIALECT};
use serde_json::json;

fn fixtures() -> Vec<serde_json::Value> {
    vec![
        serde_json::from_str(include_str!("fixtures/foo.term.json")).expect("foo fixture"),
        json!({
            "kind": "op",
            "name": "return",
            "args": [{
                "kind": "op",
                "name": "sub",
                "args": [
                    {"kind": "var", "name": "x"},
                    {"kind": "const", "value": 7, "sort": {"kind": "ctor", "name": "Int", "args": []}}
                ]
            }]
        }),
        json!({
            "kind": "op",
            "name": "return",
            "args": [{
                "kind": "op",
                "name": "or",
                "args": [
                    {"kind": "op", "name": "eq", "args": [
                        {"kind": "var", "name": "x"},
                        {"kind": "const", "value": 1, "sort": {"kind": "ctor", "name": "Int", "args": []}}
                    ]},
                    {"kind": "op", "name": "lt", "args": [
                        {"kind": "var", "name": "y"},
                        {"kind": "const", "value": 9, "sort": {"kind": "ctor", "name": "Int", "args": []}}
                    ]}
                ]
            }]
        }),
    ]
}

#[test]
fn emit_is_byte_deterministic() {
    for fixture in fixtures() {
        let first = emit(&fixture).expect("first emit");
        let second = emit(&fixture).expect("second emit");
        assert_eq!(first, second);
    }
}

#[test]
fn trait_compile_body_equals_emit_string() {
    let compiler = X8664Compiler::new();
    for fixture in fixtures() {
        let through_trait = compiler.compile(&fixture, DIALECT).expect("compile");
        let direct = emit(&fixture).expect("emit");
        assert_eq!(through_trait.preamble, "");
        assert_eq!(through_trait.body, direct);
        assert_eq!(through_trait.free_vars.len(), 0);
    }
}

#[test]
fn trait_compile_rejects_wrong_dialect() {
    let compiler = X8664Compiler::new();
    let term = json!({"kind": "op", "name": "skip", "args": [{"kind": "unit"}]});
    let result = compiler.compile(&term, "coq");
    assert!(matches!(
        result,
        Err(provekit_ir_compiler::CompileError::UnsupportedDialect(_))
    ));
}
