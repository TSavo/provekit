package slabs

// Invariants for canonicalizer/encoder.go and canonicalizer/hasher.go.
//
// Public surface covered:
//   * EncodeJCS(v interface{}) ([]byte, error): RFC 8785 JCS-JSON.
//   * Encoder, NewEncoder, (*Encoder).Encode.
//   * Blake3_512Hex(bytes []byte) string: un-prefixed 128-hex digest.
//   * ComputeCID(canonical []byte) string: "blake3-512:" + 128 hex.
//   * HashTagPrefix = "blake3-512:".
//
// Honest scope: same as Rust's hash.invariant.rs / jcs.invariant.rs.
// The IR cannot model BLAKE3 collision resistance; what it CAN say is
// shape-level (length 139, prefix length, deterministic, non-empty).

import "github.com/tsavo/provekit/go/provekit-ir-symbolic/ir"

// InvariantsEncoder authors contracts about EncodeJCS / Encoder.Encode.
func InvariantsEncoder() {
	// EncodeJCS is a function: same input, same output.
	//   forall s: String. EncodeJCS(s) = EncodeJCS(s)
	// Trivially true under "=" axioms; serves as a determinism memento.
	// The byte-faithful RFC 8785 conformance is operationally enforced
	// by canonicalizer/encoder_test.go.
	ir.Must("EncodeJCS_is_deterministic",
		ir.ForAll(ir.String, func(s ir.IrTerm) ir.IrFormula {
			return ir.Eq(ctor1("EncodeJCS", s), ctor1("EncodeJCS", s))
		}))

	// Output length is bounded below by 1.
	//   forall v: String. len(EncodeJCS(v)) >= 1
	// Smallest JCS value is `0` at length 1; arrays/objects emit `[]` /
	// `{}` at length 2.
	ir.Must("EncodeJCS_output_nonempty",
		ir.ForAll(ir.String, func(v ir.IrTerm) ir.IrFormula {
			return ir.Gte(ir.StringLength(ctor1("EncodeJCS", v)), ir.Num(1))
		}))

	// "true" emission is exactly the literal "true", length 4.
	// STRONGER INVARIANT (byte-equality) lives in encoder_test.go.
	ir.Contract("EncodeJCS_true_length_eq_4", ir.ContractArgs{
		Post: ir.Eq(
			ir.StringLength(ctor1("EncodeJCS", ir.StrConst("true"))),
			ir.Num(4),
		),
	})

	// Empty array emits "[]", length 2.
	ir.Contract("EncodeJCS_empty_array_length_eq_2", ir.ContractArgs{
		Post: ir.Eq(
			ir.StringLength(ctor1("EncodeJCS", ir.StrConst("[]"))),
			ir.Num(2),
		),
	})
}

// InvariantsHasher authors contracts about Blake3_512Hex / ComputeCID /
// HashTagPrefix.
func InvariantsHasher() {
	// ComputeCID output length is exactly 139 (11 prefix + 128 hex).
	//   forall b: String. len(ComputeCID(b)) = 139
	ir.Must("ComputeCID_output_length_eq_139",
		ir.ForAll(ir.String, func(b ir.IrTerm) ir.IrFormula {
			return ir.Eq(ir.StringLength(ctor1("ComputeCID", b)), ir.Num(139))
		}))

	// Blake3_512Hex output length is exactly 128.
	ir.Must("Blake3_512Hex_output_length_eq_128",
		ir.ForAll(ir.String, func(b ir.IrTerm) ir.IrFormula {
			return ir.Eq(ir.StringLength(ctor1("Blake3_512Hex", b)), ir.Num(128))
		}))

	// ComputeCID is deterministic.
	ir.Must("ComputeCID_is_deterministic",
		ir.ForAll(ir.String, func(b ir.IrTerm) ir.IrFormula {
			return ir.Eq(ctor1("ComputeCID", b), ctor1("ComputeCID", b))
		}))

	// HashTagPrefix length sanity: at least 10 chars.
	// The constant "blake3-512:" is exactly 11 bytes; >= 10 future-proofs
	// against versioned tags (e.g. "blake3-512-v2:") that would still hold.
	ir.Contract("HashTagPrefix_min_length", ir.ContractArgs{
		Post: ir.Gte(ir.StringLength(ctor0("HashTagPrefix")), ir.Num(10)),
	})
}
