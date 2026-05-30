package liftgo

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/tsavo/provekit/go/provekit-ir-symbolic/canonicalizer"
	"github.com/tsavo/provekit/go/provekit-ir-symbolic/proof_envelope"
)

func TestSugarBodyEmitsAstTemplateAlongsideBodyText(t *testing.T) {
	src := []byte(`package shim

func Fetch(url string, headers Header) Response {
	return http.Get(url, headers)
}
`)
	body := mustSugarBodySource(t, "shim.go", src, "Fetch")
	if body.BodyText != "return http.Get(url, headers)" {
		t.Fatalf("body_text = %q", body.BodyText)
	}
	if body.SourceCID != canonicalizer.ComputeCID([]byte(body.BodyText)) {
		t.Fatalf("source_cid = %q, want cid of body_text", body.SourceCID)
	}
	if !equalStrings(body.ParamNames, []string{"url", "headers"}) {
		t.Fatalf("param_names = %#v", body.ParamNames)
	}
	templateBytes := compactJSON(t, body.ASTTemplate)
	if body.TemplateCID != canonicalizer.ComputeCID(templateBytes) {
		t.Fatalf("template_cid = %q, want cid of %s", body.TemplateCID, templateBytes)
	}

	template := jsonObject(t, body.ASTTemplate)
	if template["kind"] != "block" {
		t.Fatalf("template kind = %#v", template["kind"])
	}
	stmts := jsonArray(t, template["stmts"])
	if len(stmts) != 1 {
		t.Fatalf("stmts = %#v, want one", stmts)
	}
	ret := jsonObject(t, stmts[0])
	if ret["kind"] != "return" {
		t.Fatalf("stmt kind = %#v, want return", ret["kind"])
	}
	call := jsonObject(t, ret["expr"])
	if call["kind"] != "method_call" {
		t.Fatalf("expr kind = %#v, want method_call", call["kind"])
	}
	if call["method"] != "Get" {
		t.Fatalf("method = %#v, want Get", call["method"])
	}
	args := jsonArray(t, call["args"])
	if got := jsonObject(t, args[0]); got["kind"] != "param_ref" || got["index"] != float64(1) {
		t.Fatalf("first arg = %#v, want param_ref 1", got)
	}
	if got := jsonObject(t, args[1]); got["kind"] != "param_ref" || got["index"] != float64(2) {
		t.Fatalf("second arg = %#v, want param_ref 2", got)
	}
}

func TestSugarBodyAlphaEquivalenceCollapsesToSameCid(t *testing.T) {
	srcA := []byte(`package shim

func Fetch(url string, headers Header) Response {
	return http.Get(url, headers)
}
`)
	srcB := []byte(`package shim

func Fetch(addr string, hdrs Header) Response {
	return http.Get(addr, hdrs)
}
`)
	bodyA := mustSugarBodySource(t, "a.go", srcA, "Fetch")
	bodyB := mustSugarBodySource(t, "b.go", srcB, "Fetch")
	if bodyA.BodyText != "return http.Get(url, headers)" {
		t.Fatalf("body_text A = %q, want verbatim original parameter spelling", bodyA.BodyText)
	}
	if bodyB.BodyText != "return http.Get(addr, hdrs)" {
		t.Fatalf("body_text B = %q, want verbatim renamed parameter spelling", bodyB.BodyText)
	}
	if !bytes.Equal(compactJSON(t, bodyA.ASTTemplate), compactJSON(t, bodyB.ASTTemplate)) {
		t.Fatalf("alpha-equivalent templates diverged:\nA: %s\nB: %s", compactJSON(t, bodyA.ASTTemplate), compactJSON(t, bodyB.ASTTemplate))
	}
	if bodyA.TemplateCID != bodyB.TemplateCID {
		t.Fatalf("template_cid mismatch: %s vs %s", bodyA.TemplateCID, bodyB.TemplateCID)
	}
	if bodyA.SourceCID == bodyB.SourceCID {
		t.Fatalf("source_cid must preserve original body text spelling")
	}
}

func TestSugarBodyParamNameSwapCanonicalizes(t *testing.T) {
	srcA := []byte(`package shim

func F(a, b int) int {
	return g(a, b)
}
`)
	srcB := []byte(`package shim

func F(x, y int) int {
	return g(x, y)
}
`)
	bodyA := mustSugarBodySource(t, "a.go", srcA, "F")
	bodyB := mustSugarBodySource(t, "b.go", srcB, "F")
	if bodyA.TemplateCID != bodyB.TemplateCID {
		t.Fatalf("template_cid mismatch under param rename: %s vs %s", bodyA.TemplateCID, bodyB.TemplateCID)
	}
}

func TestRecognizeEmitsExactTagForAlphaEquivalentUserFunction(t *testing.T) {
	binding := mustBindingTemplate(t, "concept:http-request", "provekit-shim-go-stdlib-http", "concept:family:http", `package shim

func Fetch(url string, headers Header) Response {
	return http.Get(url, headers)
}
`, "Fetch")
	root := t.TempDir()
	rel := filepath.Join("pkg", "handlers", "fetch.go")
	writeFile(t, filepath.Join(root, rel), `package handlers

func FetchURL(u string, h Header) Response {
	return http.Get(u, h)
}
`)

	resp, err := RecognizeImpl(RecognizeParams{
		ProjectRoot:      root,
		SourcePaths:      []string{rel},
		BindingTemplates: []BindingTemplate{binding},
	})
	if err != nil {
		t.Fatalf("RecognizeImpl: %v", err)
	}
	if len(resp.Tags) != 1 {
		t.Fatalf("tags = %#v, want one", resp.Tags)
	}
	tag := resp.Tags[0]
	if tag.MatchTier != "exact" {
		t.Fatalf("match_tier = %q, want exact", tag.MatchTier)
	}
	if tag.File != rel || tag.FunctionName != "FetchURL" {
		t.Fatalf("tag route = %#v", tag)
	}
	if tag.ConceptName != "concept:http-request" || tag.LibraryTag != "provekit-shim-go-stdlib-http" || tag.Family != "concept:family:http" {
		t.Fatalf("binding axes not preserved: %#v", tag)
	}
	if tag.TemplateCID != binding.TemplateCID || tag.ContractCID != binding.ContractCID {
		t.Fatalf("cid fields not preserved: %#v", tag)
	}
	if len(tag.ParamBindings) != 2 || tag.ParamBindings[0].SourceText != "u" || tag.ParamBindings[1].SourceText != "h" {
		t.Fatalf("param_bindings = %#v", tag.ParamBindings)
	}
}

func TestRecognizeReturnsEmptyTagsForNonMatchingSource(t *testing.T) {
	binding := mustBindingTemplate(t, "concept:http-request", "provekit-shim-go-stdlib-http", "", `package shim

func Fetch(url string, headers Header) Response {
	return http.Get(url, headers)
}
`, "Fetch")
	root := t.TempDir()
	rel := "fetch.go"
	writeFile(t, filepath.Join(root, rel), `package handlers

func FetchURL(u string, h Header) Response {
	return completelyDifferent(u, h)
}
`)

	resp, err := RecognizeImpl(RecognizeParams{
		ProjectRoot:      root,
		SourcePaths:      []string{rel},
		BindingTemplates: []BindingTemplate{binding},
	})
	if err != nil {
		t.Fatalf("RecognizeImpl: %v", err)
	}
	if len(resp.Tags) != 0 {
		t.Fatalf("tags = %#v, want empty", resp.Tags)
	}
}

func TestRecognizeRPCUsesGoKitOwnedTemplatesWithoutBindingTemplates(t *testing.T) {
	binding := mustBindingTemplate(t, "concept:http-request", "provekit-shim-go-stdlib-http", "concept:family:http", `package shim

func Fetch(url string, headers Header) Response {
	return http.Get(url, headers)
}
`, "Fetch")

	for _, tt := range []struct {
		name     string
		source   string
		wantTags int
	}{
		{
			name: "matching body",
			source: `package handlers

func FetchURL(u string, h Header) Response {
	return http.Get(u, h)
}
`,
			wantTags: 1,
		},
		{
			name: "non-matching body",
			source: `package handlers

func FetchURL(u string, h Header) Response {
	return completelyDifferent(u, h)
}
`,
			wantTags: 0,
		},
	} {
		t.Run(tt.name, func(t *testing.T) {
			root := t.TempDir()
			writeRecognizeProofBackedGoDependency(t, root, binding)
			rel := filepath.Join("pkg", "handlers", "fetch.go")
			writeFile(t, filepath.Join(root, rel), tt.source)

			stdin := strings.NewReader(`{"jsonrpc":"2.0","id":1,"method":"provekit.plugin.recognize","params":{"project_root":` + strconvQuote(root) + `,"source_paths":[` + strconvQuote(rel) + `]}}` + "\n")
			var stdout bytes.Buffer
			if err := RunRPC(stdin, &stdout); err != nil {
				t.Fatalf("RunRPC: %v", err)
			}

			var response map[string]any
			if err := json.Unmarshal(bytes.TrimSpace(stdout.Bytes()), &response); err != nil {
				t.Fatalf("response JSON parses: %v\nstdout: %s", err, stdout.String())
			}
			if response["error"] != nil {
				t.Fatalf("recognize RPC returned error: %v", response["error"])
			}
			result := response["result"].(map[string]any)
			tags := result["tags"].([]any)
			if len(tags) != tt.wantTags {
				t.Fatalf("tags = %#v, want %d", tags, tt.wantTags)
			}
			if tt.wantTags == 0 {
				return
			}
			tag := tags[0].(map[string]any)
			if tag["concept_name"] != binding.ConceptName || tag["library_tag"] != binding.LibraryTag {
				t.Fatalf("tag axes = %#v", tag)
			}
			if tag["template_cid"] != binding.TemplateCID || tag["contract_cid"] != binding.ContractCID {
				t.Fatalf("tag cids = %#v", tag)
			}
			if tag["target_proof_cid"] == "" {
				t.Fatalf("tag must carry target_proof_cid for bridge pinning: %#v", tag)
			}
			if tag["match_tier"] != "exact" {
				t.Fatalf("match_tier = %#v", tag["match_tier"])
			}
		})
	}
}

func TestRecognizeRoutesMultipleBindingsPerCallSitePool(t *testing.T) {
	httpBinding := mustBindingTemplate(t, "concept:http-request", "http-lib", "concept:family:http", `package shim

func Fetch(url string, headers Header) Response {
	return http.Get(url, headers)
}
`, "Fetch")
	sqlBinding := mustBindingTemplate(t, "concept:sql-execute", "sql-lib", "concept:family:sql", `package shim

func Exec(conn DB, sql string, args Args) Result {
	return conn.Execute(sql, args)
}
`, "Exec")
	root := t.TempDir()
	rel := "calls.go"
	writeFile(t, filepath.Join(root, rel), `package app

func FetchURL(u string, h Header) Response {
	return http.Get(u, h)
}

func RunQuery(db DB, query string, params Args) Result {
	return db.Execute(query, params)
}
`)

	resp, err := RecognizeImpl(RecognizeParams{
		ProjectRoot:      root,
		SourcePaths:      []string{rel},
		BindingTemplates: []BindingTemplate{httpBinding, sqlBinding},
	})
	if err != nil {
		t.Fatalf("RecognizeImpl: %v", err)
	}
	if len(resp.Tags) != 2 {
		t.Fatalf("tags = %#v, want two", resp.Tags)
	}
	seen := map[string]string{}
	for _, tag := range resp.Tags {
		seen[tag.ConceptName] = tag.FunctionName
		if tag.MatchTier != "exact" {
			t.Fatalf("match_tier = %q, want exact", tag.MatchTier)
		}
	}
	if seen["concept:http-request"] != "FetchURL" {
		t.Fatalf("http binding routed to %#v", seen["concept:http-request"])
	}
	if seen["concept:sql-execute"] != "RunQuery" {
		t.Fatalf("sql binding routed to %#v", seen["concept:sql-execute"])
	}
}

func writeRecognizeProofBackedGoDependency(t *testing.T, project string, binding BindingTemplate) string {
	t.Helper()
	member := map[string]any{
		"body": map[string]any{
			"kind":                 "library-sugar-binding-entry",
			"concept_name":         binding.ConceptName,
			"source_function_name": "Fetch",
			"target_language":      "go",
			"target_library_tag":   binding.LibraryTag,
			"family":               binding.Family,
			"param_names":          binding.ParamNames,
			"param_types":          []string{"string", "Header"},
			"return_type":          "Response",
			"body_source": map[string]any{
				"body_text":    "return http.Get(url, headers)",
				"ast_template": binding.ASTTemplate,
				"template_cid": binding.TemplateCID,
				"param_names":  binding.ParamNames,
			},
			"contract_cid": binding.ContractCID,
		},
	}
	memberBytes, err := marshalJSONNoHTML(member)
	if err != nil {
		t.Fatalf("marshal member: %v", err)
	}
	var seed [32]byte
	for i := range seed {
		seed[i] = 0x42
	}
	out, err := proof_envelope.NewBuilder().Build(&proof_envelope.Input{
		Name:       "@test/go-recognize-proof-backed",
		Version:    "0.0.0",
		Members:    map[string][]byte{"blake3-512:" + strings.Repeat("b", 128): memberBytes},
		SignerCID:  "blake3-512:" + strings.Repeat("c", 128),
		SignerSeed: seed,
		DeclaredAt: "2026-05-29T00:00:00.000Z",
	})
	if err != nil {
		t.Fatalf("build proof envelope: %v", err)
	}
	dep := filepath.Join(project, "dep")
	proofPath := filepath.Join(dep, "META-INF", "provekit", out.FilenameCID+".proof")
	if err := os.MkdirAll(filepath.Dir(proofPath), 0o755); err != nil {
		t.Fatalf("mkdir dependency proof dir: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dep, "go.mod"), []byte("module example.com/proofdep\n\ngo 1.22\n"), 0o644); err != nil {
		t.Fatalf("write dep go.mod: %v", err)
	}
	if err := os.WriteFile(filepath.Join(project, "go.mod"), []byte("module example.com/app\n\ngo 1.22\n\nrequire example.com/proofdep v0.0.0\nreplace example.com/proofdep => ./dep\n"), 0o644); err != nil {
		t.Fatalf("write project go.mod: %v", err)
	}
	if err := os.WriteFile(proofPath, out.Bytes, 0o644); err != nil {
		t.Fatalf("write proof: %v", err)
	}
	return proofPath
}

func mustSugarBodySource(t *testing.T, path string, src []byte, fn string) SugarBodySource {
	t.Helper()
	body, ok, err := SugarBodySourceForFunc(path, src, fn)
	if err != nil {
		t.Fatalf("SugarBodySourceForFunc: %v", err)
	}
	if !ok {
		t.Fatalf("missing function %s", fn)
	}
	return body
}

func mustBindingTemplate(t *testing.T, concept, library, family, src, fn string) BindingTemplate {
	t.Helper()
	body := mustSugarBodySource(t, "shim.go", []byte(src), fn)
	astTemplate := json.RawMessage(compactJSON(t, body.ASTTemplate))
	return BindingTemplate{
		ConceptName: concept,
		LibraryTag:  library,
		Family:      family,
		ASTTemplate: astTemplate,
		TemplateCID: body.TemplateCID,
		ParamNames:  body.ParamNames,
		ContractCID: "blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
	}
}

func compactJSON(t *testing.T, v any) []byte {
	t.Helper()
	b, err := marshalJSONNoHTML(v)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	return b
}

func jsonObject(t *testing.T, v any) map[string]any {
	t.Helper()
	b := compactJSON(t, v)
	var out map[string]any
	if err := json.Unmarshal(b, &out); err != nil {
		t.Fatalf("unmarshal object from %s: %v", b, err)
	}
	return out
}

func jsonArray(t *testing.T, v any) []any {
	t.Helper()
	b := compactJSON(t, v)
	var out []any
	if err := json.Unmarshal(b, &out); err != nil {
		t.Fatalf("unmarshal array from %s: %v", b, err)
	}
	return out
}

func equalStrings(got, want []string) bool {
	if len(got) != len(want) {
		return false
	}
	for i := range got {
		if got[i] != want[i] {
			return false
		}
	}
	return true
}

func writeFile(t *testing.T, path string, contents string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	if err := os.WriteFile(path, []byte(contents), 0o644); err != nil {
		t.Fatalf("write: %v", err)
	}
}
