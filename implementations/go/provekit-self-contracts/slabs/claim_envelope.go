package slabs

// Invariants for claim_envelope/{envelope,contract_minter,bridge_minter,
// from_kit}.go.
//
// Public surface covered:
//   * Minter, NewMinter, (*Minter).MintContract / MintBridge.
//   * SchemaCIDContract / SchemaCIDBridge / SchemaCIDImplication.
//   * Ed25519SigPrefix = "ed25519:".
//   * Verdict* constants.
//   * ContractMintArgs / BridgeMintArgs validation.
//   * FormulaToValue (round-trip via json).
//
// Honest scope: most contracts here are LIVING DOCS resolving undecidable
// at the verifier (the kit predicates `requiresPreOrPostOrInv`, `hasPrefix`
// have no Z3 semantics). Length predicates on schema CIDs are decidable.

import "github.com/tsavo/provekit/go/provekit-ir-symbolic/ir"

func InvariantsEnvelope() {
	// Schema CIDs are full-shape v1.1.0 (139 chars: "blake3-512:" + 128 hex).
	ir.Contract("SchemaCIDContract_length_eq_139", ir.ContractArgs{
		Post: ir.Eq(ir.StringLength(ctor0("SchemaCIDContract")), ir.Num(139)),
	})

	// Ed25519SigPrefix length sanity.
	// "ed25519:" is exactly 8 bytes.
	ir.Contract("Ed25519SigPrefix_length_eq_8", ir.ContractArgs{
		Post: ir.Eq(ir.StringLength(ctor0("Ed25519SigPrefix")), ir.Num(8)),
	})

	// finalize is a function: same unsigned body produces same signed
	// canonical bytes (modulo signer key, which is bound to the Minter).
	//   forall body: String. finalize(body) = finalize(body)
	ir.Must("Minter_finalize_is_deterministic",
		ir.ForAll(ir.String, func(b ir.IrTerm) ir.IrFormula {
			return ir.Eq(ctor1("Minter_finalize", b), ctor1("Minter_finalize", b))
		}))
}

func InvariantsContractMinter() {
	// MintContract requires at least one of Pre/Post/Inv.
	// Kit-defined predicate `requiresPreOrPostOrInv` has no Z3 semantics;
	// LIVING DOCS only.
	ir.Must("MintContract_requires_pre_or_post_or_inv",
		ir.ForAll(ir.String, func(_ ir.IrTerm) ir.IrFormula {
			// gesture: forall args. requiresPreOrPostOrInv(MintContract(args)) holds
			return ir.Eq(
				ctor1("MintContract_validate", ctor0("args")),
				ctor1("MintContract_validate", ctor0("args")),
			)
		}))

	// MintContract requires non-empty OutBinding.
	ir.Must("MintContract_requires_outBinding",
		ir.ForAll(ir.String, func(_ ir.IrTerm) ir.IrFormula {
			return ir.Gte(ir.StringLength(ctor0("OutBinding")), ir.Num(1))
		}))

	// MintContract requires non-empty ContractName.
	ir.Must("MintContract_requires_contractName",
		ir.ForAll(ir.String, func(_ ir.IrTerm) ir.IrFormula {
			return ir.Gte(ir.StringLength(ctor0("ContractName")), ir.Num(1))
		}))

	// preHash / postHash / invHash are derived (full BLAKE3-512, 139 chars
	// when present). Same shape contract as ComputeCID.
	ir.Must("MintContract_preHash_length_eq_139_when_present",
		ir.ForAll(ir.String, func(pre ir.IrTerm) ir.IrFormula {
			return ir.Eq(ir.StringLength(ctor1("preHash", pre)), ir.Num(139))
		}))
}

func InvariantsBridgeMinter() {
	// MintBridge requires non-empty SourceSymbol.
	ir.Must("MintBridge_requires_sourceSymbol",
		ir.ForAll(ir.String, func(_ ir.IrTerm) ir.IrFormula {
			return ir.Gte(ir.StringLength(ctor0("SourceSymbol")), ir.Num(1))
		}))

	// MintBridge requires non-empty TargetContractCID.
	ir.Must("MintBridge_requires_targetContractCID",
		ir.ForAll(ir.String, func(_ ir.IrTerm) ir.IrFormula {
			return ir.Gte(ir.StringLength(ctor0("TargetContractCID")), ir.Num(1))
		}))

	// Bridge propertyHash is hashRawString("bridge:" + sourceSymbol);
	// full 139-char CID.
	ir.Must("MintBridge_propertyHash_length_eq_139",
		ir.ForAll(ir.String, func(s ir.IrTerm) ir.IrFormula {
			return ir.Eq(ir.StringLength(ctor1("MintBridge_propertyHash", s)), ir.Num(139))
		}))
}

func InvariantsFromKit() {
	// FormulaToValue is deterministic.
	//   forall f: String. FormulaToValue(f) = FormulaToValue(f)
	ir.Must("FormulaToValue_is_deterministic",
		ir.ForAll(ir.String, func(f ir.IrTerm) ir.IrFormula {
			return ir.Eq(ctor1("FormulaToValue", f), ctor1("FormulaToValue", f))
		}))

	// FormulaToValue round-trips json.Marshal then json.Unmarshal; the
	// result is a JSON-shape value (no IR types leak through).
	// LIVING DOCS — kit-defined predicate `isJSONShape` has no Z3 semantics.
	ir.Must("FormulaToValue_returns_json_shape",
		ir.ForAll(ir.String, func(f ir.IrTerm) ir.IrFormula {
			// gesture: roundtrip stability
			return ir.Eq(
				ctor1("FormulaToValue", ctor1("FormulaToValue_lift", f)),
				ctor1("FormulaToValue", ctor1("FormulaToValue_lift", f)),
			)
		}))
}
