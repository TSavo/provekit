package ir

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestForAllNamedPreservesBoundName(t *testing.T) {
	resetQuantifierCounter()
	f := ForAllNamed("x", Int, func(x IrTerm) IrFormula {
		return Gte(x, Num(0))
	})
	b, err := json.Marshal(f)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	js := string(b)
	if !strings.Contains(js, `"kind":"forall"`) {
		t.Fatalf("expected forall kind in %s", js)
	}
	if !strings.Contains(js, `"name":"x"`) {
		t.Fatalf("expected verbatim bound name in %s", js)
	}
	// Confirm the bound name is NOT the auto-named placeholder.
	if strings.Contains(js, `"name":"_x0"`) {
		t.Fatalf("ForAllNamed should NOT use the auto-named placeholder: %s", js)
	}
}

func TestExistsNamedPreservesBoundName(t *testing.T) {
	resetQuantifierCounter()
	f := ExistsNamed("y", Int, func(y IrTerm) IrFormula {
		return Eq(y, Num(42))
	})
	b, _ := json.Marshal(f)
	if !strings.Contains(string(b), `"kind":"exists"`) || !strings.Contains(string(b), `"name":"y"`) {
		t.Fatalf("ExistsNamed shape wrong: %s", string(b))
	}
}

func TestMakeVarPreservesNameInJSON(t *testing.T) {
	v := MakeVar("loopVar", Int)
	b, _ := json.Marshal(v)
	if string(b) != `{"kind":"var","name":"loopVar"}` {
		t.Fatalf("MakeVar JSON wrong: %s", string(b))
	}
}

func TestMakeCtorEmitsCanonicalCtor(t *testing.T) {
	c := MakeCtor("parseInt", []IrTerm{StrConst("42")}, Int)
	b, _ := json.Marshal(c)
	want := `{"kind":"ctor","name":"parseInt","args":[{"kind":"const","value":"42","sort":{"kind":"primitive","name":"String"}}]}`
	if string(b) != want {
		t.Fatalf("MakeCtor JSON wrong:\n got: %s\nwant: %s", string(b), want)
	}
}
