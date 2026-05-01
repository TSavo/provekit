// SPDX-License-Identifier: Apache-2.0
//
// Test stub. Returns canned compiled output. Used in tests and CI
// without depending on a real solver dialect.

use serde_json::Value as Json;

use provekit_ir_compiler::{
    Capabilities, CompileError, CompiledFormula, FreeVar, IrCompiler, PROTOCOL_VERSION,
};

pub const DIALECT: &str = "stub-dialect";

/// Stub compiler. Echoes the IR-JSON into the body field; produces a
/// fixed preamble and a single fake free variable.
#[derive(Default)]
pub struct StubCompiler;

impl StubCompiler {
    pub fn new() -> Self {
        Self
    }
}

impl IrCompiler for StubCompiler {
    fn compile(&self, ir: &Json, dialect: &str) -> Result<CompiledFormula, CompileError> {
        if dialect != DIALECT {
            return Err(CompileError::UnsupportedDialect(dialect.to_string()));
        }
        let body = format!("STUB:{ir}\n");
        Ok(CompiledFormula {
            preamble: "; stub preamble\n".to_string(),
            body,
            free_vars: vec![FreeVar {
                name: "stub_var".to_string(),
                sort: "Stub".to_string(),
            }],
        })
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            name: "stub".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: PROTOCOL_VERSION.to_string(),
            dialects: vec![DIALECT.to_string()],
            supported_sorts: vec!["Stub".to_string()],
            supported_predicates: vec!["stub_pred".to_string()],
        }
    }
}
