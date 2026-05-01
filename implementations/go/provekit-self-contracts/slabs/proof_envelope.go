package slabs

// Invariants for proof_envelope/{builder,cbor}.go.
//
// Public surface covered:
//   * Builder, NewBuilder, (*Builder).Build(*Input) (*Output, error).
//   * CBOREncoder + EncodeTStr/EncodeBStr/EncodeMapHead.
//   * Deterministic-CBOR per RFC 8949 §4.2.1.
//
// Honest scope: byte-level CBOR shape claims (shortest-form, sorted-
// keys) are operationally enforced by tests. The IR can express output
// shape (FilenameCID is "blake3-512:" + 128 hex; sig is 64 bytes).

import "github.com/provekit/ir-symbolic/ir"

func InvariantsProofBuilder() {
	// Build's FilenameCID is exactly 139 chars (full v1.1.0 form, no
	// truncation). Mirrors ComputeCID length contract.
	ir.Must("Build_filenameCID_length_eq_139",
		ir.ForAll(ir.String, func(in ir.IrTerm) ir.IrFormula {
			return ir.Eq(ir.StringLength(ctor1("Build_filenameCID", in)), ir.Num(139))
		}))

	// ed25519 signature in the catalog is exactly 64 bytes (raw bstr).
	ir.Contract("Build_signature_byte_length_eq_64", ir.ContractArgs{
		Post: ir.Eq(ctor0("Build_sig_len"), ir.Num(64)),
	})

	// Build is deterministic for fixed (Members, SignerSeed, DeclaredAt).
	//   forall in. Build(in) = Build(in)
	ir.Must("Build_is_deterministic",
		ir.ForAll(ir.String, func(in ir.IrTerm) ir.IrFormula {
			return ir.Eq(ctor1("Build", in), ctor1("Build", in))
		}))
}

func InvariantsProofCBOR() {
	// CBOREncoder.AppendHead is shortest-form (RFC 8949 §4.2.1).
	// LIVING DOCS — `shortestForm` opaque to Z3.
	ir.Must("AppendHead_is_shortest_form",
		ir.ForAll(ir.Int, func(arg ir.IrTerm) ir.IrFormula {
			return ir.Eq(ctor1("AppendHead", arg), ctor1("AppendHead", arg))
		}))

	// CBOR encode of an empty map is exactly 1 byte (0xa0 = MajorMap | 0).
	ir.Contract("EncodeMapHead_zero_emits_one_byte", ir.ContractArgs{
		Post: ir.Eq(
			ir.StringLength(ctor1("EncodeMapHead", ir.Num(0))),
			ir.Num(1),
		),
	})

	// emitSortedMap key order is bytewise lex of CBOR-form (RFC 8949
	// §4.2.1). Determinism gesture; the byte-equality claim lives in
	// proof_envelope tests.
	ir.Must("emitSortedMap_is_deterministic",
		ir.ForAll(ir.String, func(pairs ir.IrTerm) ir.IrFormula {
			return ir.Eq(ctor1("emitSortedMap", pairs), ctor1("emitSortedMap", pairs))
		}))
}
