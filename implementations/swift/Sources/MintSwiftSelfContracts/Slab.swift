// SPDX-License-Identifier: Apache-2.0
//
// Slab.swift — the swift kit's self-contracts slab.
//
// Authors the 11 lift-plugin-protocol contracts (C1-C8, split into 11 facets)
// matching the Rust canonical definitions in
// implementations/rust/provekit-self-contracts/src/lift_plugin_protocol.rs.
//
// These contracts encode the rules of
// protocol/specs/2026-04-30-lift-plugin-protocol.md (v1.2.0 normative)
// as machine-enforceable contract declarations using the Swift kit's JCS
// canonical emitter. The contractSetCid is the BLAKE3-512 of the JCS-canonical
// bytes of the sorted contractContent CIDs (one per declaration), per
// `protocol/specs/2026-05-03-contract-set-extension.md` §1.
//
// Refactored from main.swift into a function so the `--rpc` mode in RPC.swift
// can walk the same slab without duplicating contract authorship. Issue #211.

import Foundation
import Provekit

// MARK: - Term constructor helpers (mirror Rust's ctor1/ctor2 helpers)

/// Unary ctor: ctor(name, arg)
private func ctor1(_ name: String, _ arg: Term) -> Term {
    .ctor(name: name, args: [arg])
}

/// Binary ctor: ctor(name, a, b)
private func ctor2(_ name: String, _ a: Term, _ b: Term) -> Term {
    .ctor(name: name, args: [a, b])
}

// MARK: - Slab walker

/// Walk the swift-self-contracts slab and return all 11 contract declarations
/// in the canonical order. Pure function: same input (none) -> same output;
/// no side effects, suitable for both the print-mode entry point and the
/// `--rpc` lift handler.
func swiftSelfContracts() -> [Declaration] {

    // Constant "true" sentinel: ctor("true_const", str("")). Defined inside
    // the function so it doesn't trip Swift 6 strict concurrency on the
    // global Term value (Term is not Sendable; that's fine for a build-time
    // function but blocks file-scope lets).
    let trueConst = ctor1("true_const", Term.str(""))

    // C1: initialize protocol_version match (spec lines 64-88).
    let c1 = Declaration.contract(
        name: "lift_plugin_initialize_protocol_version_match",
        outBinding: "out",
        pre: Formula.eq(
            ctor1("request_protocol_version", Term.str("req")),
            Term.str("provekit-lift/1")
        ),
        post: Formula.eq(
            ctor1("response_confirms_protocol_or_errors_mismatch", Term.str("req")),
            trueConst
        ),
        inv: nil
    )

    // C2a: initialize capabilities — authoring_surfaces is non-empty.
    let c2a = Declaration.contract(
        name: "lift_plugin_initialize_capabilities_authoring_surfaces_nonempty",
        outBinding: "out",
        pre: nil,
        post: nil,
        inv: Formula.forall(
            name: "resp",
            sort: .string,
            body: Formula.gte(
                ctor1("len", ctor1("authoring_surfaces_of", Term.var(name: "resp"))),
                Term.num(1)
            )
        )
    )

    // C2b: initialize capabilities — ir_version starts with "v".
    let c2b = Declaration.contract(
        name: "lift_plugin_initialize_capabilities_ir_version_starts_with_v",
        outBinding: "out",
        pre: nil,
        post: Formula.eq(
            ctor1("ir_version_starts_with_v", Term.str("resp")),
            trueConst
        ),
        inv: nil
    )

    // C3a: lift request — surface is a string (spec lines 92-100).
    let c3a = Declaration.contract(
        name: "lift_plugin_lift_request_surface_is_string",
        outBinding: "out",
        pre: nil,
        post: Formula.eq(
            ctor1("is_string", ctor1("surface_of", Term.str("req"))),
            trueConst
        ),
        inv: nil
    )

    // C3b: lift request — source_paths is non-empty.
    let c3b = Declaration.contract(
        name: "lift_plugin_lift_request_source_paths_nonempty",
        outBinding: "out",
        pre: nil,
        post: nil,
        inv: Formula.forall(
            name: "req",
            sort: .string,
            body: Formula.gte(
                ctor1("len", ctor1("source_paths_of", Term.var(name: "req"))),
                Term.num(1)
            )
        )
    )

    // C3c: lift request — each source path is a non-empty string.
    let c3c = Declaration.contract(
        name: "lift_plugin_lift_request_source_paths_each_nonempty",
        outBinding: "out",
        pre: nil,
        post: Formula.eq(
            ctor1("all_source_paths_nonempty", Term.str("req")),
            trueConst
        ),
        inv: nil
    )

    // C4: lift surface in capabilities (spec lines 160-166).
    let c4 = Declaration.contract(
        name: "lift_plugin_lift_request_surface_in_capabilities",
        outBinding: "out",
        pre: nil,
        post: Formula.eq(
            ctor2("surface_in_capabilities", Term.str("req"), Term.str("caps")),
            trueConst
        ),
        inv: nil
    )

    // C5: lift response kind in {ir-document, signed-mementos, proof-envelope}.
    let c5 = Declaration.contract(
        name: "lift_plugin_lift_response_kind_in_set",
        outBinding: "out",
        pre: nil,
        post: Formula.eq(
            ctor1("response_kind_in_allowed_set", Term.str("resp")),
            trueConst
        ),
        inv: nil
    )

    // C6: when kind == "ir-document", response.ir is an array (spec lines 104-117).
    let c6 = Declaration.contract(
        name: "lift_plugin_lift_response_ir_document_array",
        outBinding: "out",
        pre: nil,
        post: Formula.eq(
            ctor1("ir_field_is_array_when_kind_ir_document", Term.str("resp")),
            trueConst
        ),
        inv: nil
    )

    // C7: diagnostics field, when present, is an array (spec line 208).
    let c7 = Declaration.contract(
        name: "lift_plugin_diagnostic_field_is_array",
        outBinding: "out",
        pre: nil,
        post: Formula.eq(
            ctor1("diagnostics_field_is_array_or_absent", Term.str("resp")),
            trueConst
        ),
        inv: nil
    )

    // C8: lifter emits call-edge stream alongside contracts (spec #114 §1 R1).
    let c8 = Declaration.contract(
        name: "lift_emits_call_edge_stream",
        outBinding: "out",
        pre: nil,
        post: Formula.eq(
            ctor1("call_edge_stream_present_or_unit_empty", Term.str("resp")),
            trueConst
        ),
        inv: nil
    )

    return [c1, c2a, c2b, c3a, c3b, c3c, c4, c5, c6, c7, c8]
}
