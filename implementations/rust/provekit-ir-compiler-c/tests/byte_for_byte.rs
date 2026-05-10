// SPDX-License-Identifier: Apache-2.0
//
// Byte-for-byte regression check for the C11 compiler. ORP v0.2 compile
// mode is deterministic, so the same term and target descriptor must
// produce the exact same C source bytes.

use provekit_ir_compiler::IrCompiler;
use provekit_ir_compiler_c::{compile_c, CCompiler, DIALECT};
use serde_json::json;

fn fixtures() -> Vec<serde_json::Value> {
    vec![
        serde_json::from_str(include_str!("fixtures/foo.term.json")).expect("foo fixture"),
        json!({
            "kind": "c11-algebra-term",
            "source": "arith.c",
            "term": {
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
            }
        }),
        json!({
            "kind": "c11-algebra-term",
            "source": "bools.c",
            "term": {
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
            }
        }),
    ]
}

#[test]
fn emit_is_byte_deterministic() {
    for fixture in fixtures() {
        let first = compile_c(&fixture).expect("first emit");
        let second = compile_c(&fixture).expect("second emit");
        assert_eq!(first, second);
    }
}

#[test]
fn trait_compile_body_equals_emit_string() {
    let compiler = CCompiler::new();
    for fixture in fixtures() {
        let through_trait = compiler.compile(&fixture, DIALECT).expect("compile");
        let direct = compile_c(&fixture).expect("emit");
        assert_eq!(through_trait.preamble, "");
        assert_eq!(through_trait.body, direct);
        assert_eq!(through_trait.free_vars.len(), 0);
    }
}

#[test]
fn trait_compile_rejects_wrong_dialect() {
    let compiler = CCompiler::new();
    let term = json!({"kind": "op", "name": "skip", "args": [{"kind": "unit"}]});
    let result = compiler.compile(&term, "coq");
    assert!(matches!(
        result,
        Err(provekit_ir_compiler::CompileError::UnsupportedDialect(_))
    ));
}
