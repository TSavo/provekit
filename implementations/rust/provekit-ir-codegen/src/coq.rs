use crate::cddl_parser::IrSchema;
use crate::rust_gen::emit_function;

pub fn generate(_ir: &IrSchema) -> String {
    let mut out = String::new();
    out.push_str("// SPDX-License-Identifier: Apache-2.0\n// GENERATED Coq compiler\n\n");
    out.push_str("use std::collections::{BTreeMap, BTreeSet};\n");
    out.push_str("use provekit_ir_compiler::FreeVar;\n");
    out.push_str("use provekit_ir_types::*;\n\n");

    emit_function(
        &mut out,
        "pub",
        "emit_term",
        &[("term".into(), "&Term".into())],
        Some("String"),
        &crate::emit_term_coq::build(),
    );
    emit_function(
        &mut out,
        "pub",
        "emit_formula",
        &[("formula".into(), "&Formula".into())],
        Some("String"),
        &crate::emit_formula_coq::build(),
    );
    emit_function(
        &mut out,
        "",
        "emit_sort",
        &[("sort".into(), "&Sort".into())],
        Some("String"),
        &crate::h::emit_sort_body_coq(),
    );
    emit_function(
        &mut out,
        "",
        "sort_to_coq",
        &[("sort".into(), "&Sort".into())],
        Some("String"),
        &crate::h::emit_sort_body_coq(),
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
        "pub",
        "compile_formula",
        &[("formula".into(), "&Formula".into())],
        Some("(String, String, Vec<FreeVar>)"),
        &crate::compile::coq(),
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

    out
}
