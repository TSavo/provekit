// SPDX-License-Identifier: Apache-2.0
//
// Byte-for-byte regression checks for the Jasmin JVM realizer.

use provekit_ir_compiler_jvm_bytecode::{compile_jasmin, JvmBytecodeCompiler, TermCompiler};
use serde_json::Value as Json;

#[test]
fn foo_fixture_is_byte_stable() {
    let input: Json =
        serde_json::from_str(include_str!("fixtures/foo.term.json")).expect("foo term json parses");
    let expected = include_str!("fixtures/foo.expected.j");

    let first = compile_jasmin(&input).expect("compile foo once");
    let second = compile_jasmin(&input).expect("compile foo twice");

    assert_eq!(first, second);
    assert_eq!(first, expected);
}

#[test]
fn trait_output_is_byte_stable() {
    let input: Json =
        serde_json::from_str(include_str!("fixtures/foo.term.json")).expect("foo term json parses");
    let compiler = JvmBytecodeCompiler::new();

    let first = compiler
        .compile_term_json(&input)
        .expect("compile foo once");
    let second = compiler
        .compile_term_json(&input)
        .expect("compile foo twice");

    assert_eq!(first, second);
}
