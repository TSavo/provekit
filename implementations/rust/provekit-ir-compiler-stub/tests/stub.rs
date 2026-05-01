// SPDX-License-Identifier: Apache-2.0

use serde_json::json;

use provekit_ir_compiler::{CompileError, IrCompiler, PROTOCOL_VERSION};
use provekit_ir_compiler_stub::{StubCompiler, DIALECT};

#[test]
fn stub_returns_canned_preamble_and_echo_body() {
    let s = StubCompiler::new();
    let ir = json!({"kind": "atomic", "name": "=", "args": []});
    let out = s.compile(&ir, DIALECT).unwrap();
    assert_eq!(out.preamble, "; stub preamble\n");
    assert!(out.body.starts_with("STUB:"));
    assert_eq!(out.free_vars.len(), 1);
    assert_eq!(out.free_vars[0].name, "stub_var");
}

#[test]
fn stub_rejects_other_dialects() {
    let s = StubCompiler::new();
    let r = s.compile(&json!({}), "smt-lib-v2.6");
    assert!(matches!(r, Err(CompileError::UnsupportedDialect(_))));
}

#[test]
fn stub_capabilities_advertise_protocol_version() {
    let s = StubCompiler::new();
    let c = s.capabilities();
    assert_eq!(c.protocol_version, PROTOCOL_VERSION);
    assert_eq!(c.dialects, vec![DIALECT.to_string()]);
}
