package validator

import (
	"encoding/json"
	"testing"

	ir "github.com/tsavo/provekit/go/provekit-ir-symbolic/ir"
)

type TestUser struct {
	Name  string `validate:"required,min=1,max=100"`
	Age   int    `validate:"gte=0,lte=150"`
	Email string `validate:"required,email"`
	Role  string `validate:"oneof=admin editor viewer"`
	Bio   string `validate:"len=200"`
}

func TestLiftStruct(t *testing.T) {
	decls := LiftStruct(TestUser{})

	if len(decls) != 5 {
		t.Fatalf("expected 5 declarations, got %d", len(decls))
	}

	names := make(map[string]bool)
	for _, d := range decls {
		names[d.DeclName()] = true
	}

	for _, want := range []string{
		"TestUser.Name",
		"TestUser.Age",
		"TestUser.Email",
		"TestUser.Role",
		"TestUser.Bio",
	} {
		if !names[want] {
			t.Errorf("missing declaration for %s", want)
		}
	}
}

func TestLiftStruct_RangeConstraintByteEquivalent(t *testing.T) {
	// Same constraint as @Min(0) @Max(100) in Java Bean Validation
	// and pydantic Field(ge=0, le=100) in Python.
	type Score struct {
		Value int `validate:"gte=0,lte=100"`
	}

	decls := LiftStruct(Score{})
	if len(decls) != 1 {
		t.Fatalf("expected 1 declaration, got %d", len(decls))
	}

	d := decls[0].(ir.ContractDeclaration)
	if d.Pre == nil {
		t.Fatal("expected precondition, got nil")
	}

	jcs, err := json.Marshal(d.Pre)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}

	// Golden JCS: the same IR formula structure as Bean Validation @Min(0) @Max(100)
	// and JML //@ requires score >= 0 && score <= 100.
	// Note: Go kit uses logical key order (kind, name, args); JCS-alpha sort
	// is tracked separately in the Go kit canonicalizer.
	expected := `{"kind":"and","operands":[{"kind":"atomic","name":"≥","args":[{"kind":"var","name":"Value"},{"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}}]},{"kind":"atomic","name":"≤","args":[{"kind":"var","name":"Value"},{"kind":"const","value":100,"sort":{"kind":"primitive","name":"Int"}}]}]}`

	if string(jcs) != expected {
		t.Errorf("JCS mismatch:\n  got:      %s\n  expected: %s", jcs, expected)
	}
}

func TestLiftStruct_StringRequiredNotNullEquivalent(t *testing.T) {
	// Same constraint as @NotNull on a String field in Bean Validation.
	// Go's "required" maps to neq(var, "") rather than neq(var, null)
	// since Go has no null concept for value types.
	type Input struct {
		Name string `validate:"required"`
	}

	decls := LiftStruct(Input{})
	if len(decls) != 1 {
		t.Fatalf("expected 1 declaration, got %d", len(decls))
	}

	d := decls[0].(ir.ContractDeclaration)
	jcs, err := json.Marshal(d.Pre)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}

	expected := `{"kind":"atomic","name":"≠","args":[{"kind":"var","name":"Name"},{"kind":"const","value":"","sort":{"kind":"primitive","name":"String"}}]}`

	if string(jcs) != expected {
		t.Errorf("JCS mismatch:\n  got:      %s\n  expected: %s", jcs, expected)
	}
}

func TestLiftStruct_MinMaxStringLength(t *testing.T) {
	// min/max on a string field maps to strlen >= N and strlen <= N.
	type Entry struct {
		Title string `validate:"min=1,max=200"`
	}

	decls := LiftStruct(Entry{})
	if len(decls) != 1 {
		t.Fatalf("expected 1 declaration, got %d", len(decls))
	}

	d := decls[0].(ir.ContractDeclaration)
	jcs, err := json.Marshal(d.Pre)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}

	expected := `{"kind":"and","operands":[{"kind":"atomic","name":"≥","args":[{"kind":"ctor","name":"String.prototype.length","args":[{"kind":"var","name":"Title"}]},{"kind":"const","value":1,"sort":{"kind":"primitive","name":"Int"}}]},{"kind":"atomic","name":"≤","args":[{"kind":"ctor","name":"String.prototype.length","args":[{"kind":"var","name":"Title"}]},{"kind":"const","value":200,"sort":{"kind":"primitive","name":"Int"}}]}]}`

	if string(jcs) != expected {
		t.Errorf("JCS mismatch:\n  got:      %s\n  expected: %s", jcs, expected)
	}
}

func TestLiftStruct_EmptyStruct(t *testing.T) {
	type Empty struct{}
	decls := LiftStruct(Empty{})
	if len(decls) != 0 {
		t.Errorf("expected 0 declarations for empty struct, got %d", len(decls))
	}
}

func TestLiftStruct_NoTags(t *testing.T) {
	type Plain struct {
		Field string
	}
	decls := LiftStruct(Plain{})
	if len(decls) != 0 {
		t.Errorf("expected 0 declarations for untagged struct, got %d", len(decls))
	}
}

func TestLiftStruct_OneofConstraint(t *testing.T) {
	type Choice struct {
		Option string `validate:"oneof=a b c"`
	}

	decls := LiftStruct(Choice{})
	if len(decls) != 1 {
		t.Fatalf("expected 1 declaration, got %d", len(decls))
	}

	d := decls[0].(ir.ContractDeclaration)
	jcs, err := json.Marshal(d.Pre)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}

	expected := `{"kind":"or","operands":[{"kind":"atomic","name":"=","args":[{"kind":"var","name":"Option"},{"kind":"const","value":"a","sort":{"kind":"primitive","name":"String"}}]},{"kind":"atomic","name":"=","args":[{"kind":"var","name":"Option"},{"kind":"const","value":"b","sort":{"kind":"primitive","name":"String"}}]},{"kind":"atomic","name":"=","args":[{"kind":"var","name":"Option"},{"kind":"const","value":"c","sort":{"kind":"primitive","name":"String"}}]}]}`

	if string(jcs) != expected {
		t.Errorf("JCS mismatch:\n  got:      %s\n  expected: %s", jcs, expected)
	}
}
