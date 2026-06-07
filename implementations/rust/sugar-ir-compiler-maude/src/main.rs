// SPDX-License-Identifier: Apache-2.0
//
// Binary entry point for provekit-ir-maude.

use std::io::{self, Read};

use sugar_ir_compiler::IrCompiler;
use sugar_ir_compiler_maude::{MaudeCompiler, DIALECT};
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

    let compiler = MaudeCompiler::new();
    let result = match compiler.compile(&ir, DIALECT) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error compiling IR to Maude: {e}");
            std::process::exit(1);
        }
    };

    print!("{}{}", result.preamble, result.body);
}
