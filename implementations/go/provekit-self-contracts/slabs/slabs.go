// Package slabs holds the per-source-file invariant authors for the Go
// peer self-contracts dogfood. Each public-API Go source file under
// implementations/go/provekit-ir-symbolic/{canonicalizer,claim_envelope,
// ir,proof_envelope,verifier}/ has a sister `Invariants_<label>()`
// function declaring 2-7 contracts about its public surface.
//
// Style mirrors implementations/rust/.../{*.invariant.rs}: kit
// primitives only, IR ctor-by-name to model "the function called X",
// honest scope (some contracts are LIVING DOCS that resolve undecidable
// at the verifier; others reach Z3 cleanly).
//
// The orchestrator in cmd/mint-go-self-contracts walks each Invariants_*
// function, drains the kit collector, mints each as a signed memento
// under the foundation key, and bundles the lot into a single .proof
// whose filename IS the catalog CID. Two runs into separate dirs MUST
// produce identical CIDs; the binary fails loud if not.
package slabs

import "github.com/tsavo/provekit/go/provekit-ir-symbolic/ir"

// Slab is one source file's authored contracts plus the source label.
// The orchestrator iterates Slabs() and mints each in deterministic
// (insertion) order.
type Slab struct {
	Label string
	Path  string
	Run   func()
}

// ctor1 wraps a kit-defined unary ctor by name. Mirrors the Rust pattern
// for modeling "the function called <name>" as an opaque IR Ctor node.
func ctor1(name string, arg ir.IrTerm) ir.IrTerm {
	return ir.MakeCtor(name, []ir.IrTerm{arg}, ir.Int)
}

// ctor2 wraps a binary kit-defined ctor by name.
func ctor2(name string, a, b ir.IrTerm) ir.IrTerm {
	return ir.MakeCtor(name, []ir.IrTerm{a, b}, ir.Int)
}

// ctor0 wraps a nullary "constant" ctor (e.g. a package-level constant
// like BLAKE3_512_PREFIX). The dummy Int arg is a quirk of MakeCtor
// requiring args; mirrors the Rust pattern at hash.invariant.rs:80.
func ctor0(name string) ir.IrTerm {
	return ir.MakeCtor(name, []ir.IrTerm{ir.Num(0)}, ir.Int)
}

// Slabs returns every source-file slab in deterministic order.
//
// Insertion order is load-bearing: the orchestrator preserves it through
// the BTreeMap of mementos by content-address (member CIDs sort lex,
// not by insertion), but the per_source_counts report mirrors this order.
func Slabs() []Slab {
	return []Slab{
		{Label: "encoder", Path: "canonicalizer/encoder.go", Run: InvariantsEncoder},
		{Label: "hasher", Path: "canonicalizer/hasher.go", Run: InvariantsHasher},
		{Label: "envelope", Path: "claim_envelope/envelope.go", Run: InvariantsEnvelope},
		{Label: "contract_minter", Path: "claim_envelope/contract_minter.go", Run: InvariantsContractMinter},
		{Label: "bridge_minter", Path: "claim_envelope/bridge_minter.go", Run: InvariantsBridgeMinter},
		{Label: "from_kit", Path: "claim_envelope/from_kit.go", Run: InvariantsFromKit},
		{Label: "ir_types", Path: "ir/types.go", Run: InvariantsIRTypes},
		{Label: "ir_quantifiers", Path: "ir/quantifiers.go", Run: InvariantsIRQuantifiers},
		{Label: "ir_property", Path: "ir/property.go", Run: InvariantsIRProperty},
		{Label: "ir_canonicalize", Path: "ir/canonicalize.go", Run: InvariantsIRCanonicalize},
		{Label: "proof_builder", Path: "proof_envelope/builder.go", Run: InvariantsProofBuilder},
		{Label: "proof_cbor", Path: "proof_envelope/cbor.go", Run: InvariantsProofCBOR},
		{Label: "load_all_proofs", Path: "verifier/load_all_proofs.go", Run: InvariantsLoadAllProofs},
		// Phase-2 cross-kit bridges to rust's lift-plugin-protocol contracts.
		// Two slabs: counterpart contracts first, then bridges. The order
		// is load-bearing for the orchestrator's post-mint resolution
		// pass: every counterpart contract MUST be minted before the
		// bridges are processed so the placeholder targetContractCid
		// values can be rewritten to real memento CIDs.
		{
			Label: "lift_plugin_protocol_contracts",
			Path:  "provekit-self-contracts/slabs/lift_plugin_protocol.go",
			Run:   InvariantsLiftPluginProtocolContracts,
		},
		{
			Label: "lift_plugin_protocol_bridges",
			Path:  "provekit-self-contracts/slabs/lift_plugin_protocol.go",
			Run:   InvariantsLiftPluginProtocolBridges,
		},
	}
}
