use crate::cddl_parser::IrSchema;
use crate::rust_gen::emit_function;

pub fn generate(_ir: &IrSchema) -> String {
    let mut out = String::new();
    out.push_str("// SPDX-License-Identifier: Apache-2.0\n// GENERATED SMT-LIB v2.6 compiler\n\n");
    out.push_str("use std::collections::{BTreeMap, BTreeSet};\n");
    out.push_str("use provekit_ir_compiler::{CompiledFormula, FreeVar};\n");
    out.push_str("use provekit_ir_types::*;\n\n");

    emit_function(
        &mut out,
        "pub",
        "emit_term",
        &[("term".into(), "&Term".into())],
        Some("String"),
        &crate::emit_term::build(),
    );
    emit_function(
        &mut out,
        "pub",
        "emit_formula",
        &[("formula".into(), "&Formula".into())],
        Some("String"),
        &crate::emit_formula_smt::build(),
    );
    emit_function(
        &mut out,
        "",
        "emit_sort",
        &[("sort".into(), "&Sort".into())],
        Some("String"),
        &crate::h::emit_sort_body(),
    );
    emit_function(
        &mut out,
        "",
        "emit_const_value",
        &[
            ("value".into(), "&serde_json::Value".into()),
            ("_sort_name".into(), "&str".into()),
        ],
        Some("String"),
        &crate::h::emit_const_value_body(),
    );
    emit_function(
        &mut out,
        "",
        "smt_atomic_name",
        &[("name".into(), "&str".into())],
        Some("&str"),
        &crate::emit_formula_smt::atomic_name_body(),
    );
    emit_function(
        &mut out,
        "pub",
        "collect_free_vars_formula",
        &[
            ("formula".into(), "&Formula".into()),
            ("out".into(), "&mut BTreeMap<String, String>".into()),
            ("bound".into(), "&BTreeSet<String>".into()),
        ],
        None,
        &crate::free_vars::formula_body(),
    );
    emit_function(
        &mut out,
        "pub",
        "collect_free_vars_term",
        &[
            ("term".into(), "&Term".into()),
            ("out".into(), "&mut BTreeMap<String, String>".into()),
            ("bound".into(), "&BTreeSet<String>".into()),
        ],
        None,
        &crate::free_vars::term_body(),
    );
    emit_function(
        &mut out,
        "pub",
        "compile_formula",
        &[("formula".into(), "&Formula".into())],
        Some("CompiledFormula"),
        &crate::compile::smt(),
    );

    out
}
