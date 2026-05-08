package main

import (
	"encoding/json"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

func unwrapCheckPositiveEntry() BaselineEntry {
	return NewBaselineEntry(
		"checkPositive",
		&Post{Constraints: []string{"x > 0"}},
		&Post{Constraints: []string{"returns true"}},
	)
}

func consumeReturnEntry() BaselineEntry {
	return NewBaselineEntry(
		"consumeReturn",
		&Post{Constraints: []string{"returns true"}},
		&Post{},
	)
}

func callCheckPositive() ForwardStmt {
	return ForwardStmt{
		Kind:     ForwardStmtCall,
		CalleeID: "checkPositive",
		Range:    SingleLineRange(4, 12, 25),
	}
}

func callConsumeReturn() ForwardStmt {
	return ForwardStmt{
		Kind:     ForwardStmtCall,
		CalleeID: "consumeReturn",
		Range:    SingleLineRange(5, 12, 25),
	}
}

func TestCallsiteSatisfiesPreNoDiagnostic(t *testing.T) {
	propagator := NewForwardPropagator([]BaselineEntry{unwrapCheckPositiveEntry()})
	body := []ForwardStmt{
		{Kind: ForwardStmtAssign, Post: Post{Constraints: []string{"x > 0", "caller kept an extra fact"}}},
		callCheckPositive(),
	}

	diagnostics := propagator.EmitDiagnostics(body)

	if len(diagnostics) != 0 {
		t.Fatalf("extra caller facts should still imply the callee precondition: %#v", diagnostics)
	}
}

func TestCallsiteViolatesPreDiagnosticEmitted(t *testing.T) {
	propagator := NewForwardPropagator([]BaselineEntry{unwrapCheckPositiveEntry()})
	body := []ForwardStmt{
		{Kind: ForwardStmtAssign, Post: Post{Constraints: []string{"x <= 0"}}},
		callCheckPositive(),
	}

	diagnostics := propagator.EmitDiagnostics(body)

	if len(diagnostics) != 1 {
		t.Fatalf("expected one diagnostic, got %#v", diagnostics)
	}
	diagnostic := diagnostics[0]
	if diagnostic.Code != "implication-failed" {
		t.Fatalf("code = %q, want implication-failed", diagnostic.Code)
	}
	if diagnostic.Source != "provekit" {
		t.Fatalf("source = %q, want provekit", diagnostic.Source)
	}
	if diagnostic.Severity != 1 {
		t.Fatalf("severity = %d, want 1", diagnostic.Severity)
	}
	if diagnostic.Data.Callee != "checkPositive" {
		t.Fatalf("callee = %q, want checkPositive", diagnostic.Data.Callee)
	}
	if got := strings.Join(diagnostic.Data.MissingConjuncts, ","); got != "x > 0" {
		t.Fatalf("missing conjuncts = %q, want x > 0", got)
	}
	if !strings.HasPrefix(diagnostic.Data.CurrentPostCID, "blake3-512:") {
		t.Fatalf("current_post_cid missing prefix: %q", diagnostic.Data.CurrentPostCID)
	}
	if !strings.HasPrefix(diagnostic.Data.BaselineIndexCID, "blake3-512:") {
		t.Fatalf("baseline_index_cid missing prefix: %q", diagnostic.Data.BaselineIndexCID)
	}
}

func TestBranchMergePartialSatisfaction(t *testing.T) {
	propagator := NewForwardPropagator([]BaselineEntry{unwrapCheckPositiveEntry()})
	body := []ForwardStmt{
		{
			Kind: ForwardStmtIfElse,
			ThenBranch: []ForwardStmt{
				{Kind: ForwardStmtAssign, Post: Post{Constraints: []string{"x > 0"}}},
			},
			ElseBranch: []ForwardStmt{
				{Kind: ForwardStmtAssign, Post: Post{}},
			},
		},
		callCheckPositive(),
	}

	diagnostics := propagator.EmitDiagnostics(body)

	if len(diagnostics) != 1 {
		t.Fatalf("expected one diagnostic on join path, got %#v", diagnostics)
	}
	if got := strings.Join(diagnostics[0].Data.MissingConjuncts, ","); got != "x > 0" {
		t.Fatalf("missing conjuncts = %q, want x > 0", got)
	}
}

func TestTopFallbackSuppressesFalsePositive(t *testing.T) {
	propagator := NewForwardPropagator([]BaselineEntry{unwrapCheckPositiveEntry()})
	body := []ForwardStmt{
		{Kind: ForwardStmtUnsupported},
		callCheckPositive(),
	}

	diagnostics := propagator.EmitDiagnostics(body)

	if len(diagnostics) != 0 {
		t.Fatalf("top fallback must suppress implication-failed diagnostics: %#v", diagnostics)
	}
}

func TestFailedPreconditionDoesNotPropagateCalleePostcondition(t *testing.T) {
	propagator := NewForwardPropagator([]BaselineEntry{unwrapCheckPositiveEntry(), consumeReturnEntry()})
	body := []ForwardStmt{
		{Kind: ForwardStmtAssign, Post: Post{Constraints: []string{"x <= 0"}}},
		callCheckPositive(),
		callConsumeReturn(),
	}

	diagnostics := propagator.EmitDiagnostics(body)

	if len(diagnostics) != 2 {
		t.Fatalf("expected both failed preconditions to diagnose, got %#v", diagnostics)
	}
	if diagnostics[0].Data.Callee != "checkPositive" {
		t.Fatalf("first callee = %q, want checkPositive", diagnostics[0].Data.Callee)
	}
	if diagnostics[1].Data.Callee != "consumeReturn" {
		t.Fatalf("second callee = %q, want consumeReturn", diagnostics[1].Data.Callee)
	}
}

func TestLowerFloorSourceResetsOnGoMethods(t *testing.T) {
	source := `
func establishesFact() {
	checkPositive(5)
}

func (s Service) publicViolates() {
	checkPositive(-1)
}
`
	diagnostics := FloorV1SeedForwardPropagator().EmitDiagnostics(LowerFloorSource(source))

	if len(diagnostics) != 1 {
		t.Fatalf("expected qualified method body to reset state and diagnose, got %#v", diagnostics)
	}
}

func TestLowerFloorSourceIgnoresNonCodeCheckPositiveText(t *testing.T) {
	source := `
func noFalseCalls() {
	// checkPositive(-1)
	_ = "checkPositive(-1)"
	_ = ` + "`checkPositive(-1)`" + `
	notcheckPositive(-1)
}
`
	diagnostics := FloorV1SeedForwardPropagator().EmitDiagnostics(LowerFloorSource(source))

	if len(diagnostics) != 0 {
		t.Fatalf("expected comments, strings, and longer identifiers to be ignored, got %#v", diagnostics)
	}
}

func TestParseFloorFixtureEmitsForwardPropagationDiagnostic(t *testing.T) {
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("runtime.Caller failed")
	}
	root := filepath.Clean(filepath.Join(filepath.Dir(file), "../../../.."))
	fixture := filepath.Join(root, "tests/lsp/floor-fixture/go.go")
	source, err := os.ReadFile(fixture)
	if err != nil {
		t.Fatalf("read fixture: %v", err)
	}

	done := capture()
	msg := json.RawMessage(mustMarshal(parseParams{Path: "go.go", Source: string(source)}))
	handleRequest(mustMarshal(rpcRequest{JSONRPC: "2.0", ID: 30.0, Method: "parse", Params: msg}))
	resp := done()
	if resp.Error != nil {
		t.Fatalf("parse error: %s", resp.Error.Message)
	}
	m := resultMap(resp)
	diagnosticsRaw, ok := m["diagnostics"]
	if !ok {
		t.Fatal("diagnostics field missing from parse result")
	}
	diagnostics, ok := diagnosticsRaw.([]interface{})
	if !ok {
		t.Fatalf("diagnostics not a list: %T", diagnosticsRaw)
	}
	if len(diagnostics) != 1 {
		t.Fatalf("expected one diagnostic, got %#v", diagnostics)
	}
	diagnostic, ok := diagnostics[0].(map[string]interface{})
	if !ok {
		t.Fatalf("diagnostic not an object: %T", diagnostics[0])
	}
	if diagnostic["code"] != "implication-failed" {
		t.Fatalf("code = %v, want implication-failed", diagnostic["code"])
	}
	data, ok := diagnostic["data"].(map[string]interface{})
	if !ok {
		t.Fatalf("diagnostic data not an object: %T", diagnostic["data"])
	}
	if data["kind"] != "provekit.lsp.implication_failed" {
		t.Fatalf("kind = %v, want provekit.lsp.implication_failed", data["kind"])
	}
	if data["callee"] != "checkPositive" {
		t.Fatalf("callee = %v, want checkPositive", data["callee"])
	}
}
