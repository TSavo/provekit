// SPDX-License-Identifier: Apache-2.0

use std::io::{self, Read};

use provekit_ir_compiler_jvm_bytecode::compile_jasmin;
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

    match compile_jasmin(&ir) {
        Ok(jasmin) => print!("{jasmin}"),
        Err(e) => {
            eprintln!("Error compiling IR to JVM bytecode: {e}");
            std::process::exit(1);
        }
    }
}
