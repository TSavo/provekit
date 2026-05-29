package realizego

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
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

func TestPluginCheckRunsGoTest(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "go.mod"), []byte("module example.com/check\n\ngo 1.22\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dir, "check_test.go"), []byte("package check\n\nimport \"testing\"\n\nfunc TestOK(t *testing.T) {}\n"), 0o644); err != nil {
		t.Fatalf("write check_test.go: %v", err)
	}

	response := handleCheck(json.RawMessage(`12`), []byte(`{"out_dir":"`+dir+`"}`))
	envelope, ok := response.(map[string]any)
	if !ok {
		t.Fatalf("response type = %T", response)
	}
	result := envelope["result"].(map[string]any)
	if result["ok"] != true {
		t.Fatalf("go check should pass: %#v", result)
	}
	if result["command"] != "go test ./..." {
		t.Fatalf("command = %#v", result["command"])
	}
}

func TestResolveDependencyProofsReturnsProofsFromGoModuleDependencies(t *testing.T) {
	project := t.TempDir()
	dep := filepath.Join(project, "dep")
	proofName := "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.proof"
	proofPath := filepath.Join(dep, "META-INF", "provekit", proofName)
	if err := os.MkdirAll(filepath.Dir(proofPath), 0o755); err != nil {
		t.Fatalf("mkdir proof dir: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dep, "go.mod"), []byte("module example.com/proofdep\n\ngo 1.22\n"), 0o644); err != nil {
		t.Fatalf("write dep go.mod: %v", err)
	}
	if err := os.WriteFile(proofPath, []byte("proof bytes"), 0o644); err != nil {
		t.Fatalf("write proof: %v", err)
	}
	if err := os.WriteFile(filepath.Join(project, "go.mod"), []byte("module example.com/app\n\ngo 1.22\n\nrequire example.com/proofdep v0.0.0\nreplace example.com/proofdep => ./dep\n"), 0o644); err != nil {
		t.Fatalf("write project go.mod: %v", err)
	}

	var stdout bytes.Buffer
	stdin := strings.NewReader(`{"jsonrpc":"2.0","id":7,"method":"provekit.plugin.resolve_dependency_proofs","params":{"project_root":` + strconvQuote(project) + `}}` + "\n")
	if err := RunRPC(stdin, &stdout); err != nil {
		t.Fatalf("RunRPC: %v", err)
	}

	var response map[string]any
	if err := json.Unmarshal(bytes.TrimSpace(stdout.Bytes()), &response); err != nil {
		t.Fatalf("parse response %q: %v", stdout.String(), err)
	}
	if response["error"] != nil {
		t.Fatalf("resolve_dependency_proofs returned error: %#v", response["error"])
	}
	result := response["result"].(map[string]any)
	paths := result["proof_paths"].([]any)
	if len(paths) != 1 {
		t.Fatalf("proof_paths len = %d, want 1; response=%s", len(paths), stdout.String())
	}
	got := paths[0].(string)
	if !filepath.IsAbs(got) {
		t.Fatalf("proof path must be absolute: %s", got)
	}
	if filepath.Base(got) != proofName {
		t.Fatalf("proof basename = %q, want %q", filepath.Base(got), proofName)
	}
	if _, err := os.Stat(got); err != nil {
		t.Fatalf("proof path must exist: %v", err)
	}
}

func strconvQuote(s string) string {
	b, _ := json.Marshal(s)
	return string(b)
}
