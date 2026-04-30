package ir

import (
	"encoding/json"
	"testing"
)

// Fixtures generated from the TS kit's `@provekit/ir/symbolic` module.
// Reproduce locally:
//
//	npx tsx <<'EOF'
//	import { property, beginCollecting, _resetCollector,
//	  forAll, exists, parseInt, eq, num, str, Int, String as StringSort
//	} from "/path/to/src/ir/symbolic/index.js";
//
//	_resetCollector();
//	const f = beginCollecting();
//	property("zeroIsZero", eq(parseInt(str("0")), num(0)));
//	console.log(JSON.stringify(f()));
//	EOF
//
// Cross-language equivalence contract: the Go kit's IR data structure
// MUST serialize to byte-identical JSON for the same logical claim.
// JSON parity is a sanity proxy that the AST canonicalizer's input
// matches across kits — the load-bearing hash is CBOR over the
// canonical FOL form (see docs/specs/2026-04-29-ast-canonicalizer.md).

const tsFixtureSimpleEq = `[{"kind":"property","name":"zeroIsZero","formula":{"kind":"atomic","predicate":"=","args":[{"kind":"ctor","name":"parseInt","args":[{"kind":"const","value":"0","sort":{"kind":"primitive","name":"String"}}],"sort":{"kind":"primitive","name":"Int"}},{"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}}]}}]`

const tsFixtureForAllEq = `[{"kind":"property","name":"denominator-nonzero","formula":{"kind":"forall","sort":{"kind":"primitive","name":"Int"},"predicate":{"kind":"lambda","varName":"_x0","sort":{"kind":"primitive","name":"Int"},"body":{"kind":"atomic","predicate":"=","args":[{"kind":"var","name":"_x0","sort":{"kind":"primitive","name":"Int"}},{"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}}]}}}}]`

const tsFixtureExistsParseInt = `[{"kind":"property","name":"can-be-zero","formula":{"kind":"exists","sort":{"kind":"primitive","name":"String"},"predicate":{"kind":"lambda","varName":"_x0","sort":{"kind":"primitive","name":"String"},"body":{"kind":"atomic","predicate":"=","args":[{"kind":"ctor","name":"parseInt","args":[{"kind":"var","name":"_x0","sort":{"kind":"primitive","name":"String"}}],"sort":{"kind":"primitive","name":"Int"}},{"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}}]}}}}]`

func TestCanonicalFormSimpleEqMatchesTS(t *testing.T) {
	ResetCollector()
	finish := BeginCollecting()
	Property("zeroIsZero", Eq(ParseInt(StrConst("0")), Num(0)))
	decls := finish()

	got, err := json.Marshal(decls)
	if err != nil {
		t.Fatalf("marshal error: %v", err)
	}
	if string(got) != tsFixtureSimpleEq {
		t.Errorf("byte-equivalence with TS kit failed:\n  got:  %s\n  want: %s", got, tsFixtureSimpleEq)
	}
}

func TestCanonicalFormForAllMatchesTS(t *testing.T) {
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
	if string(got) != tsFixtureForAllEq {
		t.Errorf("byte-equivalence with TS kit failed:\n  got:  %s\n  want: %s", got, tsFixtureForAllEq)
	}
}

func TestCanonicalFormExistsParseIntMatchesTS(t *testing.T) {
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
	if string(got) != tsFixtureExistsParseInt {
		t.Errorf("byte-equivalence with TS kit failed:\n  got:  %s\n  want: %s", got, tsFixtureExistsParseInt)
	}
}

func TestCanonicalFormDeterministic(t *testing.T) {
	// Same logical claim built twice must produce byte-identical JSON.
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
	if string(got) != tsFixtureSimpleEq {
		t.Errorf("MarshalDeclarations parity:\n  got:  %s\n  want: %s", got, tsFixtureSimpleEq)
	}
}
