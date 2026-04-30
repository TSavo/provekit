package ir

import (
	"encoding/json"
	"testing"
)

func TestPrimitiveSortMarshalJSON(t *testing.T) {
	cases := []struct {
		sort Sort
		want string
	}{
		{Bool, `{"kind":"primitive","name":"Bool"}`},
		{Int, `{"kind":"primitive","name":"Int"}`},
		{Real, `{"kind":"primitive","name":"Real"}`},
		{String, `{"kind":"primitive","name":"String"}`},
		{Ref, `{"kind":"primitive","name":"Ref"}`},
		{Node, `{"kind":"primitive","name":"Node"}`},
		{Edge, `{"kind":"primitive","name":"Edge"}`},
	}
	for _, c := range cases {
		got, err := json.Marshal(c.sort)
		if err != nil {
			t.Fatalf("marshal error: %v", err)
		}
		if string(got) != c.want {
			t.Errorf("got %s, want %s", got, c.want)
		}
	}
}

func TestSetOf(t *testing.T) {
	got, err := json.Marshal(SetOf(Int))
	if err != nil {
		t.Fatalf("marshal error: %v", err)
	}
	want := `{"kind":"set","element":{"kind":"primitive","name":"Int"}}`
	if string(got) != want {
		t.Errorf("got %s, want %s", got, want)
	}
}

func TestTupleOf(t *testing.T) {
	got, err := json.Marshal(TupleOf(Int, String))
	if err != nil {
		t.Fatalf("marshal error: %v", err)
	}
	want := `{"kind":"tuple","elements":[{"kind":"primitive","name":"Int"},{"kind":"primitive","name":"String"}]}`
	if string(got) != want {
		t.Errorf("got %s, want %s", got, want)
	}
}

func TestFuncOf(t *testing.T) {
	got, err := json.Marshal(FuncOf([]Sort{Int, Int}, Bool))
	if err != nil {
		t.Fatalf("marshal error: %v", err)
	}
	want := `{"kind":"function","domain":[{"kind":"primitive","name":"Int"},{"kind":"primitive","name":"Int"}],"range":{"kind":"primitive","name":"Bool"}}`
	if string(got) != want {
		t.Errorf("got %s, want %s", got, want)
	}
}

func TestVarTermMarshalDropsSort(t *testing.T) {
	v := varTerm{Name: "_x0", Sort: Int}
	got, err := json.Marshal(v)
	if err != nil {
		t.Fatalf("marshal error: %v", err)
	}
	want := `{"kind":"var","name":"_x0"}`
	if string(got) != want {
		t.Errorf("got %s, want %s", got, want)
	}
}

func TestConstTermMarshalKeepsSort(t *testing.T) {
	got, err := json.Marshal(Num(42))
	if err != nil {
		t.Fatalf("marshal error: %v", err)
	}
	want := `{"kind":"const","value":42,"sort":{"kind":"primitive","name":"Int"}}`
	if string(got) != want {
		t.Errorf("got %s, want %s", got, want)
	}
}

func TestCtorTermMarshalDropsSort(t *testing.T) {
	got, err := json.Marshal(ParseInt(StrConst("0")))
	if err != nil {
		t.Fatalf("marshal error: %v", err)
	}
	want := `{"kind":"ctor","name":"parseInt","args":[{"kind":"const","value":"0","sort":{"kind":"primitive","name":"String"}}]}`
	if string(got) != want {
		t.Errorf("got %s, want %s", got, want)
	}
}

func TestAtomicFormulaMarshalUsesName(t *testing.T) {
	got, err := json.Marshal(Eq(Num(0), Num(0)))
	if err != nil {
		t.Fatalf("marshal error: %v", err)
	}
	want := `{"kind":"atomic","name":"=","args":[{"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}},{"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}}]}`
	if string(got) != want {
		t.Errorf("got %s, want %s", got, want)
	}
}

func TestConnectivesMarshalUseOperands(t *testing.T) {
	a := Eq(Num(0), Num(0))
	b := Eq(Num(1), Num(1))

	notF, err := json.Marshal(Not(a))
	if err != nil {
		t.Fatalf("marshal error: %v", err)
	}
	wantNot := `{"kind":"not","operands":[{"kind":"atomic","name":"=","args":[{"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}},{"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}}]}]}`
	if string(notF) != wantNot {
		t.Errorf("not:\n  got:  %s\n  want: %s", notF, wantNot)
	}

	andF, err := json.Marshal(And(a, b))
	if err != nil {
		t.Fatalf("marshal error: %v", err)
	}
	var roundtrip map[string]any
	if err := json.Unmarshal(andF, &roundtrip); err != nil {
		t.Fatalf("not valid JSON: %v", err)
	}
	if roundtrip["kind"] != "and" {
		t.Errorf("expected kind=and, got %v", roundtrip["kind"])
	}
	if _, ok := roundtrip["operands"].([]any); !ok {
		t.Errorf("expected operands array on and, got %T", roundtrip["operands"])
	}

	impF, err := json.Marshal(Implies(a, b))
	if err != nil {
		t.Fatalf("marshal error: %v", err)
	}
	if err := json.Unmarshal(impF, &roundtrip); err != nil {
		t.Fatalf("not valid JSON: %v", err)
	}
	if roundtrip["kind"] != "implies" {
		t.Errorf("expected kind=implies, got %v", roundtrip["kind"])
	}
	ops, ok := roundtrip["operands"].([]any)
	if !ok {
		t.Fatalf("expected operands array on implies, got %T", roundtrip["operands"])
	}
	if len(ops) != 2 {
		t.Errorf("implies operands: want 2 (antecedent, consequent), got %d", len(ops))
	}
}

func TestQuantifierFormulaMarshalIsFlat(t *testing.T) {
	ResetCollector()
	BeginCollecting()
	defer ResetCollector()

	f := ForAll(Int, func(x IrTerm) IrFormula {
		return Gt(x, Num(0))
	})
	// Use the kit's non-escaping encoder (encodeJSON) to match what the
	// canonicalizer feeds into JCS. stdlib json.Marshal would re-escape `>`.
	got, err := encodeJSON(f)
	if err != nil {
		t.Fatalf("marshal error: %v", err)
	}
	want := `{"kind":"forall","name":"_x0","sort":{"kind":"primitive","name":"Int"},"body":{"kind":"atomic","name":">","args":[{"kind":"var","name":"_x0"},{"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}}]}}`
	if string(got) != want {
		t.Errorf("flat quantifier:\n  got:  %s\n  want: %s", got, want)
	}
}
