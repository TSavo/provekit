// SPDX-License-Identifier: Apache-2.0
//
//! The protocol grammar `protocol/provekit-ir.cddl` must parse and
//! validate under the `cddl` crate (the same parse the codegen does, with
//! the strict flag). This guards the grammar file against syntax errors —
//! e.g. when a new node kind like the wp-rule schema's `substitute` /
//! `apply` (spec 2026-05-13-wp-as-formula.md §2.3) is added.

#[test]
fn protocol_ir_cddl_parses_and_validates() {
    // Tests run with CWD = the crate dir (`implementations/rust/provekit-ir-codegen`).
    let path = "../../../protocol/provekit-ir.cddl";
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {path}: {e}"));
    if let Err(e) = cddl::cddl_from_str(&text, true) {
        panic!("protocol/provekit-ir.cddl failed CDDL validation:\n{e}");
    }
}
