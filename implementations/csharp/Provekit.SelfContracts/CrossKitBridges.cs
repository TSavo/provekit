// SPDX-License-Identifier: Apache-2.0
//
// Cross-kit conformance bridges for the lift-plugin protocol. C# peer.
//
// Phase 2 of the cross-kit bridge work. Phase 1 landed in PR #84:
// implementations/rust/provekit-self-contracts/src/lift_plugin_protocol.rs
// authored 10 contracts encoding the rules of
// protocol/specs/2026-04-30-lift-plugin-protocol.md (the "lift-plugin-
// protocol spec", v1.2.0 normative). PR #88 re-signed rust's bundle
// attestation. The rust .proof now ships those 10 contracts as
// signed mementos with content-addressed CIDs.
//
// This file does the C#-side counterpart, mirroring PR #89 (python),
// PR #92 (go), and PR #93 (ts):
//
//   1. Mints 10 C# COUNTERPART contracts named csharp_lift_plugin_<rule>.
//      Each counterpart asserts "csharp-kit's lift plugin satisfies
//      rust's <contract name>" as the C# implementation should observe
//      it. Same shape as the rust contracts (kit-defined named ctors
//      with a paired-equality post against `true_const`); the verifier
//      discharges the operational check at the per-callsite layer.
//
//   2. Builds 10 BRIDGE declarations linking each rust source contract
//      (by its memento envelope CID) to its C# counterpart contract.
//      Per protocol/specs/2026-04-30-ir-formal-grammar.md the
//      BridgeDeclaration locked-key-order shape is
//        {kind, name, sourceSymbol, sourceLayer, sourceContractCid,
//         targetContractCid, targetProofCid, targetLayer, notes?}
//      and the verifier uses `sourceContractCid` + `targetContractCid`
//      to look up envelopes in the unified pool.
//
// The rust contract CIDs frozen below are the memento envelope CIDs
// extracted via `cargo run --release -p provekit-self-contracts --bin
// print-lift-plugin-protocol-cids`. They are stable under the rust
// orchestrator's pinned producer ("provekit-self-contracts@1.0"),
// pinned timestamp, and pinned signer seed ([0x42; 32]). Any drift
// (a rust contract body change, a producer-id rename, a timestamp
// bump) will fail the pinned-hash test in the sibling
// CrossKitBridgesTests.cs and signal a phase-2 re-mint event.
//
// `targetContractCid` carries the sentinel `pending-csharp-counterpart:
// <name>` placeholder. Mirrors Go's discipline: the bridge bytes are
// content-addressable independent of the (transient) C# bundle's
// internal CIDs. Phase 3 will wire orchestrator post-mint resolution
// (mirroring rust's lib.rs:368-385 closed-loop bridge resolution).
//
// `targetProofCid` is `deferred:phase-3-proof-bundle`. Phase 3 (binary-
// attestation protocol) will replace the literal with the real
// C# lift plugin binary CID.

using Provekit.Canonicalizer;
using Provekit.IR;
using static Provekit.IR.Predicates;
using static Provekit.IR.Terms;
using static Provekit.IR.Collector;

namespace Provekit.SelfContracts.Invariants;

public static class CrossKitBridges
{
    // ----- Phase-2 frozen literals --------------------------------------

    /// <summary>
    /// Source layer for Phase-2 bridges: the upstream Rust self-contracts kit.
    /// </summary>
    public const string RustKitLayer = "rust-kit";

    /// <summary>Target layer for Phase-2 bridges: this C# kit.</summary>
    public const string CsharpKitLayer = "csharp-kit";

    /// <summary>
    /// Phase-3 placeholder for the C# lift module's .proof bundle CID.
    /// Until the C# lift binary is bundled and signed, this sentinel
    /// keeps the field present and machine-readable without lying about
    /// a real CID. Verifier resolution against this string fails loud
    /// with `BridgeTargetProofCidMismatch`, which is the right signal
    /// that the bridge has not yet been bound to a real binary.
    /// </summary>
    public const string DeferredCsharpLiftBinaryCid = "deferred:phase-3-proof-bundle";

    /// <summary>Notes attached to every Phase-2 bridge declaration.</summary>
    public const string Phase2BridgeNotes =
        "lift-plugin-protocol conformance bridge; phase 2";

    /// <summary>Prefix of the placeholder CID used for `targetContractCid`.</summary>
    public const string PendingTargetContractCidPrefix = "pending-csharp-counterpart:";

    /// <summary>
    /// Rust contract memento CIDs from the lift_plugin_protocol slab in
    /// the rust self-contracts bundle. Extracted via
    /// `cargo run --release -p provekit-self-contracts --bin
    /// print-lift-plugin-protocol-cids`. Pinned here so Phase-2 bridges
    /// can reference them without re-running the rust mint. If a future
    /// rust mint drifts these CIDs, the sibling test will fail (the
    /// bridge bytes change, so the pinned bridge CID changes).
    /// Order matches the declaration order in
    /// implementations/rust/provekit-self-contracts/src/lift_plugin_protocol.rs.
    /// </summary>
    public static readonly IReadOnlyList<KeyValuePair<string, string>> RustContractCids =
        new[]
        {
            new KeyValuePair<string, string>(
                "lift_plugin_initialize_protocol_version_match",
                "blake3-512:95163d00976803c3ef381494a8a940bd862529f7bdfb72aa523bd58359b86d6fce017991658932e3e3dee8b4c60b26066bfa270474b2896c19dd2ec85d4aa47a"),
            new KeyValuePair<string, string>(
                "lift_plugin_initialize_capabilities_authoring_surfaces_nonempty",
                "blake3-512:1898e2518e96628bbe46704f6f6a90cc57572f3b15bb3f4f6a7d8fef28a8c92e31b33b14f21d4011ed7ad11d4ea09c67c1549cbe1c2bf38e53b7e8cfdb656099"),
            new KeyValuePair<string, string>(
                "lift_plugin_initialize_capabilities_ir_version_starts_with_v",
                "blake3-512:08d09e6f677e77f5b501a07a5271cebdadb19c48c52375ae9e6edcb699b6515eacdea2d7966497c3b3aca4054340e7222fe97bbbb8f60e2ee62baaec6ef719f0"),
            new KeyValuePair<string, string>(
                "lift_plugin_lift_request_surface_is_string",
                "blake3-512:bf6ac4f7e481ba1fea26716f9d2e7756c86b1940610e2d9e35a5d6e11faa8993a92cd291f491c4d520e5daf1a54c32aeb492adac5aa8d61d224ca1104adaaf8a"),
            new KeyValuePair<string, string>(
                "lift_plugin_lift_request_source_paths_nonempty",
                "blake3-512:3f2915b063357c28cd2bd8132279e819424999b21a776824d3db9231ca4acb8fdc02ea6e5a8945e55a1d439fda94d07b365d0d160e4ece94b1012fe064ca7c22"),
            new KeyValuePair<string, string>(
                "lift_plugin_lift_request_source_paths_each_nonempty",
                "blake3-512:f57621c2ba995cbd13d9d06c4209ad9ecdb6369d1e90d902b90996275dd40a38804c986b77b9f28bdf7eefc2b0f242284d1612a4149f5abb0451097a72f95822"),
            new KeyValuePair<string, string>(
                "lift_plugin_lift_request_surface_in_capabilities",
                "blake3-512:61c67906e3b2ff0d0a61419436670140009556402b643516c4afb14212c057a080bf6f29a0c4c374fe2eb45f8016ddfc82ed12fae2735c7384a8b56a7597db51"),
            new KeyValuePair<string, string>(
                "lift_plugin_lift_response_kind_in_set",
                "blake3-512:7642bd5eb5262354921513ee6e01bf70dad917f3467464ad904750685e84d0241ef9b0f40b6e0d66dd73e0d5cc1908e4a0a45d45530dda511e1919786034e2a0"),
            new KeyValuePair<string, string>(
                "lift_plugin_lift_response_ir_document_array",
                "blake3-512:692df8b67bc3ad69943f5909779f489bdc8173bbb08fd61585bb1b8bc0a2c20c6891ba7b9a2a4e4e3a6e5a4441b1191f4618924783446cb07277879c885cbc20"),
            new KeyValuePair<string, string>(
                "lift_plugin_diagnostic_field_is_array",
                "blake3-512:ea5dd139fddc9e5ab6cfcb9854de1ce6bbedcccbe7b070c1aef9fbbef3b8579ebf33ff14cdc97013e1f3e1c391964f275a0275b615b8259037b0cb92d0e0dd35"),
        };

    /// <summary>Build the counterpart contract name for a rust contract.</summary>
    public static string CounterpartContractName(string rustContractName)
        => $"csharp_{rustContractName}";

    /// <summary>Build the bridge declaration name for a rust contract.</summary>
    public static string BridgeName(string rustContractName)
        => $"bridge_to_{rustContractName}";

    /// <summary>
    /// Sentinel `targetContractCid` value used during Phase 2. The
    /// orchestrator (Phase 3) will detect the prefix and rewrite the
    /// field to the real C# counterpart memento CID after every
    /// counterpart contract has been minted. Mirrors Go's
    /// `pendingTargetContractCidPlaceholder`.
    /// </summary>
    public static string PendingTargetContractCid(string counterpartName)
        => PendingTargetContractCidPrefix + counterpartName;

    // ----- Counterpart contract slab ------------------------------------

    /// <summary>
    /// Author the 10 C# counterpart contracts. Each asserts "csharp-kit's
    /// lift plugin satisfies rust's <contract name>" using the same
    /// kit-defined named-ctor / paired-equality shape as the rust slab.
    ///
    /// Wired into <c>Program.RegisterAll()</c> so the contracts flow into
    /// the C# self-contracts bundle. The bundle CID will drift after this
    /// PR lands; the C# attestation is re-signed as a follow-up.
    /// </summary>
    public static void Register()
    {
        // -- C1: initialize protocol_version match. ---------------------
        Contract("csharp_lift_plugin_initialize_protocol_version_match",
            pre: Eq(
                Ctor("csharp_request_protocol_version", StrConst("req")),
                StrConst("provekit-lift/1")),
            post: Eq(
                Ctor("csharp_response_confirms_protocol_or_errors_mismatch", StrConst("req")),
                Ctor("true_const", StrConst(""))));

        // -- C2.a: initialize capabilities.authoring_surfaces is nonempty. -
        Contract("csharp_lift_plugin_initialize_capabilities_authoring_surfaces_nonempty",
            post: Eq(
                Ctor("csharp_authoring_surfaces_nonempty", StrConst("resp")),
                Ctor("true_const", StrConst(""))));

        // -- C2.b: initialize capabilities.ir_version starts with "v". --
        Contract("csharp_lift_plugin_initialize_capabilities_ir_version_starts_with_v",
            post: Eq(
                Ctor("csharp_ir_version_starts_with_v", StrConst("resp")),
                Ctor("true_const", StrConst(""))));

        // -- C3.a: lift request `surface` field is a string. ------------
        Contract("csharp_lift_plugin_lift_request_surface_is_string",
            post: Eq(
                Ctor("csharp_is_string", Ctor("csharp_surface_of", StrConst("req"))),
                Ctor("true_const", StrConst(""))));

        // -- C3.b: lift request `source_paths` is nonempty. -------------
        Contract("csharp_lift_plugin_lift_request_source_paths_nonempty",
            post: Eq(
                Ctor("csharp_source_paths_nonempty", StrConst("req")),
                Ctor("true_const", StrConst(""))));

        // -- C3.c: every `source_paths` element is nonempty. ------------
        Contract("csharp_lift_plugin_lift_request_source_paths_each_nonempty",
            post: Eq(
                Ctor("csharp_source_paths_each_nonempty", StrConst("req")),
                Ctor("true_const", StrConst(""))));

        // -- C4: lift surface in capabilities (init handshake gate). ----
        Contract("csharp_lift_plugin_lift_request_surface_in_capabilities",
            post: Eq(
                Ctor("csharp_surface_in_capabilities", StrConst("req"), StrConst("caps")),
                Ctor("true_const", StrConst(""))));

        // -- C5: response kind in {ir-document, signed-mementos,
        //        proof-envelope}. -----------------------------------------
        Contract("csharp_lift_plugin_lift_response_kind_in_set",
            post: Eq(
                Ctor("csharp_response_kind_in_allowed_set", StrConst("resp")),
                Ctor("true_const", StrConst(""))));

        // -- C6: when kind == "ir-document", `ir` field is an array. ----
        Contract("csharp_lift_plugin_lift_response_ir_document_array",
            post: Eq(
                Ctor("csharp_ir_field_is_array_when_kind_ir_document", StrConst("resp")),
                Ctor("true_const", StrConst(""))));

        // -- C7: `diagnostics` field, when present, is an array. --------
        Contract("csharp_lift_plugin_diagnostic_field_is_array",
            post: Eq(
                Ctor("csharp_diagnostics_field_is_array_or_absent", StrConst("resp")),
                Ctor("true_const", StrConst(""))));
    }

    // ----- Bridge construction ------------------------------------------

    /// <summary>
    /// Construct the 10 phase-2 BridgeDeclarations linking each rust
    /// source contract (by memento envelope CID) to its C# counterpart
    /// contract (by `pending-csharp-counterpart:&lt;name&gt;` placeholder).
    /// Returned in stable rust-declaration order.
    /// </summary>
    public static IReadOnlyList<BridgeDeclaration> BuildBridges()
    {
        var bridges = new List<BridgeDeclaration>(RustContractCids.Count);
        foreach (var pair in RustContractCids)
        {
            var rustName = pair.Key;
            var rustCid = pair.Value;
            var counterpart = CounterpartContractName(rustName);
            bridges.Add(new BridgeDeclaration(
                Name: BridgeName(rustName),
                SourceSymbol: rustName,
                SourceLayer: RustKitLayer,
                SourceContractCid: rustCid,
                TargetContractCid: PendingTargetContractCid(counterpart),
                TargetProofCid: DeferredCsharpLiftBinaryCid,
                TargetLayer: CsharpKitLayer,
                Notes: Phase2BridgeNotes));
        }
        return bridges;
    }

    /// <summary>
    /// JCS-encode the bridges array (a JSON array of BridgeDeclaration
    /// objects) and return the BLAKE3-512 self-identifying CID. This is
    /// the value pinned by the test; drift in any rust CID, name, layer,
    /// notes literal, placeholder shape, or declaration order changes
    /// it.
    /// </summary>
    public static string ComputeBridgesArrayCid()
    {
        var bridges = BuildBridges();
        var arr = Value.Array(bridges.Select(Serialize.BridgeDeclarationToValue).ToArray());
        var bytes = Jcs.EncodeUtf8(arr);
        return Hash.Blake3_512(bytes);
    }
}
