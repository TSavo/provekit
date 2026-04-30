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
//   - quantifier is FLAT: {kind, name, sort, body} — no Lambda wrapper
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
