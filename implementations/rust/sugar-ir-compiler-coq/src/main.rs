// SPDX-License-Identifier: Apache-2.0
//
// Binary entry point for provekit-ir-compiler-coq
//
// Reads IR JSON from stdin, emits Coq to stdout.
// Usage: cat input.json | provekit-ir-coq

use sugar_ir_compiler::IrCompiler;
use sugar_ir_compiler_coq::CoqCompiler;
use serde_json::Value as Json;
use std::io::{self, Read};

fn main() {
    // Read all stdin
    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        eprintln!("Error reading stdin: {}", e);
        std::process::exit(1);
    }

    // Parse JSON
    let ir: Json = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error parsing JSON: {}", e);
            std::process::exit(1);
        }
    };

    // Compile to Coq
    let compiler = CoqCompiler::new();
    let result = match compiler.compile(&ir, "coq") {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error compiling IR to Coq: {}", e);
            std::process::exit(1);
        }
    };

    // Output: preamble + body
    println!("{}", result.preamble);
    println!("{}", result.body);
}
