// SPDX-License-Identifier: Apache-2.0
//
// Binary entry point for provekit-ir-compiler-wasm.
//
// Reads ProofIR term JSON from stdin and emits WebAssembly WAT to stdout.
// Usage: cat foo.term.json | provekit-ir-wasm

use std::io::{self, Read};

use provekit_ir_compiler_wasm::compile_wat;
use serde_json::Value as Json;

fn main() {
    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        eprintln!("Error reading stdin: {e}");
        std::process::exit(1);
    }

    let ir: Json = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error parsing JSON: {e}");
            std::process::exit(1);
        }
    };

    match compile_wat(&ir) {
        Ok(wat) => print!("{wat}"),
        Err(e) => {
            eprintln!("Error compiling IR to WAT: {e}");
            std::process::exit(1);
        }
    }
}
