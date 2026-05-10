// SPDX-License-Identifier: Apache-2.0
//
// Binary entry point for provekit-ir-compiler-c.
//
// Reads ProofIR term JSON from stdin and emits C11 source to stdout.
// Usage: cat foo.term.json | provekit-ir-c

use std::io::{self, Read};

use provekit_ir_compiler_c::compile_c;
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

    match compile_c(&ir) {
        Ok(c_source) => print!("{c_source}"),
        Err(e) => {
            eprintln!("Error compiling IR to C: {e}");
            std::process::exit(1);
        }
    }
}
