// SPDX-License-Identifier: Apache-2.0
//
// Binary entry point for provekit-ir-compiler-x86-64.
//
// Reads ProofIR term JSON from stdin and emits x86-64 SysV assembly.

use provekit_ir_compiler_x86_64::{TermCompiler, X8664Compiler};
use serde_json::Value as Json;
use std::io::{self, Read, Write};

fn main() {
    let mut input = String::new();
    if let Err(error) = io::stdin().read_to_string(&mut input) {
        eprintln!("Error reading stdin: {error}");
        std::process::exit(1);
    }

    let ir: Json = match serde_json::from_str(&input) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("Error parsing JSON: {error}");
            std::process::exit(1);
        }
    };

    let compiler = X8664Compiler::new();
    let asm = match compiler.compile_term_json(&ir) {
        Ok(asm) => asm,
        Err(error) => {
            eprintln!("Error compiling IR to x86-64: {error}");
            std::process::exit(1);
        }
    };

    if let Err(error) = io::stdout().write_all(asm.as_bytes()) {
        eprintln!("Error writing assembly: {error}");
        std::process::exit(1);
    }
}
