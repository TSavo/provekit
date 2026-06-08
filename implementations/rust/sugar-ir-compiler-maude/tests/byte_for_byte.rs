use serde_json::{json, Value as Json};
use sugar_ir_compiler::IrCompiler;
use sugar_ir_compiler_maude::{emit, MaudeCompiler, DIALECT};

fn nat_obligation() -> Json {
    json!({
        "kind": "atomic",
        "name": "equational_theory",
        "theory": {
            "name": "provekit-nat",
            "sorts": ["Nat"],
            "operators": [
                {"name": "zero", "arity": [], "result": "Nat"},
                {"name": "s", "arity": ["Nat"], "result": "Nat"},
                {"name": "plus", "arity": ["Nat", "Nat"], "result": "Nat"}
            ],
            "variables": [
                {"name": "N", "sort": "Nat"},
                {"name": "M", "sort": "Nat"}
            ],
            "equations": [
                {
                    "label": "plus-zero-left",
                    "lhs": {"kind": "ctor", "name": "plus", "args": [
                        {"kind": "ctor", "name": "zero", "args": []},
                        {"kind": "var", "name": "N"}
                    ]},
                    "rhs": {"kind": "var", "name": "N"}
                },
                {
                    "label": "plus-s-left",
                    "lhs": {"kind": "ctor", "name": "plus", "args": [
                        {"kind": "ctor", "name": "s", "args": [
                            {"kind": "var", "name": "N"}
                        ]},
                        {"kind": "var", "name": "M"}
                    ]},
                    "rhs": {"kind": "ctor", "name": "s", "args": [
                        {"kind": "ctor", "name": "plus", "args": [
                            {"kind": "var", "name": "N"},
                            {"kind": "var", "name": "M"}
                        ]}
                    ]}
                }
            ]
        },
        "obligation": {
            "lhs": {"kind": "ctor", "name": "plus", "args": [
                {"kind": "ctor", "name": "s", "args": [
                    {"kind": "ctor", "name": "zero", "args": []}
                ]},
                {"kind": "ctor", "name": "s", "args": [
                    {"kind": "ctor", "name": "zero", "args": []}
                ]}
            ]},
            "rhs": {"kind": "ctor", "name": "s", "args": [
                {"kind": "ctor", "name": "s", "args": [
                    {"kind": "ctor", "name": "zero", "args": []}
                ]}
            ]}
        }
    })
}

fn ac_obligation() -> Json {
    json!({
        "kind": "atomic",
        "name": "equational_theory",
        "theory": {
            "name": "provekit-ac",
            "sorts": ["Elt"],
            "operators": [
                {"name": "a", "arity": [], "result": "Elt"},
                {"name": "b", "arity": [], "result": "Elt"},
                {"name": "c", "arity": [], "result": "Elt"},
                {"name": "d", "arity": [], "result": "Elt"},
                {"name": "plus", "maude": "_+_", "arity": ["Elt", "Elt"], "result": "Elt", "attrs": ["assoc", "comm"]}
            ],
            "equations": []
        },
        "obligation": {
            "lhs": {"kind": "ctor", "name": "plus", "args": [
                {"kind": "ctor", "name": "plus", "args": [
                    {"kind": "ctor", "name": "plus", "args": [
                        {"kind": "ctor", "name": "a", "args": []},
                        {"kind": "ctor", "name": "b", "args": []}
                    ]},
                    {"kind": "ctor", "name": "c", "args": []}
                ]},
                {"kind": "ctor", "name": "d", "args": []}
            ]},
            "rhs": {"kind": "ctor", "name": "plus", "args": [
                {"kind": "ctor", "name": "a", "args": []},
                {"kind": "ctor", "name": "plus", "args": [
                    {"kind": "ctor", "name": "b", "args": []},
                    {"kind": "ctor", "name": "plus", "args": [
                        {"kind": "ctor", "name": "c", "args": []},
                        {"kind": "ctor", "name": "d", "args": []}
                    ]}
                ]}
            ]}
        }
    })
}

#[test]
fn nat_lowering_is_byte_for_byte_stable() {
    let expected = "\
fmod PROVEKIT-NAT is
  sort Nat .
  op zero : -> Nat .
  op s : Nat -> Nat .
  op plus : Nat Nat -> Nat .
  vars N M : Nat .
  eq plus(zero, N) = N .
  eq plus(s(N), M) = s(plus(N, M)) .
endfm

red in PROVEKIT-NAT : plus(s(zero), s(zero)) .
red in PROVEKIT-NAT : s(s(zero)) .
search in PROVEKIT-NAT : plus(s(zero), s(zero)) =>* s(s(zero)) .
";
    assert_eq!(emit(&nat_obligation()).unwrap(), expected);
}

#[test]
fn ac_builtin_lowering_is_byte_for_byte_stable() {
    let expected = "\
fmod PROVEKIT-AC is
  sort Elt .
  op a : -> Elt .
  op b : -> Elt .
  op c : -> Elt .
  op d : -> Elt .
  op _+_ : Elt Elt -> Elt [assoc comm] .
endfm

red in PROVEKIT-AC : (((a + b) + c) + d) .
red in PROVEKIT-AC : (a + (b + (c + d))) .
search in PROVEKIT-AC : (((a + b) + c) + d) =>* (a + (b + (c + d))) .
";
    assert_eq!(emit(&ac_obligation()).unwrap(), expected);
}

#[test]
fn trait_output_preamble_plus_body_equals_emit() {
    let compiler = MaudeCompiler::new();
    for ir in [nat_obligation(), ac_obligation()] {
        let parts = compiler.compile(&ir, DIALECT).unwrap();
        let combined = format!("{}{}", parts.preamble, parts.body);
        assert_eq!(combined, emit(&ir).unwrap());
    }
}

#[test]
fn capabilities_are_equational_only() {
    let compiler = MaudeCompiler::new();
    let caps = compiler.capabilities();
    assert_eq!(caps.dialects, vec![DIALECT.to_string()]);
    assert_eq!(
        caps.supported_predicates,
        vec!["equational_theory".to_string()]
    );
    assert!(caps
        .supported_sorts
        .contains(&"equational_theory".to_string()));
}
