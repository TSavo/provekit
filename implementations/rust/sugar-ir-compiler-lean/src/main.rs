// SPDX-License-Identifier: Apache-2.0
//
// Binary entry point for sugar-ir-compiler-lean.

use serde_json::Value as Json;
use std::io::{self, Read};
use sugar_ir_compiler::IrCompiler;
use sugar_ir_compiler_lean::{LeanCompiler, DIALECT};

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

    let compiler = LeanCompiler::new();
    let result = match compiler.compile(&ir, DIALECT) {
        Ok(result) => result,
        Err(error) => {
            eprintln!("Error compiling IR to Lean: {error}");
            std::process::exit(1);
        }
    };

    print!("{}{}", result.preamble, result.body);
}
