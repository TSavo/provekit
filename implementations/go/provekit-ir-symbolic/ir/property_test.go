package ir

import (
	"strings"
	"testing"
)

func TestContractCollectsDeclaration(t *testing.T) {
	ResetCollector()
	finish := BeginCollecting()
	Property("zeroIsZero", Eq(ParseInt(StrConst("0")), Num(0)))
	decls := finish()

	if len(decls) != 1 {
		t.Fatalf("want 1 decl, got %d", len(decls))
	}
	cd, ok := decls[0].(ContractDeclaration)
	if !ok {
		t.Fatalf("want ContractDeclaration, got %T", decls[0])
	}
	if cd.Name != "zeroIsZero" {
		t.Errorf("name: want zeroIsZero, got %s", cd.Name)
	}
	if cd.OutBinding != "out" {
		t.Errorf("outBinding: want out, got %s", cd.OutBinding)
	}
	if cd.Pre == nil {
		t.Errorf("Pre should be set by Property()")
	}
}

func TestContractWithPostAndInv(t *testing.T) {
	ResetCollector()
	finish := BeginCollecting()
	pre := Eq(Num(1), Num(1))
	post := Eq(Num(2), Num(2))
	inv := Eq(Num(3), Num(3))
	Contract("threeSlots", ContractArgs{Pre: pre, Post: post, Inv: inv})
	decls := finish()

	cd := decls[0].(ContractDeclaration)
	if cd.Pre == nil || cd.Post == nil || cd.Inv == nil {
		t.Errorf("all three slots should be set")
	}
}

func TestContractRequiresAtLeastOneSlot(t *testing.T) {
	ResetCollector()
	BeginCollecting()
	defer ResetCollector()
	defer func() {
		r := recover()
		if r == nil {
			t.Fatal("want panic when all slots nil, got none")
		}
		msg, _ := r.(string)
		if !strings.Contains(msg, "at least one of Pre / Post / Inv") {
			t.Errorf("panic message: %v", r)
		}
	}()
	Contract("emptySlots", ContractArgs{})
}

func TestMustIsPreconditionAlias(t *testing.T) {
	ResetCollector()
	finish := BeginCollecting()
	Must("aliasOfPre", Eq(Num(0), Num(0)))
	decls := finish()

	cd := decls[0].(ContractDeclaration)
	if cd.Pre == nil {
		t.Errorf("Must should set Pre")
	}
	if cd.Post != nil || cd.Inv != nil {
		t.Errorf("Must should not set Post/Inv")
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
	wantKinds := []string{"contract", "bridge", "contract"}
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

func TestForAllReturnsFlatQuantifier(t *testing.T) {
	ResetCollector()
	BeginCollecting()
	defer ResetCollector()

	f := ForAll(Int, func(x IrTerm) IrFormula {
		return Gt(x, Num(0))
	})
	q, ok := f.(quantFormula)
	if !ok {
		t.Fatalf("want quantFormula, got %T", f)
	}
	if q.Kind != "forall" {
		t.Errorf("kind: want forall, got %s", q.Kind)
	}
	if q.Sort != Int {
		t.Errorf("sort: want Int")
	}
	if _, ok := q.Body.(atomicFormula); !ok {
		t.Errorf("body: want atomicFormula, got %T", q.Body)
	}
}

func TestExistsReturnsFlatQuantifier(t *testing.T) {
	ResetCollector()
	BeginCollecting()
	defer ResetCollector()

	f := Exists(String, func(s IrTerm) IrFormula {
		return Eq(ParseInt(s), Num(0))
	})
	q, ok := f.(quantFormula)
	if !ok {
		t.Fatalf("want quantFormula, got %T", f)
	}
	if q.Kind != "exists" {
		t.Errorf("kind: want exists, got %s", q.Kind)
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

	q1 := f1.(quantFormula)
	q2 := f2.(quantFormula)

	if q1.Name != "_x0" {
		t.Errorf("first run name: want _x0, got %s", q1.Name)
	}
	if q2.Name != "_x0" {
		t.Errorf("second run name: want _x0 (counter reset), got %s", q2.Name)
	}
}
