package slabs

// Cross-kit conformance bridges for the lift-plugin protocol.
//
// Phase 2 of the cross-kit bridge work. Phase 1 landed in PR #84:
// implementations/rust/provekit-self-contracts/src/lift_plugin_protocol.rs
// authored 10 contracts encoding the rules of
// protocol/specs/2026-04-30-lift-plugin-protocol.md (the "lift-plugin-
// protocol spec", v1.2.0 normative). PR #88 re-signed rust's bundle
// attestation. The rust .proof now ships those 10 contracts as
// signed mementos with content-addressed CIDs.
//
// This file does the go-side counterpart:
//
//   1. Mints 11 GO COUNTERPART contracts named go_lift_plugin_<rule>.
//      Each counterpart asserts "go-kit's lift plugin satisfies rust's
//      <contract name>" as the go implementation should observe it.
//      Same shape as the rust contracts (kit-defined named ctors with
//      a paired-equality post against `true_const`); the verifier
//      discharges the operational check at the per-callsite layer.
//
//   2. Mints 11 BRIDGE declarations linking each rust source contract
//      (by its memento CID extracted from rust's .proof bundle) to its
//      go counterpart contract. Per protocol/specs/2026-04-30-ir-formal-
//      grammar.md the BridgeDeclaration locked-key-order shape is
//        {kind, name, sourceSymbol, sourceLayer, sourceContractCid,
//         targetContractCid, targetProofCid, targetLayer, notes?}
//      and the verifier uses `sourceContractCid` + `targetContractCid`
//      to look up envelopes in the unified pool, with `targetProofCid`
//      providing the forward-pin gate (BridgeDeclaration.Consequent-
//      BundlePinned, normative per PR #10).
//
// The rust contract CIDs in `rustContractCID` are the memento envelope
// CIDs of rust's signed contract envelopes, extracted from the rust
// .proof produced by `cargo run --release -p provekit-self-contracts
// --bin mint-self-contracts`. They are stable under the rust
// orchestrator's pinned producer ("provekit-self-contracts@1.0"),
// pinned timestamp ("2026-04-30T18:00:00.000Z"), and pinned signer seed
// ([0x42; 32]). Any drift (a rust contract body change, a producer-id
// rename, a timestamp bump) will fail the pinned-hash test in
// lift_plugin_protocol_test.go and signal a phase-2 re-mint event.
//
// The go counterpart contracts mint at run time inside the go
// orchestrator (see cmd/mint-go-self-contracts/main.go), producing
// their own memento CIDs from the go bundle. The bridge declarations
// reference those targets by the counterpart contract NAME during
// authoring; the orchestrator resolves name -> CID at mint time after
// every counterpart contract has been minted (mirrors rust's
// lib.rs:368-385 closed-loop bridge resolution). Until that resolution
// pass runs, BridgeDeclaration.TargetContractCid is the placeholder
// returned by `pendingTargetContractCidPlaceholder`; the orchestrator
// rewrites it before bundling.
//
// targetProofCid is intentionally `deferred:phase-3-proof-bundle`
// because computing the go lift plugin's binary CID is phase-3 work
// (binary attestation per protocol/specs/2026-05-02-binary-attestation
// -protocol.md). Phase 3 will replace the literal with the real CID.

import "github.com/tsavo/provekit/go/provekit-ir-symbolic/ir"

// rustContractCID maps each lift-plugin-protocol rust contract name to
// the memento envelope CID found in the rust self-contracts .proof
// bundle. Extracted via `cargo run -p provekit-self-contracts --bin
// mint-self-contracts /tmp/<dir>` and walking the bundle's `members`
// map for the 11 envelopes whose evidence.body.contractName has the
// `lift_plugin_` prefix (or `lift_emits_` for C8).
//
// Rust source: implementations/rust/provekit-self-contracts/src/
//
//	lift_plugin_protocol.rs (PR #84)
//
// Rust bundle CID at extraction time:
//
//	blake3-512:a6dcf733721f902c77c19a2e818e7638e37c0f9e6254ac607a39f6
//	           8584aba2c9442b204fe536f25713988e271e684a8585f10d991fefa
//	           08df7e99a8a3df7f60e
//
// Frozen here as static strings: rust's bundle CID and member CIDs are
// load-bearing; if they drift, the pinned-hash test in
// lift_plugin_protocol_test.go fails and signals that phase 2 must be
// re-minted.
var rustContractCID = map[string]string{
	"lift_plugin_initialize_protocol_version_match":                   "blake3-512:95163d00976803c3ef381494a8a940bd862529f7bdfb72aa523bd58359b86d6fce017991658932e3e3dee8b4c60b26066bfa270474b2896c19dd2ec85d4aa47a",
	"lift_plugin_initialize_capabilities_authoring_surfaces_nonempty": "blake3-512:1898e2518e96628bbe46704f6f6a90cc57572f3b15bb3f4f6a7d8fef28a8c92e31b33b14f21d4011ed7ad11d4ea09c67c1549cbe1c2bf38e53b7e8cfdb656099",
	"lift_plugin_initialize_capabilities_ir_version_starts_with_v":    "blake3-512:08d09e6f677e77f5b501a07a5271cebdadb19c48c52375ae9e6edcb699b6515eacdea2d7966497c3b3aca4054340e7222fe97bbbb8f60e2ee62baaec6ef719f0",
	"lift_plugin_lift_request_surface_is_string":                      "blake3-512:bf6ac4f7e481ba1fea26716f9d2e7756c86b1940610e2d9e35a5d6e11faa8993a92cd291f491c4d520e5daf1a54c32aeb492adac5aa8d61d224ca1104adaaf8a",
	"lift_plugin_lift_request_source_paths_nonempty":                  "blake3-512:3f2915b063357c28cd2bd8132279e819424999b21a776824d3db9231ca4acb8fdc02ea6e5a8945e55a1d439fda94d07b365d0d160e4ece94b1012fe064ca7c22",
	"lift_plugin_lift_request_source_paths_each_nonempty":             "blake3-512:f57621c2ba995cbd13d9d06c4209ad9ecdb6369d1e90d902b90996275dd40a38804c986b77b9f28bdf7eefc2b0f242284d1612a4149f5abb0451097a72f95822",
	"lift_plugin_lift_request_surface_in_capabilities":                "blake3-512:61c67906e3b2ff0d0a61419436670140009556402b643516c4afb14212c057a080bf6f29a0c4c374fe2eb45f8016ddfc82ed12fae2735c7384a8b56a7597db51",
	"lift_plugin_lift_response_kind_in_set":                           "blake3-512:7642bd5eb5262354921513ee6e01bf70dad917f3467464ad904750685e84d0241ef9b0f40b6e0d66dd73e0d5cc1908e4a0a45d45530dda511e1919786034e2a0",
	"lift_plugin_lift_response_ir_document_array":                     "blake3-512:692df8b67bc3ad69943f5909779f489bdc8173bbb08fd61585bb1b8bc0a2c20c6891ba7b9a2a4e4e3a6e5a4441b1191f4618924783446cb07277879c885cbc20",
	"lift_plugin_diagnostic_field_is_array":                           "blake3-512:ea5dd139fddc9e5ab6cfcb9854de1ce6bbedcccbe7b070c1aef9fbbef3b8579ebf33ff14cdc97013e1f3e1c391964f275a0275b615b8259037b0cb92d0e0dd35",
	// C8: lift_emits_call_edge_stream (spec #114 §1 R1). Added in rust
	// commit e64488d. CID extracted from rust bundle
	// blake3-512:60df6322388ff7d9ccd1b9ee9d6457fdfe89a51b3d2d73a34daa0131
	//            f65c80543331832eb88920fe514ebb799bc655808f152d36274542ac
	//            9879c862a33f3a92 (the 11-contract bundle).
	// Note: the rust contract name lacks the lift_plugin_ prefix by
	// design; the bridge name uses lift_plugin_ to satisfy the Go
	// conformance test prefix invariant (bridge_to_lift_plugin_*).
	"lift_emits_call_edge_stream": "blake3-512:2d5c9e7071972ecd6004f9dad28a112739538b6e71d187e4f7ad4db6ed770d0b76c501eb7eaed0b3b57e50815be1c27f63ce7106d2b825d243040f387b188c91",
}

// LiftPluginRustContractCID returns the rust memento envelope CID for
// the named lift-plugin-protocol contract. Empty string if the name is
// not one of the 11 rust contracts. Exported for the orchestrator's
// post-mint resolution pass and for the pinned-hash test.
func LiftPluginRustContractCID(rustName string) string {
	return rustContractCID[rustName]
}

// LiftPluginGoTargetProofCIDPlaceholder is the go-kit lift plugin's
// `targetProofCid` value during phase 2. Phase 3 (binary-attestation
// protocol; see protocol/specs/2026-05-02-binary-attestation-protocol.md)
// will replace this with the real binary CID.
//
// The verifier accepts an empty string as the back-compat path and
// flags it with a stderr warning ("ConsequentBundlePinned not
// enforced"). Using a sentinel string instead means the field is
// PRESENT and lookup-distinguishable: any verifier that tries to
// resolve `deferred:phase-3-proof-bundle` will fail loud with
// `BridgeTargetProofCidMismatch`, exactly the right signal that the
// bridge has not yet been bound to a real binary.
const LiftPluginGoTargetProofCIDPlaceholder = "deferred:phase-3-proof-bundle"

// liftPluginBridgePairs lists the 11 (rust contract name, go counterpart
// contract name, bridge name) triples in stable order. The order is
// declaration order of the rust contracts in
// implementations/rust/provekit-self-contracts/src/lift_plugin_protocol.rs.
//
// Bridge naming: `bridge_to_<rust_contract_name>` per the phase-2 PR
// description, with the exception that C8 (lift_emits_call_edge_stream)
// uses bridge_to_lift_plugin_emits_call_edge_stream to satisfy the
// bridge_to_lift_plugin_* prefix invariant enforced by the test suite.
//
// Counterpart naming: `go_lift_plugin_<rule>` per the phase-2 PR
// description.
type liftPluginBridgePair struct {
	rustName      string // source of the conformance claim
	goCounterpart string // go-kit contract that claims conformance
	bridgeName    string // bridge_to_<rust_contract_name>
}

func liftPluginBridgePairs() []liftPluginBridgePair {
	return []liftPluginBridgePair{
		{
			rustName:      "lift_plugin_initialize_protocol_version_match",
			goCounterpart: "go_lift_plugin_initialize_protocol_version_match",
			bridgeName:    "bridge_to_lift_plugin_initialize_protocol_version_match",
		},
		{
			rustName:      "lift_plugin_initialize_capabilities_authoring_surfaces_nonempty",
			goCounterpart: "go_lift_plugin_initialize_capabilities_authoring_surfaces_nonempty",
			bridgeName:    "bridge_to_lift_plugin_initialize_capabilities_authoring_surfaces_nonempty",
		},
		{
			rustName:      "lift_plugin_initialize_capabilities_ir_version_starts_with_v",
			goCounterpart: "go_lift_plugin_initialize_capabilities_ir_version_starts_with_v",
			bridgeName:    "bridge_to_lift_plugin_initialize_capabilities_ir_version_starts_with_v",
		},
		{
			rustName:      "lift_plugin_lift_request_surface_is_string",
			goCounterpart: "go_lift_plugin_lift_request_surface_is_string",
			bridgeName:    "bridge_to_lift_plugin_lift_request_surface_is_string",
		},
		{
			rustName:      "lift_plugin_lift_request_source_paths_nonempty",
			goCounterpart: "go_lift_plugin_lift_request_source_paths_nonempty",
			bridgeName:    "bridge_to_lift_plugin_lift_request_source_paths_nonempty",
		},
		{
			rustName:      "lift_plugin_lift_request_source_paths_each_nonempty",
			goCounterpart: "go_lift_plugin_lift_request_source_paths_each_nonempty",
			bridgeName:    "bridge_to_lift_plugin_lift_request_source_paths_each_nonempty",
		},
		{
			rustName:      "lift_plugin_lift_request_surface_in_capabilities",
			goCounterpart: "go_lift_plugin_lift_request_surface_in_capabilities",
			bridgeName:    "bridge_to_lift_plugin_lift_request_surface_in_capabilities",
		},
		{
			rustName:      "lift_plugin_lift_response_kind_in_set",
			goCounterpart: "go_lift_plugin_lift_response_kind_in_set",
			bridgeName:    "bridge_to_lift_plugin_lift_response_kind_in_set",
		},
		{
			rustName:      "lift_plugin_lift_response_ir_document_array",
			goCounterpart: "go_lift_plugin_lift_response_ir_document_array",
			bridgeName:    "bridge_to_lift_plugin_lift_response_ir_document_array",
		},
		{
			rustName:      "lift_plugin_diagnostic_field_is_array",
			goCounterpart: "go_lift_plugin_diagnostic_field_is_array",
			bridgeName:    "bridge_to_lift_plugin_diagnostic_field_is_array",
		},
		// C8: the rust contract name lacks the lift_plugin_ prefix; the
		// bridge name uses lift_plugin_ to satisfy the prefix invariant.
		{
			rustName:      "lift_emits_call_edge_stream",
			goCounterpart: "go_lift_plugin_emits_call_edge_stream",
			bridgeName:    "bridge_to_lift_plugin_emits_call_edge_stream",
		},
	}
}

// LiftPluginBridgePairNames returns the 11 bridge names in stable
// declaration order. Exported for tests.
func LiftPluginBridgePairNames() []string {
	pairs := liftPluginBridgePairs()
	out := make([]string, len(pairs))
	for i, p := range pairs {
		out[i] = p.bridgeName
	}
	return out
}

// LiftPluginGoCounterpartNames returns the 11 go-counterpart contract
// names in stable declaration order. Exported for tests + the
// orchestrator's post-mint bridge resolution.
func LiftPluginGoCounterpartNames() []string {
	pairs := liftPluginBridgePairs()
	out := make([]string, len(pairs))
	for i, p := range pairs {
		out[i] = p.goCounterpart
	}
	return out
}

// InvariantsLiftPluginProtocolContracts authors the 11 go counterpart
// contracts that assert "go-kit's lift plugin satisfies rust's
// <contract name>". Each counterpart mirrors the rust contract's shape:
// a kit-defined named ctor whose paired-equality with `true_const`
// encodes the protocol-level claim.
//
// Operational verification of these claims is the same shape as the
// rust verify_c*_* functions: a per-rule predicate that takes a
// JSON-RPC message and returns Result<(), String>. The go-side
// verifiers live in the lift plugin itself (cmd/mint-go-self-contracts/
// rpc.go) and are exercised by the protocol-conformance tests at
// implementations/go/provekit-lift-go-tests/.
func InvariantsLiftPluginProtocolContracts() {
	// -- C1: initialize protocol_version match. ------------------------
	//
	// go-kit's `--rpc` initialize handler emits
	//   capabilities.protocol_version == "pep/1.7.0"
	// matching the request, OR responds with PROTOCOL_VERSION_MISMATCH
	// (the spec-named error).
	ir.Contract("go_lift_plugin_initialize_protocol_version_match",
		ir.ContractArgs{
			Pre: ir.Eq(
				ctor1("go_request_protocol_version", ir.StrConst("req")),
				ir.StrConst("pep/1.7.0"),
			),
			Post: ir.Eq(
				ctor1("go_response_confirms_protocol_or_errors_mismatch", ir.StrConst("req")),
				ctor1("true_const", ir.StrConst("")),
			),
		})

	// -- C2.a: initialize capabilities.authoring_surfaces is nonempty. -
	ir.Must("go_lift_plugin_initialize_capabilities_authoring_surfaces_nonempty",
		ir.ForAll(ir.String, func(resp ir.IrTerm) ir.IrFormula {
			return ir.Gte(
				ir.StringLength(ctor1("go_authoring_surfaces_of", resp)),
				ir.Num(1),
			)
		}))

	// -- C2.b: initialize capabilities.ir_version starts with "v". -----
	ir.Contract("go_lift_plugin_initialize_capabilities_ir_version_starts_with_v",
		ir.ContractArgs{
			Post: ir.Eq(
				ctor1("go_ir_version_starts_with_v", ir.StrConst("resp")),
				ctor1("true_const", ir.StrConst("")),
			),
		})

	// -- C3.a: lift request `surface` field is a string. ---------------
	ir.Contract("go_lift_plugin_lift_request_surface_is_string",
		ir.ContractArgs{
			Post: ir.Eq(
				ctor1("go_is_string", ctor1("go_surface_of", ir.StrConst("req"))),
				ctor1("true_const", ir.StrConst("")),
			),
		})

	// -- C3.b: lift request `source_paths` is nonempty. ----------------
	ir.Must("go_lift_plugin_lift_request_source_paths_nonempty",
		ir.ForAll(ir.String, func(req ir.IrTerm) ir.IrFormula {
			return ir.Gte(
				ir.StringLength(ctor1("go_source_paths_of", req)),
				ir.Num(1),
			)
		}))

	// -- C3.c: every lift request `source_paths` element is nonempty. --
	ir.Contract("go_lift_plugin_lift_request_source_paths_each_nonempty",
		ir.ContractArgs{
			Post: ir.Eq(
				ctor1("go_source_paths_each_nonempty", ir.StrConst("req")),
				ctor1("true_const", ir.StrConst("")),
			),
		})

	// -- C4: lift surface in capabilities (init handshake gate). -------
	ir.Contract("go_lift_plugin_lift_request_surface_in_capabilities",
		ir.ContractArgs{
			Post: ir.Eq(
				ctor2("go_surface_in_capabilities", ir.StrConst("req"), ir.StrConst("caps")),
				ctor1("true_const", ir.StrConst("")),
			),
		})

	// -- C5: lift response `kind` in {ir-document, signed-mementos,    -
	//        proof-envelope}. -----------------------------------------
	ir.Contract("go_lift_plugin_lift_response_kind_in_set",
		ir.ContractArgs{
			Post: ir.Eq(
				ctor1("go_response_kind_in_allowed_set", ir.StrConst("resp")),
				ctor1("true_const", ir.StrConst("")),
			),
		})

	// -- C6: when kind == "ir-document", `ir` field is an array. -------
	ir.Contract("go_lift_plugin_lift_response_ir_document_array",
		ir.ContractArgs{
			Post: ir.Eq(
				ctor1("go_ir_field_is_array_when_kind_ir_document", ir.StrConst("resp")),
				ctor1("true_const", ir.StrConst("")),
			),
		})

	// -- C7: `diagnostics` field, when present, is an array. -----------
	ir.Contract("go_lift_plugin_diagnostic_field_is_array",
		ir.ContractArgs{
			Post: ir.Eq(
				ctor1("go_diagnostics_field_is_array_or_absent", ir.StrConst("resp")),
				ctor1("true_const", ir.StrConst("")),
			),
		})

	// -- C8: lifter emits call-edge stream alongside contracts. ----------
	//        Mirrors rust C8 `lift_emits_call_edge_stream` (spec #114 §1
	//        R1). The ctor discharges vacuously when the compilation unit
	//        is empty and fires when a non-empty contract set is emitted
	//        with no accompanying call-edge stream.
	ir.Contract("go_lift_plugin_emits_call_edge_stream",
		ir.ContractArgs{
			Post: ir.Eq(
				ctor1("go_call_edge_stream_present_or_unit_empty", ir.StrConst("resp")),
				ctor1("true_const", ir.StrConst("")),
			),
		})
}

// InvariantsLiftPluginProtocolBridges authors the 11 cross-kit bridges
// linking each rust source contract (by memento CID) to its go
// counterpart contract.
//
// The orchestrator (cmd/mint-go-self-contracts/main.go) post-processes
// the collected BridgeDeclarations: for each one, it resolves
// `targetContractCid` from the placeholder to the real go counterpart
// contract memento CID after the contract slabs have minted. This
// mirrors rust's lib.rs:368-385 closed-loop bridge-resolution pass.
func InvariantsLiftPluginProtocolBridges() {
	for _, p := range liftPluginBridgePairs() {
		ir.Bridge(p.bridgeName, ir.BridgeSpec{
			SourceSymbol:      p.rustName,
			SourceLayer:       "rust-kit",
			SourceContractCid: rustContractCID[p.rustName],
			// Placeholder: the orchestrator rewrites this to the
			// counterpart's memento CID after minting all contracts.
			// Tests that walk the collected BridgeDeclaration directly
			// (without going through the orchestrator) see the
			// counterpart NAME here so the bridge is still self-
			// describing; orchestrator code knows the convention.
			TargetContractCid: pendingTargetContractCidPlaceholder(p.goCounterpart),
			TargetProofCid:    LiftPluginGoTargetProofCIDPlaceholder,
			TargetLayer:       "go-kit",
			Notes:             "lift-plugin-protocol conformance bridge; phase 2",
		})
	}
}

// pendingTargetContractCidPlaceholder embeds the go counterpart
// contract NAME inside a sentinel CID-shaped string. The orchestrator
// recognizes the "pending-go-counterpart:" prefix and rewrites the
// field to the real memento CID after all counterpart contracts have
// been minted. The shape is still a string, so JCS encoding succeeds
// and the bridge is structurally well-formed.
func pendingTargetContractCidPlaceholder(counterpartName string) string {
	return "pending-go-counterpart:" + counterpartName
}

// PendingTargetContractCidPrefix is exported so the orchestrator can
// detect-and-resolve the placeholder set by
// InvariantsLiftPluginProtocolBridges.
const PendingTargetContractCidPrefix = "pending-go-counterpart:"

// BuildLiftPluginProtocolBridges constructs the 11 BridgeDeclarations
// directly (without going through the kit collector). Used by the
// pinned-hash test and any cross-kit consumer that wants the bridges
// as values rather than collected declarations.
//
// Returned bridges have:
//   - SourceContractCid set to the rust memento CID (frozen).
//   - TargetContractCid set to the pending placeholder (the orchestrator
//     resolves this at mint time).
//   - TargetProofCid set to LiftPluginGoTargetProofCIDPlaceholder
//     ("deferred:phase-3-proof-bundle").
//
// Bridges are returned in stable declaration order.
func BuildLiftPluginProtocolBridges() []ir.BridgeDeclaration {
	pairs := liftPluginBridgePairs()
	out := make([]ir.BridgeDeclaration, len(pairs))
	for i, p := range pairs {
		out[i] = ir.BridgeDeclaration{
			Name:              p.bridgeName,
			SourceSymbol:      p.rustName,
			SourceLayer:       "rust-kit",
			SourceContractCid: rustContractCID[p.rustName],
			TargetContractCid: pendingTargetContractCidPlaceholder(p.goCounterpart),
			TargetProofCid:    LiftPluginGoTargetProofCIDPlaceholder,
			TargetLayer:       "go-kit",
			Notes:             "lift-plugin-protocol conformance bridge; phase 2",
		}
	}
	return out
}
