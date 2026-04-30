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

func TestVarTermMarshal(t *testing.T) {
	v := varTerm{Name: "_x0", Sort: Int}
	got, err := json.Marshal(v)
	if err != nil {
		t.Fatalf("marshal error: %v", err)
	}
	want := `{"kind":"var","name":"_x0","sort":{"kind":"primitive","name":"Int"}}`
	if string(got) != want {
		t.Errorf("got %s, want %s", got, want)
	}
}

func TestConstTermMarshal(t *testing.T) {
	got, err := json.Marshal(Num(42))
	if err != nil {
		t.Fatalf("marshal error: %v", err)
	}
	want := `{"kind":"const","value":42,"sort":{"kind":"primitive","name":"Int"}}`
	if string(got) != want {
		t.Errorf("got %s, want %s", got, want)
	}
}

func TestCtorTermMarshal(t *testing.T) {
	got, err := json.Marshal(ParseInt(StrConst("0")))
	if err != nil {
		t.Fatalf("marshal error: %v", err)
	}
	want := `{"kind":"ctor","name":"parseInt","args":[{"kind":"const","value":"0","sort":{"kind":"primitive","name":"String"}}],"sort":{"kind":"primitive","name":"Int"}}`
	if string(got) != want {
		t.Errorf("got %s, want %s", got, want)
	}
}

func TestAtomicFormulaMarshal(t *testing.T) {
	got, err := json.Marshal(Eq(Num(0), Num(0)))
	if err != nil {
		t.Fatalf("marshal error: %v", err)
	}
	want := `{"kind":"atomic","predicate":"=","args":[{"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}},{"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}}]}`
	if string(got) != want {
		t.Errorf("got %s, want %s", got, want)
	}
}

func TestConnectivesMarshal(t *testing.T) {
	a := Eq(Num(0), Num(0))
	b := Eq(Num(1), Num(1))

	notF, err := json.Marshal(Not(a))
	if err != nil {
		t.Fatalf("marshal error: %v", err)
	}
	if string(notF) != `{"kind":"not","body":{"kind":"atomic","predicate":"=","args":[{"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}},{"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}}]}}` {
		t.Errorf("not: %s", notF)
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
}
