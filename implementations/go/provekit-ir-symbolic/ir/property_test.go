package ir

import (
	"strings"
	"testing"
)

func TestPropertyCollectsDeclaration(t *testing.T) {
	ResetCollector()
	finish := BeginCollecting()
	Property("zeroIsZero", Eq(ParseInt(StrConst("0")), Num(0)))
	decls := finish()

	if len(decls) != 1 {
		t.Fatalf("want 1 decl, got %d", len(decls))
	}
	pd, ok := decls[0].(PropertyDeclaration)
	if !ok {
		t.Fatalf("want PropertyDeclaration, got %T", decls[0])
	}
	if pd.Name != "zeroIsZero" {
		t.Errorf("name: want zeroIsZero, got %s", pd.Name)
	}
}

func TestBridgeCollectsDeclaration(t *testing.T) {
	ResetCollector()
	finish := BeginCollecting()
	Bridge("parseIntBridgesV8", BridgeSpec{
		SourceSymbol:      "global.parseInt",
		SourceLayer:       "ts-kit@1.0",
		TargetContractCid: "abc1234567890def",
		TargetLayer:       "V8@12.4",
		Notes:             "the canonical bridge",
	})
	decls := finish()

	if len(decls) != 1 {
		t.Fatalf("want 1 decl, got %d", len(decls))
	}
	bd, ok := decls[0].(BridgeDeclaration)
	if !ok {
		t.Fatalf("want BridgeDeclaration, got %T", decls[0])
	}
	if bd.SourceSymbol != "global.parseInt" {
		t.Errorf("source: want global.parseInt, got %s", bd.SourceSymbol)
	}
	if bd.Notes != "the canonical bridge" {
		t.Errorf("notes: want 'the canonical bridge', got %s", bd.Notes)
	}
}

func TestMultipleDeclsCollectInOrder(t *testing.T) {
	ResetCollector()
	finish := BeginCollecting()
	Property("p1", Eq(Num(0), Num(0)))
	Bridge("b1", BridgeSpec{
		SourceSymbol:      "x",
		SourceLayer:       "L1",
		TargetContractCid: strings.Repeat("0", 32),
		TargetLayer:       "L2",
	})
	Property("p2", Eq(Num(1), Num(1)))
	decls := finish()

	if len(decls) != 3 {
		t.Fatalf("want 3 decls, got %d", len(decls))
	}
	wantKinds := []string{"property", "bridge", "property"}
	wantNames := []string{"p1", "b1", "p2"}
	for i, d := range decls {
		if d.Kind() != wantKinds[i] {
			t.Errorf("decls[%d] kind: want %s, got %s", i, wantKinds[i], d.Kind())
		}
		if d.DeclName() != wantNames[i] {
			t.Errorf("decls[%d] name: want %s, got %s", i, wantNames[i], d.DeclName())
		}
	}
}

func TestPropertyOutsideCollectorPanics(t *testing.T) {
	ResetCollector()
	defer func() {
		r := recover()
		if r == nil {
			t.Fatal("want panic, got none")
		}
		if !strings.Contains(r.(string), "outside an active collector") {
			t.Errorf("panic message: %v", r)
		}
	}()
	Property("orphan", Eq(Num(0), Num(0)))
}

func TestNestedBeginCollectingPanics(t *testing.T) {
	ResetCollector()
	finish := BeginCollecting()
	defer finish()
	defer func() {
		r := recover()
		if r == nil {
			t.Fatal("want panic, got none")
		}
		if !strings.Contains(r.(string), "already active") {
			t.Errorf("panic message: %v", r)
		}
	}()
	BeginCollecting()
}

func TestForAllWrapsBody(t *testing.T) {
	ResetCollector()
	BeginCollecting()
	defer ResetCollector()

	f := ForAll(Int, func(x IrTerm) IrFormula {
		return Gt(x, Num(0))
	})
	fa, ok := f.(forAllFormula)
	if !ok {
		t.Fatalf("want forAllFormula, got %T", f)
	}
	if fa.Sort != Int {
		t.Errorf("sort: want Int")
	}
	if _, ok := fa.Predicate.Body.(atomicFormula); !ok {
		t.Errorf("predicate body: want atomicFormula, got %T", fa.Predicate.Body)
	}
}

func TestExistsWrapsBody(t *testing.T) {
	ResetCollector()
	BeginCollecting()
	defer ResetCollector()

	f := Exists(String, func(s IrTerm) IrFormula {
		return Eq(ParseInt(s), Num(0))
	})
	if _, ok := f.(existsFormula); !ok {
		t.Fatalf("want existsFormula, got %T", f)
	}
}

func TestDescribeMustBuildsPath(t *testing.T) {
	ResetCollector()
	finish := BeginCollecting()

	Describe("parseInt", func() {
		Must("canReturnZero", Exists(String, func(s IrTerm) IrFormula {
			return Eq(ParseInt(s), Num(0))
		}))
	})

	decls := finish()
	if len(decls) != 1 {
		t.Fatalf("want 1 decl, got %d", len(decls))
	}
	if decls[0].DeclName() != "parseInt > canReturnZero" {
		t.Errorf("name: want 'parseInt > canReturnZero', got %s", decls[0].DeclName())
	}
}

func TestNestedDescribeBuildsPath(t *testing.T) {
	ResetCollector()
	finish := BeginCollecting()

	Describe("Math", func() {
		Describe("abs", func() {
			Must("non-negative", ForAll(Int, func(x IrTerm) IrFormula {
				return Gt(Abs(x), Num(-1))
			}))
		})
	})

	decls := finish()
	if len(decls) != 1 {
		t.Fatalf("want 1 decl, got %d", len(decls))
	}
	if decls[0].DeclName() != "Math > abs > non-negative" {
		t.Errorf("name: want 'Math > abs > non-negative', got %s", decls[0].DeclName())
	}
}

func TestDescribePopsAfterBody(t *testing.T) {
	ResetCollector()
	finish := BeginCollecting()

	Describe("a", func() {
		Must("inner", Eq(Num(0), Num(0)))
	})
	Must("outer", Eq(Num(1), Num(1)))

	decls := finish()
	got := []string{decls[0].DeclName(), decls[1].DeclName()}
	want := []string{"a > inner", "outer"}
	for i := range got {
		if got[i] != want[i] {
			t.Errorf("decls[%d]: want %s, got %s", i, want[i], got[i])
		}
	}
}

func TestMustSkipIsNoOp(t *testing.T) {
	ResetCollector()
	finish := BeginCollecting()

	Describe("parseInt", func() {
		MustSkip("legacy", Eq(Num(0), Num(0)))
		Must("real", Eq(Num(1), Num(1)))
	})

	decls := finish()
	if len(decls) != 1 {
		t.Fatalf("want 1 decl, got %d", len(decls))
	}
	if decls[0].DeclName() != "parseInt > real" {
		t.Errorf("name: want 'parseInt > real', got %s", decls[0].DeclName())
	}
}

func TestDescribeSkipDoesNotInvokeBody(t *testing.T) {
	ResetCollector()
	finish := BeginCollecting()

	bodyRan := false
	DescribeSkip("never", func() {
		bodyRan = true
		Must("ignored", Eq(Num(0), Num(0)))
	})

	decls := finish()
	if len(decls) != 0 {
		t.Errorf("want 0 decls, got %d", len(decls))
	}
	if bodyRan {
		t.Errorf("body should not have run")
	}
}

func TestResetCollectorRecoversFromLeakedState(t *testing.T) {
	BeginCollecting()
	// Simulate exception leaking the collector.
	ResetCollector()

	finish := BeginCollecting()
	Property("ok", Eq(Num(0), Num(0)))
	decls := finish()
	if len(decls) != 1 {
		t.Fatalf("want 1 decl, got %d", len(decls))
	}
}

func TestQuantifierCounterResetsBetweenCollections(t *testing.T) {
	ResetCollector()

	finish1 := BeginCollecting()
	f1 := ForAll(Int, func(x IrTerm) IrFormula { return Eq(x, Num(0)) })
	_ = f1
	finish1()

	finish2 := BeginCollecting()
	f2 := ForAll(Int, func(x IrTerm) IrFormula { return Eq(x, Num(0)) })
	defer finish2()

	fa1 := f1.(forAllFormula)
	fa2 := f2.(forAllFormula)

	if fa1.Predicate.VarName != "_x0" {
		t.Errorf("first run varName: want _x0, got %s", fa1.Predicate.VarName)
	}
	if fa2.Predicate.VarName != "_x0" {
		t.Errorf("second run varName: want _x0 (counter reset), got %s", fa2.Predicate.VarName)
	}
}
