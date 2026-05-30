package realizego

import (
	"bytes"
	"encoding/base64"
	"encoding/json"
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/tsavo/provekit/go/provekit-ir-symbolic/proof_envelope"
)

func TestRealizeIdentityEmitsCompilableSignature(t *testing.T) {
	project := t.TempDir()
	writeProofBackedGoDependency(t, project, "go", "identity", "x", "return x")
	withWorkingDir(t, project, func() {
		got, err := Realize(RealizeRequest{
			Function:         "Id",
			Params:           []string{"x"},
			ParamTypes:       []string{"int"},
			ReturnType:       "int",
			ConceptName:      "identity",
			TargetLibraryTag: "go",
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
	})
}

// Discrimination: an unsupported concept is refused with MissingTemplateError,
// never silently stubbed.
func TestRealizeUnsupportedConceptRefuses(t *testing.T) {
	project := t.TempDir()
	writeProofBackedGoDependency(t, project, "go", "identity", "x", "return x")
	withWorkingDir(t, project, func() {
		_, err := Realize(RealizeRequest{
			Function:         "F",
			Params:           []string{"x"},
			ConceptName:      "concept:not-supported",
			TargetLibraryTag: "go",
		})
		if err == nil {
			t.Fatal("unsupported concept must be refused")
		}
		if _, ok := err.(*MissingTemplateError); !ok {
			t.Fatalf("want *MissingTemplateError, got %T: %v", err, err)
		}
	})
}

// Discrimination: identity's signature guard rejects a wrong param count
// (2 params) rather than emitting a malformed body.
func TestRealizeIdentityRejectsWrongArity(t *testing.T) {
	project := t.TempDir()
	writeProofBackedGoDependency(t, project, "go", "identity", "x", "return x")
	withWorkingDir(t, project, func() {
		_, err := Realize(RealizeRequest{
			Function:         "Id2",
			Params:           []string{"x", "y"},
			ConceptName:      "identity",
			TargetLibraryTag: "go",
		})
		if err == nil {
			t.Fatal("identity with 2 params must be refused (max_params=1)")
		}
	})
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

func TestPluginAssembleReturnsFormattedGoFiles(t *testing.T) {
	params := map[string]any{
		"target_lang":   "go",
		"file_basename": "id",
		"package_hint":  "sample",
		"fragments": []map[string]any{
			{
				"concept_name": "identity",
				"source":       "func Label(x int) string {\nreturn fmt.Sprint(x)\n}",
				"imports":      []string{"fmt"},
				"helpers":      []string{"const helperValue = 1"},
			},
		},
	}
	req := map[string]any{
		"jsonrpc": "2.0",
		"id":      17,
		"method":  "provekit.plugin.assemble",
		"params":  params,
	}
	raw, err := json.Marshal(req)
	if err != nil {
		t.Fatalf("marshal assemble request: %v", err)
	}

	var stdout bytes.Buffer
	if err := RunRPC(bytes.NewReader(append(raw, '\n')), &stdout); err != nil {
		t.Fatalf("RunRPC: %v", err)
	}

	var response map[string]any
	if err := json.Unmarshal(bytes.TrimSpace(stdout.Bytes()), &response); err != nil {
		t.Fatalf("parse response %q: %v", stdout.String(), err)
	}
	if response["error"] != nil {
		t.Fatalf("assemble returned error: %#v", response["error"])
	}
	result := response["result"].(map[string]any)
	files := result["files"].([]any)
	if len(files) != 1 {
		t.Fatalf("files len = %d, want 1; response=%s", len(files), stdout.String())
	}
	file := files[0].(map[string]any)
	if file["path"] != "id.go" {
		t.Fatalf("assembled path = %#v, want id.go", file["path"])
	}
	content := file["content"].(string)
	if _, err := parser.ParseFile(token.NewFileSet(), "id.go", content, parser.AllErrors); err != nil {
		t.Fatalf("assembled content must parse as Go: %v\n%s", err, content)
	}
	for _, want := range []string{
		"package sample",
		"import \"fmt\"",
		"const helperValue = 1",
		"func Label(x int) string",
		"return fmt.Sprint(x)",
	} {
		if !strings.Contains(content, want) {
			t.Fatalf("assembled content missing %q:\n%s", want, content)
		}
	}
}

func TestPluginMaterializeSourceParsesBoundaryDirective(t *testing.T) {
	project := t.TempDir()
	writeProofBackedGoDependency(t, project, "go", "identity", "x", "return x")
	srcDir := filepath.Join(project, "src")
	if err := os.MkdirAll(srcDir, 0o755); err != nil {
		t.Fatalf("mkdir src: %v", err)
	}
	sourcePath := filepath.Join(srcDir, "id.go")
	if err := os.WriteFile(sourcePath, []byte("package sample\n\n//provekit:boundary(concept=\"identity\", library=\"go\")\nfunc Id(x int) int {\n\treturn 0\n}\n"), 0o644); err != nil {
		t.Fatalf("write source: %v", err)
	}

	req := map[string]any{
		"jsonrpc": "2.0",
		"id":      23,
		"method":  "provekit.plugin.materialize_source",
		"params": map[string]any{
			"project_root":       project,
			"source_dir":         srcDir,
			"target_lang":        "go",
			"target_library_tag": "go",
		},
	}
	raw, err := json.Marshal(req)
	if err != nil {
		t.Fatalf("marshal materialize request: %v", err)
	}

	var stdout bytes.Buffer
	if err := RunRPC(bytes.NewReader(append(raw, '\n')), &stdout); err != nil {
		t.Fatalf("RunRPC: %v", err)
	}

	var response map[string]any
	if err := json.Unmarshal(bytes.TrimSpace(stdout.Bytes()), &response); err != nil {
		t.Fatalf("parse response %q: %v", stdout.String(), err)
	}
	if response["error"] != nil {
		t.Fatalf("materialize_source returned error: %#v", response["error"])
	}
	result := response["result"].(map[string]any)
	files := result["files"].([]any)
	if len(files) != 1 {
		t.Fatalf("files len = %d, want 1; response=%s", len(files), stdout.String())
	}
	file := files[0].(map[string]any)
	if file["path"] != "id.go" {
		t.Fatalf("materialized path = %#v, want id.go", file["path"])
	}
	content := file["content"].(string)
	if _, err := parser.ParseFile(token.NewFileSet(), "id.go", content, parser.AllErrors); err != nil {
		t.Fatalf("materialized Go must parse: %v\n%s", err, content)
	}
	for _, want := range []string{
		"package sample",
		"func Id(x int) int",
		"return x",
	} {
		if !strings.Contains(content, want) {
			t.Fatalf("materialized content missing %q:\n%s", want, content)
		}
	}
	if strings.Contains(content, "provekit:boundary") {
		t.Fatalf("materialized content must not preserve boundary directive:\n%s", content)
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
	proofs := result["proofs"].([]any)
	if len(proofs) != 1 {
		t.Fatalf("proofs len = %d, want 1; response=%s", len(proofs), stdout.String())
	}
	got := proofs[0].(map[string]any)
	if got["cid"] != strings.TrimSuffix(proofName, ".proof") {
		t.Fatalf("proof cid = %q, want %q", got["cid"], strings.TrimSuffix(proofName, ".proof"))
	}
	if got["bytes_base64"] != base64.StdEncoding.EncodeToString([]byte("proof bytes")) {
		t.Fatalf("proof bytes_base64 not returned: %#v", got)
	}
	if got["source"] == proofPath {
		t.Fatalf("resolver must not hand the CLI a Go module-internal proof path: %s", got["source"])
	}
}

func TestBodyTemplateEntriesReturnProofBackedGoModuleBindings(t *testing.T) {
	project := t.TempDir()
	writeProofBackedGoDependency(t, project, "go", "concept:go-proof-backed", "value", "return value + 41")

	var stdout bytes.Buffer
	stdin := strings.NewReader(`{"jsonrpc":"2.0","id":8,"method":"provekit.plugin.body_template_entries","params":{"project_root":` + strconvQuote(project) + `,"target_library_tag":"go"}}` + "\n")
	if err := RunRPC(stdin, &stdout); err != nil {
		t.Fatalf("RunRPC: %v", err)
	}

	var response map[string]any
	if err := json.Unmarshal(bytes.TrimSpace(stdout.Bytes()), &response); err != nil {
		t.Fatalf("parse response %q: %v", stdout.String(), err)
	}
	if response["error"] != nil {
		t.Fatalf("body_template_entries returned error: %#v", response["error"])
	}
	result := response["result"].(map[string]any)
	entries := result["entries"].([]any)
	if len(entries) != 1 {
		t.Fatalf("entries len = %d, want 1; response=%s", len(entries), stdout.String())
	}
	entry := entries[0].(map[string]any)
	if entry["concept_name"] != "concept:go-proof-backed" {
		t.Fatalf("concept_name = %#v", entry["concept_name"])
	}
	template := entry["emission_template"].(map[string]any)
	if template["template"] != "return ${param0} + 41" {
		t.Fatalf("template = %#v, want proof-backed placeholder template", template["template"])
	}
	if entry["target_library_tag"] != "go" {
		t.Fatalf("target_library_tag = %#v, want go", entry["target_library_tag"])
	}
}

func TestBodyTemplateEntriesReportsMalformedProof(t *testing.T) {
	project := t.TempDir()
	proofName := "blake3-512:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd.proof"
	writeGoDependencyWithProofBytes(t, project, proofName, []byte("not deterministic cbor"))

	var stdout bytes.Buffer
	stdin := strings.NewReader(`{"jsonrpc":"2.0","id":10,"method":"provekit.plugin.body_template_entries","params":{"project_root":` + strconvQuote(project) + `,"target_library_tag":"go"}}` + "\n")
	if err := RunRPC(stdin, &stdout); err != nil {
		t.Fatalf("RunRPC: %v", err)
	}

	var response map[string]any
	if err := json.Unmarshal(bytes.TrimSpace(stdout.Bytes()), &response); err != nil {
		t.Fatalf("parse response %q: %v", stdout.String(), err)
	}
	errObj, ok := response["error"].(map[string]any)
	if !ok {
		t.Fatalf("malformed proof must produce an explicit kit error: %s", stdout.String())
	}
	message, _ := errObj["message"].(string)
	if !strings.Contains(message, "BODY_TEMPLATE_ENTRIES_FAILED") ||
		!strings.Contains(message, "decode Go shim proof") {
		t.Fatalf("malformed proof diagnostic is not explicit enough: %q", message)
	}
}

func TestInvokeUsesProofBackedTemplateFromGoModuleDependency(t *testing.T) {
	project := t.TempDir()
	writeProofBackedGoDependency(t, project, "go", "concept:go-proof-backed", "value", "return value + 41")
	oldwd, err := os.Getwd()
	if err != nil {
		t.Fatalf("getwd: %v", err)
	}
	if err := os.Chdir(project); err != nil {
		t.Fatalf("chdir project: %v", err)
	}
	defer func() {
		if err := os.Chdir(oldwd); err != nil {
			t.Fatalf("restore cwd: %v", err)
		}
	}()

	var stdout bytes.Buffer
	stdin := strings.NewReader(`{"jsonrpc":"2.0","id":9,"method":"provekit.plugin.invoke","params":{"function":"AddFortyOne","params":["x"],"param_types":["int"],"return_type":"int","concept_name":"concept:go-proof-backed","target_library_tag":"go"}}` + "\n")
	if err := RunRPC(stdin, &stdout); err != nil {
		t.Fatalf("RunRPC: %v", err)
	}

	var response map[string]any
	if err := json.Unmarshal(bytes.TrimSpace(stdout.Bytes()), &response); err != nil {
		t.Fatalf("parse response %q: %v", stdout.String(), err)
	}
	if response["error"] != nil {
		t.Fatalf("invoke returned error: %#v", response["error"])
	}
	result := response["result"].(map[string]any)
	source := result["source"].(string)
	if !strings.Contains(source, "func AddFortyOne(x int) int") {
		t.Fatalf("source missing signature: %s", source)
	}
	if !strings.Contains(source, "return x + 41") {
		t.Fatalf("source must come from the proof-backed body, not inline identity: %s", source)
	}
}

func writeProofBackedGoDependency(t *testing.T, project, libraryTag, conceptName, paramName, bodyText string) string {
	t.Helper()
	member := map[string]any{
		"body": map[string]any{
			"kind":                 "library-sugar-binding-entry",
			"concept_name":         conceptName,
			"source_function_name": "ProofBacked",
			"target_language":      "go",
			"target_library_tag":   libraryTag,
			"param_names":          []string{paramName},
			"param_types":          []string{"int"},
			"return_type":          "int",
			"body_source": map[string]any{
				"body_text": bodyText,
			},
			"loss_record_contribution": map[string]any{
				"form": "literal",
				"value": map[string]any{
					"entries": []any{},
				},
			},
		},
	}
	memberBytes, err := json.Marshal(member)
	if err != nil {
		t.Fatalf("marshal member: %v", err)
	}
	var seed [32]byte
	for i := range seed {
		seed[i] = 0x42
	}
	out, err := proof_envelope.NewBuilder().Build(&proof_envelope.Input{
		Name:       "@test/go-proof-backed",
		Version:    "0.0.0",
		Members:    map[string][]byte{"blake3-512:" + strings.Repeat("b", 128): memberBytes},
		SignerCID:  "blake3-512:" + strings.Repeat("c", 128),
		SignerSeed: seed,
		DeclaredAt: "2026-05-29T00:00:00.000Z",
	})
	if err != nil {
		t.Fatalf("build proof envelope: %v", err)
	}
	return writeGoDependencyWithProofBytes(t, project, out.FilenameCID+".proof", out.Bytes)
}

func writeGoDependencyWithProofBytes(t *testing.T, project, proofName string, proofBytes []byte) string {
	t.Helper()
	dep := filepath.Join(project, "dep")
	proofPath := filepath.Join(dep, "META-INF", "provekit", proofName)
	if err := os.MkdirAll(filepath.Dir(proofPath), 0o755); err != nil {
		t.Fatalf("mkdir dependency proof dir: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dep, "go.mod"), []byte("module example.com/proofdep\n\ngo 1.22\n"), 0o644); err != nil {
		t.Fatalf("write dep go.mod: %v", err)
	}
	if err := os.WriteFile(filepath.Join(project, "go.mod"), []byte("module example.com/app\n\ngo 1.22\n\nrequire example.com/proofdep v0.0.0\nreplace example.com/proofdep => ./dep\n"), 0o644); err != nil {
		t.Fatalf("write project go.mod: %v", err)
	}
	if err := os.WriteFile(proofPath, proofBytes, 0o644); err != nil {
		t.Fatalf("write proof: %v", err)
	}
	return proofPath
}

func withWorkingDir(t *testing.T, dir string, fn func()) {
	t.Helper()
	oldwd, err := os.Getwd()
	if err != nil {
		t.Fatalf("getwd: %v", err)
	}
	if err := os.Chdir(dir); err != nil {
		t.Fatalf("chdir %s: %v", dir, err)
	}
	defer func() {
		if err := os.Chdir(oldwd); err != nil {
			t.Fatalf("restore cwd: %v", err)
		}
	}()
	fn()
}

func strconvQuote(s string) string {
	b, _ := json.Marshal(s)
	return string(b)
}
