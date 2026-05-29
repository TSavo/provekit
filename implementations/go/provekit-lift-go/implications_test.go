package liftgo

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestRPCLiftImplicationsEmitsBridgeForMatchedGoCall(t *testing.T) {
	root := t.TempDir()
	src := `package sample

func Caller(x int) int {
	return Callee(x)
}

func Callee(x int) int {
	return x + 1
}
`
	if err := os.WriteFile(filepath.Join(root, "sample.go"), []byte(src), 0o644); err != nil {
		t.Fatalf("write sample.go: %v", err)
	}

	stdin := strings.NewReader(`{"jsonrpc":"2.0","id":1,"method":"provekit.plugin.lift_implications","params":{"workspace_root":` + strconvQuote(root) + `,"source_paths":["sample.go"],"contract_bindings":[{"name":"example.com/sample.Callee","contract_cid":"blake3-512:abc"}]}}` + "\n")
	var stdout bytes.Buffer
	if err := RunRPC(stdin, &stdout); err != nil {
		t.Fatalf("RunRPC: %v", err)
	}

	var response map[string]any
	if err := json.Unmarshal(bytes.TrimSpace(stdout.Bytes()), &response); err != nil {
		t.Fatalf("response JSON parses: %v\nstdout: %s", err, stdout.String())
	}
	if response["error"] != nil {
		t.Fatalf("lift_implications RPC returned error: %v", response["error"])
	}
	result := response["result"].(map[string]any)
	ir := result["ir"].([]any)
	if len(ir) != 1 {
		t.Fatalf("expected one bridge for Callee call, got %d: %#v", len(ir), ir)
	}
	bridge := ir[0].(map[string]any)
	if bridge["kind"] != "bridge" {
		t.Fatalf("kind = %#v, want bridge", bridge["kind"])
	}
	if bridge["sourceSymbol"] != "Callee" {
		t.Fatalf("sourceSymbol = %#v, want Callee", bridge["sourceSymbol"])
	}
	if bridge["targetContractCid"] != "blake3-512:abc" {
		t.Fatalf("targetContractCid = %#v", bridge["targetContractCid"])
	}
	diagnostics := result["diagnostics"].([]any)
	if len(diagnostics) != 0 {
		t.Fatalf("diagnostics = %#v, want none", diagnostics)
	}
}

func TestLiftImplicationsReportsGapForUnmatchedGoCall(t *testing.T) {
	root := t.TempDir()
	src := `package sample

func Caller(x int) int {
	return Missing(x)
}
`
	if err := os.WriteFile(filepath.Join(root, "sample.go"), []byte(src), 0o644); err != nil {
		t.Fatalf("write sample.go: %v", err)
	}

	result, err := LiftImplications(ImplicationParams{
		WorkspaceRoot:    root,
		SourcePaths:      []string{"sample.go"},
		ContractBindings: nil,
	})
	if err != nil {
		t.Fatalf("LiftImplications: %v", err)
	}
	if len(result.IR) != 0 {
		t.Fatalf("expected no bridges without binding, got %#v", result.IR)
	}
	if len(result.Diagnostics) != 1 {
		t.Fatalf("expected one lift-gap diagnostic, got %#v", result.Diagnostics)
	}
	if result.Diagnostics[0]["kind"] != "lift-gap" || result.Diagnostics[0]["callee"] != "Missing" {
		t.Fatalf("unexpected diagnostic: %#v", result.Diagnostics[0])
	}
}

func strconvQuote(s string) string {
	b, err := json.Marshal(s)
	if err != nil {
		panic(err)
	}
	return string(b)
}
