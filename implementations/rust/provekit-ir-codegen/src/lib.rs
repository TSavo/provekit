//! provekit-ir-codegen — Generate Rust types and compilers from CDDL grammar.
//!
//! Usage:
//!     cargo run -p provekit-ir-codegen
//!
//! Reads `protocol/provekit-ir.cddl` and emits directly into consumer crates:
//!     provekit-ir-types/src/lib.rs        (serde types)
//!     provekit-ir-compiler-smt-lib/src/generated.rs
//!     provekit-ir-compiler-coq/src/generated.rs

pub mod cddl_parser;
pub mod rust_gen;
pub mod h;
pub mod emit_term;
pub mod emit_term_coq;
pub mod emit_formula_smt;
pub mod emit_formula_coq;
pub mod free_vars;
pub mod compile;
pub mod smt;
pub mod coq;

use std::fs;

pub fn generate_all(cddl_path: &str) -> Result<(), String> {
    let cddl_text = fs::read_to_string(cddl_path)
        .map_err(|e| format!("Failed to read CDDL: {}", e))?;

    let cddl = cddl::cddl_from_str(&cddl_text, true)
        .map_err(|e| format!("Failed to parse CDDL: {:?}", e))?;

    let ir = cddl_parser::extract_ir(&cddl);

    // 1. Types crate (direct lib.rs — this crate IS the generated types)
    let types_rs = rust_gen::emit_module(&ir, rust_gen::ModuleKind::Types);
    fs::write("provekit-ir-types/src/lib.rs", types_rs)
        .map_err(|e| format!("Failed to write provekit-ir-types/src/lib.rs: {}", e))?;

    // 2. SMT compiler
    let smt_rs = smt::generate(&ir);
    fs::write("provekit-ir-compiler-smt-lib/src/generated.rs", smt_rs)
        .map_err(|e| format!("Failed to write smt generated.rs: {}", e))?;

    // 3. Coq compiler
    let coq_rs = coq::generate(&ir);
    fs::write("provekit-ir-compiler-coq/src/generated.rs", coq_rs)
        .map_err(|e| format!("Failed to write coq generated.rs: {}", e))?;

    Ok(())
}
