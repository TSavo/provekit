package ir

import (
	"encoding/json"
	"testing"
)

func TestNumIsInt(t *testing.T) {
	tm := Num(42)
	if tm.TermSort() != Int {
		t.Errorf("Num sort: want Int, got %v", tm.TermSort())
	}
}

func TestRealConstIsReal(t *testing.T) {
	tm := RealConst(3.14)
	if tm.TermSort() != Real {
		t.Errorf("RealConst sort: want Real, got %v", tm.TermSort())
	}
}

func TestStrConstIsString(t *testing.T) {
	tm := StrConst("hello")
	if tm.TermSort() != String {
		t.Errorf("StrConst sort: want String, got %v", tm.TermSort())
	}
}

func TestBoolConstIsBool(t *testing.T) {
	tm := BoolConst(true)
	if tm.TermSort() != Bool {
		t.Errorf("BoolConst sort: want Bool, got %v", tm.TermSort())
	}
}

func TestParseIntReturnsInt(t *testing.T) {
	tm := ParseInt(StrConst("42"))
	if tm.TermSort() != Int {
		t.Errorf("ParseInt sort: want Int, got %v", tm.TermSort())
	}
	ct, ok := tm.(ctorTerm)
	if !ok {
		t.Fatalf("ParseInt: want ctorTerm, got %T", tm)
	}
	if ct.Name != "parseInt" {
		t.Errorf("ctor name: want parseInt, got %s", ct.Name)
	}
}

func TestParseFloatReturnsReal(t *testing.T) {
	tm := ParseFloat(StrConst("3.14"))
	if tm.TermSort() != Real {
		t.Errorf("ParseFloat sort: want Real, got %v", tm.TermSort())
	}
}

func TestPredicatesReturnBoolTermSort(t *testing.T) {
	for name, tm := range map[string]IrTerm{
		"isNaN":     IsNaN(Num(0)),
		"isFinite":  IsFinite(Num(0)),
		"isInteger": IsInteger(Num(0)),
	} {
		if tm.TermSort() != Bool {
			t.Errorf("%s sort: want Bool, got %v", name, tm.TermSort())
		}
	}
}

func TestAbsPreservesInputSort(t *testing.T) {
	if Abs(Num(-3)).TermSort() != Int {
		t.Errorf("Abs(int): want Int")
	}
	if Abs(RealConst(-3.0)).TermSort() != Real {
		t.Errorf("Abs(real): want Real")
	}
}

func TestMaxMinPreserveFirstArgSort(t *testing.T) {
	if Max(Num(1), Num(2)).TermSort() != Int {
		t.Errorf("Max(int, int): want Int")
	}
	if Min(RealConst(1.5), RealConst(2.5)).TermSort() != Real {
		t.Errorf("Min(real, real): want Real")
	}
}

func TestFloorCeilSignReturnInt(t *testing.T) {
	if Floor(RealConst(1.5)).TermSort() != Int {
		t.Errorf("Floor: want Int")
	}
	if Ceil(RealConst(1.5)).TermSort() != Int {
		t.Errorf("Ceil: want Int")
	}
	if Sign(Num(-3)).TermSort() != Int {
		t.Errorf("Sign: want Int")
	}
}

func TestSqrtReturnsReal(t *testing.T) {
	if Sqrt(Num(4)).TermSort() != Real {
		t.Errorf("Sqrt: want Real")
	}
}

func TestArithmeticCtors(t *testing.T) {
	cases := []struct {
		tm   IrTerm
		name string
		sort Sort
	}{
		{Add(Num(1), Num(2)), "+", Int},
		{Sub(Num(5), Num(3)), "-", Int},
		{Mul(Num(2), Num(4)), "*", Int},
		{Div(Num(1), Num(2)), "/", Real},
		{Neg(Num(5)), "-", Int},
	}
	for _, c := range cases {
		ct, ok := c.tm.(ctorTerm)
		if !ok {
			t.Errorf("%s: want ctorTerm, got %T", c.name, c.tm)
			continue
		}
		if ct.Name != c.name {
			t.Errorf("name: want %s, got %s", c.name, ct.Name)
		}
		if c.tm.TermSort() != c.sort {
			t.Errorf("%s sort: want %v, got %v", c.name, c.sort, c.tm.TermSort())
		}
	}
}

func TestAtomicPredicateNames(t *testing.T) {
	cases := []struct {
		formula IrFormula
		want    string
	}{
		{Eq(Num(0), Num(1)), "="},
		{Neq(Num(0), Num(1)), "≠"},
		{Lt(Num(0), Num(1)), "<"},
		{Lte(Num(0), Num(1)), "≤"},
		{Gt(Num(0), Num(1)), ">"},
		{Gte(Num(0), Num(1)), "≥"},
		{IsTrue(BoolConst(true)), "true"},
		{IsFalse(BoolConst(false)), "false"},
	}
	for _, c := range cases {
		af, ok := c.formula.(atomicFormula)
		if !ok {
			t.Errorf("want atomicFormula for %s, got %T", c.want, c.formula)
			continue
		}
		if af.Name != c.want {
			t.Errorf("name: want %s, got %s", c.want, af.Name)
		}
	}
}

func TestStringArrayPrimitives(t *testing.T) {
	if StringLength(StrConst("hi")).TermSort() != Int {
		t.Errorf("StringLength: want Int")
	}
	if StringIncludes(StrConst("hi"), StrConst("h")).TermSort() != Bool {
		t.Errorf("StringIncludes: want Bool")
	}
	if ArrayLength(StrConst("[]")).TermSort() != Int {
		t.Errorf("ArrayLength: want Int")
	}
	if ArrayIncludes(StrConst("[]"), Num(0)).TermSort() != Bool {
		t.Errorf("ArrayIncludes: want Bool")
	}
}

func TestAddJSONShape(t *testing.T) {
	got, err := json.Marshal(Add(Num(2), Num(3)))
	if err != nil {
		t.Fatalf("marshal error: %v", err)
	}
	// v1.1.0: ctor drops `sort` from JSON.
	want := `{"args":[{"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":2},{"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":3}],"kind":"ctor","name":"+"}`
	if string(got) != want {
		t.Errorf("got %s, want %s", got, want)
	}
}
