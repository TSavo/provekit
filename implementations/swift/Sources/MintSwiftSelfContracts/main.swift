// SPDX-License-Identifier: Apache-2.0
//
// MintSwiftSelfContracts — Swift kit self-contracts attestation minter.
//
// Enumerates the 11 lift-plugin-protocol contracts (C1-C8, split into 11 facets)
// matching the Rust canonical definitions in
// implementations/rust/provekit-self-contracts/src/lift_plugin_protocol.rs.
//
// These contracts encode the rules of
// protocol/specs/2026-04-30-lift-plugin-protocol.md (v1.2.0 normative)
// as machine-enforceable contract declarations using the Swift kit's JCS
// canonical emitter. The contractSetCid is the BLAKE3-512 hash of the
// JCS-canonical bytes of all 11 declarations as an array.
//
// Output (to stdout):
//   catalog CID: <bundle-cid-placeholder>
//   contractSetCid: blake3-512:<hex>
//
// The "catalog CID" line is a placeholder — the swift kit does not yet
// have a full mint pipeline (no signed-CBOR envelope, no .proof bundle).
// Phase 3 will add the binary attestation protocol; for now the
// contractSetCid is the authoritative value and is signed by
// tools/foundation-keygen/sign-self-contracts.
//
// Mirrors the output shape of:
//   implementations/csharp/Provekit.SelfContracts (dotnet run)
//   implementations/typescript/src/bin/mint-ts-self-contracts.test.ts

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

/// Constant "true" sentinel: ctor("true_const", str(""))
private let trueConst = ctor1("true_const", Term.str(""))

// MARK: - C1-C8 contract authoring (11 facets)
//
// Names mirror the Rust slab in lift_plugin_protocol.rs.
// Each contract carries pre/post formulas that assert the rule holds
// for the Swift LSP plugin. The formula shape mirrors the rust contracts.

// C1: initialize protocol_version match (spec lines 64-88).
// pre: request_protocol_version(req) = "provekit-lift/1"
// post: response_confirms_protocol_or_errors_mismatch(req) = true_const
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

// C2a: initialize capabilities — authoring_surfaces is non-empty (spec lines 73-86).
// forall resp. len(authoring_surfaces_of(resp)) >= 1.
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

// C2b: initialize capabilities — ir_version starts with "v" (spec lines 73-86).
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

// C3b: lift request — source_paths is non-empty (spec lines 92-100).
// forall req. len(source_paths_of(req)) >= 1.
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

// MARK: - Mint

let allContracts: [Declaration] = [c1, c2a, c2b, c3a, c3b, c3c, c4, c5, c6, c7, c8]

// contractSetCid: BLAKE3-512 of the JCS-canonical bytes of all 11 declarations.
let jcs = Jcs.encodeDeclarations(allContracts)
let contractSetCid = Blake3.hex(Data(jcs.utf8))

// The full signed-CBOR bundle is not yet implemented for the Swift kit;
// the bundle CID is a placeholder. Phase 3 will replace this with a real
// .proof bundle per protocol/specs/2026-05-02-binary-attestation-protocol.md.
let bundleCidPlaceholder = "swift-kit-bundle:phase-3-deferred"

print("catalog CID: \(bundleCidPlaceholder)")
print("contractSetCid: \(contractSetCid)")
