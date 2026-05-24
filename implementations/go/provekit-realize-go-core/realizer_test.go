package realizego

import (
	"strings"
	"testing"
)

func TestRealizeIdentityEmitsCompilableSignature(t *testing.T) {
	got, err := Realize(RealizeRequest{
		Function:    "Id",
		Params:      []string{"x"},
		ParamTypes:  []string{"int"},
		ReturnType:  "int",
		ConceptName: "identity",
	})
	if err != nil {
		t.Fatalf("Realize identity: %v", err)
	}
	if got.IsStub {
		t.Fatal("identity must not be a stub")
	}
	if got.Extension != "go" {
		t.Fatalf("extension = %q, want go", got.Extension)
	}
	if !strings.Contains(got.Source, "func Id(x int) int") {
		t.Fatalf("source missing signature: %s", got.Source)
	}
	if !strings.Contains(got.Source, "return x") {
		t.Fatalf("identity body must be `return x`: %s", got.Source)
	}
}

// Discrimination: an unsupported concept is refused with MissingTemplateError,
// never silently stubbed.
func TestRealizeUnsupportedConceptRefuses(t *testing.T) {
	_, err := Realize(RealizeRequest{
		Function:    "F",
		Params:      []string{"x"},
		ConceptName: "concept:not-supported",
	})
	if err == nil {
		t.Fatal("unsupported concept must be refused")
	}
	if _, ok := err.(*MissingTemplateError); !ok {
		t.Fatalf("want *MissingTemplateError, got %T: %v", err, err)
	}
}

// Discrimination: identity's signature guard rejects a wrong param count
// (2 params) rather than emitting a malformed body.
func TestRealizeIdentityRejectsWrongArity(t *testing.T) {
	_, err := Realize(RealizeRequest{
		Function:    "Id2",
		Params:      []string{"x", "y"},
		ConceptName: "identity",
	})
	if err == nil {
		t.Fatal("identity with 2 params must be refused (max_params=1)")
	}
}

// Structural: substitute fills ${paramN} placeholders positionally.
func TestSubstitutePositional(t *testing.T) {
	out, err := substitute("return ${param0}", []string{"value"})
	if err != nil {
		t.Fatalf("substitute: %v", err)
	}
	if out != "return value" {
		t.Fatalf("substitute = %q, want `return value`", out)
	}
}
