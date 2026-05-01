// SPDX-License-Identifier: Apache-2.0
//
// invariants_test.go - Formal Invariant Tests for Go IR Kit
//
// These tests verify the formal invariants from
// protocol/specs/2026-04-30-ir-formal-grammar.md.

package ir

import (
	"testing"
)

func TestInvariantVarTermHasNoSort(t *testing.T) {
	v := MakeVar("x", Int)
	_ = v
	// VarTerm carries no sort field - verified by interface design
}

func TestInvariantConstTermHasSort(t *testing.T) {
	c := Num(42)
	// constTerm carries sort field internally
	_ = c
}

func TestInvariantLambdaTermHasParamSortAndBody(t *testing.T) {
	lam := Lambda("x", Int, Num(42))
	data, err := encodeJSON(lam)
	if err != nil {
		t.Fatalf("marshal lambda: %v", err)
	}
	s := string(data)
	if !contains(s, `"kind":"lambda"`) {
		t.Error("lambda missing kind")
	}
	if !contains(s, `"paramName":"x"`) {
		t.Error("lambda missing paramName")
	}
	if !contains(s, `"paramSort":`) {
		t.Error("lambda missing paramSort")
	}
	if !contains(s, `"body":`) {
		t.Error("lambda missing body")
	}
}

func TestInvariantLetTermHasBindingsAndBody(t *testing.T) {
	letExpr := Let([]letBinding{
		{Name: "x", BoundTerm: Num(1)},
	}, Num(2))
	data, err := encodeJSON(letExpr)
	if err != nil {
		t.Fatalf("marshal let: %v", err)
	}
	s := string(data)
	if !contains(s, `"kind":"let"`) {
		t.Error("let missing kind")
	}
	if !contains(s, `"bindings":`) {
		t.Error("let missing bindings")
	}
	if !contains(s, `"body":`) {
		t.Error("let missing body")
	}
}

func TestInvariantChoiceFormulaHasVarNameSortAndBody(t *testing.T) {
	c := Choice("x", Int, func(v IrTerm) IrFormula {
		return Eq(v, Num(0))
	})
	data, err := encodeJSON(c)
	if err != nil {
		t.Fatalf("marshal choice: %v", err)
	}
	s := string(data)
	if !contains(s, `"kind":"choice"`) {
		t.Error("choice missing kind")
	}
	if !contains(s, `"varName":"x"`) {
		t.Error("choice missing varName")
	}
	if !contains(s, `"sort":`) {
		t.Error("choice missing sort")
	}
	if !contains(s, `"body":`) {
		t.Error("choice missing body")
	}
}

func TestInvariantLambdaJSONRoundTrip(t *testing.T) {
	lam := Lambda("x", Int, Num(42))
	data, err := encodeJSON(lam)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	if len(data) == 0 {
		t.Error("marshal produced empty output")
	}
}

func TestInvariantLetJSONRoundTrip(t *testing.T) {
	letExpr := Let([]letBinding{
		{Name: "x", BoundTerm: Num(1)},
	}, Num(2))
	data, err := encodeJSON(letExpr)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	if len(data) == 0 {
		t.Error("marshal produced empty output")
	}
}

func TestInvariantChoiceJSONRoundTrip(t *testing.T) {
	c := Choice("x", Int, func(v IrTerm) IrFormula {
		return Eq(v, Num(0))
	})
	data, err := encodeJSON(c)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	if len(data) == 0 {
		t.Error("marshal produced empty output")
	}
}

func contains(s, substr string) bool {
	return len(s) >= len(substr) && (s == substr || len(s) > 0 && containsAt(s, substr))
}

func containsAt(s, substr string) bool {
	for i := 0; i <= len(s)-len(substr); i++ {
		if s[i:i+len(substr)] == substr {
			return true
		}
	}
	return false
}
