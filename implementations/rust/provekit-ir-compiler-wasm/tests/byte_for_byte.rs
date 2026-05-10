// SPDX-License-Identifier: Apache-2.0
//
// Byte-for-byte regression check for the WAT compiler. The realizer is
// deterministic, so compiling the same term twice must return the exact
// same bytes, and the fixture captures the public surface for foo.

use serde_json::Value as Json;

use provekit_ir_compiler_wasm::{compile_wat, WasmCompiler};

#[test]
fn foo_fixture_is_byte_stable() {
    let input: Json = serde_json::from_str(include_str!(
        "../../../../menagerie/c11-language-signature/example/foo.term.json"
    ))
    .expect("foo term json parses");
    let expected = include_str!("../fixtures/foo.wat");

    let first = compile_wat(&input).expect("compile foo once");
    let second = compile_wat(&input).expect("compile foo twice");

    assert_eq!(first, second);
    assert_eq!(first, expected);
}

#[test]
fn trait_output_is_byte_stable() {
    let input: Json = serde_json::from_str(include_str!(
        "../../../../menagerie/c11-language-signature/example/foo.term.json"
    ))
    .expect("foo term json parses");
    let compiler = WasmCompiler::new();

    let first = compiler.compile_term(&input).expect("compile foo once");
    let second = compiler.compile_term(&input).expect("compile foo twice");

    assert_eq!(first, second);
}
