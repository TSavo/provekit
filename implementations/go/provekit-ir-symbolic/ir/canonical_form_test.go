package ir

import (
	"encoding/json"
	"testing"
)

// Golden v1.1.0 IR-JSON byte sequences for the Go kit. These are the
// canonical wire forms after the maximal-uniformity cut:
//
//   - top-level decl is `kind:"contract"` (was `"property"`)
//   - contract has `outBinding` always; `pre/post/inv` optional, omitted when nil
//   - quantifier is FLAT: {kind, name, sort, body}; no Lambda wrapper
//   - var/ctor drop their `sort` field from JSON; const keeps it
//   - atomic uses `name` (was `predicate`)
//   - and/or/not/implies all use `operands` (no conjuncts/disjuncts/body/antecedent)
//
// Locked key orders track the IR formal grammar
// (protocol/specs/2026-04-30-ir-formal-grammar.md). Sister kits (C++
// reference, future TS port) must hash to the same JCS bytes for the
// same logical claim.

const goldenSimpleEq = `[{"kind":"contract","name":"zeroIsZero","outBinding":"out","pre":{"kind":"atomic","name":"=","args":[{"kind":"ctor","name":"parseInt","args":[{"kind":"const","value":"0","sort":{"kind":"primitive","name":"String"}}]},{"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}}]}}]`

const goldenForAllEq = `[{"kind":"contract","name":"denominator-nonzero","outBinding":"out","pre":{"kind":"forall","name":"_x0","sort":{"kind":"primitive","name":"Int"},"body":{"kind":"atomic","name":"=","args":[{"kind":"var","name":"_x0"},{"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}}]}}}]`

const goldenExistsParseInt = `[{"kind":"contract","name":"can-be-zero","outBinding":"out","pre":{"kind":"exists","name":"_x0","sort":{"kind":"primitive","name":"String"},"body":{"kind":"atomic","name":"=","args":[{"kind":"ctor","name":"parseInt","args":[{"kind":"var","name":"_x0"}]},{"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}}]}}}]`

func TestCanonicalFormSimpleEq(t *testing.T) {
	ResetCollector()
	finish := BeginCollecting()
	Property("zeroIsZero", Eq(ParseInt(StrConst("0")), Num(0)))
	decls := finish()

	got, err := json.Marshal(decls)
	if err != nil {
		t.Fatalf("marshal error: %v", err)
	}
	if string(got) != goldenSimpleEq {
		t.Errorf("v1.1.0 IR-JSON shape mismatch:\n  got:  %s\n  want: %s", got, goldenSimpleEq)
	}
}

func TestCanonicalFormForAllEq(t *testing.T) {
	ResetCollector()
	finish := BeginCollecting()
	Property("denominator-nonzero", ForAll(Int, func(b IrTerm) IrFormula {
		return Eq(b, Num(0))
	}))
	decls := finish()

	got, err := json.Marshal(decls)
	if err != nil {
		t.Fatalf("marshal error: %v", err)
	}
	if string(got) != goldenForAllEq {
		t.Errorf("v1.1.0 IR-JSON shape mismatch:\n  got:  %s\n  want: %s", got, goldenForAllEq)
	}
}

func TestCanonicalFormExistsParseInt(t *testing.T) {
	ResetCollector()
	finish := BeginCollecting()
	Property("can-be-zero", Exists(String, func(s IrTerm) IrFormula {
		return Eq(ParseInt(s), Num(0))
	}))
	decls := finish()

	got, err := json.Marshal(decls)
	if err != nil {
		t.Fatalf("marshal error: %v", err)
	}
	if string(got) != goldenExistsParseInt {
		t.Errorf("v1.1.0 IR-JSON shape mismatch:\n  got:  %s\n  want: %s", got, goldenExistsParseInt)
	}
}

func TestCanonicalFormDeterministic(t *testing.T) {
	build := func() []byte {
		ResetCollector()
		finish := BeginCollecting()
		Property("rt", Eq(Num(0), Num(0)))
		decls := finish()
		out, err := json.Marshal(decls)
		if err != nil {
			t.Fatalf("marshal: %v", err)
		}
		return out
	}
	a := build()
	b := build()
	if string(a) != string(b) {
		t.Errorf("not deterministic:\n  a: %s\n  b: %s", a, b)
	}
}

func TestMarshalDeclarationsHelper(t *testing.T) {
	ResetCollector()
	finish := BeginCollecting()
	Property("zeroIsZero", Eq(ParseInt(StrConst("0")), Num(0)))
	decls := finish()

	got, err := MarshalDeclarations(decls)
	if err != nil {
		t.Fatalf("MarshalDeclarations error: %v", err)
	}
	if string(got) != goldenSimpleEq {
		t.Errorf("MarshalDeclarations shape:\n  got:  %s\n  want: %s", got, goldenSimpleEq)
	}
}

// goldenBridgeWithPinning is the v1.3.0 BridgeDeclaration locked-key-
// order JSON for a bridge that carries sourceContractCid + targetProofCid
// (per protocol/specs/2026-04-30-ir-formal-grammar.md, post PR #10).
//
// Cross-impl invariant: the Rust kit's serde-derived
// `Declaration::Bridge` (provekit-ir-types/src/lib.rs) emits the field
// sequence
//
//	{kind, name, sourceSymbol, sourceLayer, sourceContractCid,
//	 targetContractCid, targetProofCid, targetLayer [, notes]}
//
// in declaration order; serde with `skip_serializing_if =
// "Option::is_none"` omits notes when None. The Go MarshalJSON
// hand-emits the same sequence. This golden test pins the exact byte
// sequence so any drift between the Go kit and Rust kit (or a future
// change to either MarshalJSON) shows up here, not at the JCS hash
// boundary.
const goldenBridgeWithPinning = `[{"kind":"bridge","name":"js-parseInt-to-ref","sourceSymbol":"parseInt","sourceLayer":"javascript","sourceContractCid":"blake3-512:js-parseInt-v24","targetContractCid":"blake3-512:ref-parseInt-v1","targetProofCid":"blake3-512:ecma262-v14-proof","targetLayer":"reference"}]`

const goldenBridgeWithPinningAndNotes = `[{"kind":"bridge","name":"js-parseInt-to-ref","sourceSymbol":"parseInt","sourceLayer":"javascript","sourceContractCid":"blake3-512:js-parseInt-v24","targetContractCid":"blake3-512:ref-parseInt-v1","targetProofCid":"blake3-512:ecma262-v14-proof","targetLayer":"reference","notes":"the canonical bridge"}]`

func TestCanonicalFormBridgeWithPinning(t *testing.T) {
	ResetCollector()
	finish := BeginCollecting()
	Bridge("js-parseInt-to-ref", BridgeSpec{
		SourceSymbol:      "parseInt",
		SourceLayer:       "javascript",
		SourceContractCid: "blake3-512:js-parseInt-v24",
		TargetContractCid: "blake3-512:ref-parseInt-v1",
		TargetProofCid:    "blake3-512:ecma262-v14-proof",
		TargetLayer:       "reference",
	})
	decls := finish()

	got, err := json.Marshal(decls)
	if err != nil {
		t.Fatalf("marshal error: %v", err)
	}
	if string(got) != goldenBridgeWithPinning {
		t.Errorf("v1.3.0 bridge JSON shape mismatch:\n  got:  %s\n  want: %s", got, goldenBridgeWithPinning)
	}
}

func TestCanonicalFormBridgeWithPinningAndNotes(t *testing.T) {
	ResetCollector()
	finish := BeginCollecting()
	Bridge("js-parseInt-to-ref", BridgeSpec{
		SourceSymbol:      "parseInt",
		SourceLayer:       "javascript",
		SourceContractCid: "blake3-512:js-parseInt-v24",
		TargetContractCid: "blake3-512:ref-parseInt-v1",
		TargetProofCid:    "blake3-512:ecma262-v14-proof",
		TargetLayer:       "reference",
		Notes:             "the canonical bridge",
	})
	decls := finish()

	got, err := json.Marshal(decls)
	if err != nil {
		t.Fatalf("marshal error: %v", err)
	}
	if string(got) != goldenBridgeWithPinningAndNotes {
		t.Errorf("v1.3.0 bridge+notes JSON shape mismatch:\n  got:  %s\n  want: %s", got, goldenBridgeWithPinningAndNotes)
	}
}
