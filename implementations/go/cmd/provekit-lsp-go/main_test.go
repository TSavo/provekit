package main

import (
	"encoding/json"
	"testing"
)

// capture overrides sendResponse to collect the last response line.
func capture() func() *rpcResponse {
	var last rpcResponse
	orig := sendResponse
	sendResponse = func(resp rpcResponse) {
		last = resp
	}
	return func() *rpcResponse {
		sendResponse = orig
		return &last
	}
}

// resultMap JSON-roundtrips resp.Result to map[string]interface{}.
func resultMap(resp *rpcResponse) map[string]interface{} {
	b, _ := json.Marshal(resp.Result)
	var m map[string]interface{}
	json.Unmarshal(b, &m)
	return m
}

func TestHandleInit(t *testing.T) {
	done := capture()
	handleRequest(`{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}`)
	resp := done()
	if resp.Error != nil {
		t.Fatalf("initialize error: %s", resp.Error.Message)
	}
	m := resultMap(resp)
	if m["name"] != "provekit-lsp-go" {
		t.Errorf("name: got %v", m["name"])
	}
	if m["version"] != "0.1.0" {
		t.Errorf("version: got %v", m["version"])
	}
	if m["protocol_version"] != "provekit-lsp-shared/1" {
		t.Errorf("protocol_version: got %v", m["protocol_version"])
	}
	if m["kit_id"] != "go" {
		t.Errorf("kit_id: got %v", m["kit_id"])
	}
	caps, ok := m["capabilities"].(map[string]interface{})
	if !ok {
		t.Fatalf("capabilities not an object: %T", m["capabilities"])
	}
	methods, ok := caps["methods"].([]interface{})
	if !ok {
		t.Fatalf("capabilities.methods not a list: %T", caps["methods"])
	}
	if !containsString(methods, "analyzeDocument") {
		t.Fatalf("initialize must advertise analyzeDocument, got %v", methods)
	}
}

func TestHandleAnalyzeDocumentSharedShape(t *testing.T) {
	done := capture()
	src := `package sample

//provekit:sugar(concept="identity", library="go-stdlib", version="1")
func Identity(x int) int {
	return x
}
`
	params := map[string]interface{}{
		"kit_id":           "go",
		"uri":              "file:///workspace/identity.go",
		"file":             "identity.go",
		"text":             src,
		"document_version": float64(7),
		"workspace_root":   "/workspace",
	}
	msg := json.RawMessage(mustMarshal(params))
	handleRequest(mustMarshal(rpcRequest{JSONRPC: "2.0", ID: 8.0, Method: "analyzeDocument", Params: msg}))
	resp := done()
	if resp.Error != nil {
		t.Fatalf("analyzeDocument error: %s", resp.Error.Message)
	}
	m := resultMap(resp)
	if m["kind"] != "lsp-document-analysis" {
		t.Fatalf("kind = %v", m["kind"])
	}
	if m["schema_version"] != "1" {
		t.Fatalf("schema_version = %v", m["schema_version"])
	}
	if m["kit_id"] != "go" || m["uri"] != "file:///workspace/identity.go" || m["file"] != "identity.go" {
		t.Fatalf("identity fields wrong: %v", m)
	}
	if m["document_cid"] == "" {
		t.Fatalf("document_cid missing: %v", m)
	}
	entries, ok := m["entries"].([]interface{})
	if !ok || len(entries) == 0 {
		t.Fatalf("expected entries, got %T %v", m["entries"], m["entries"])
	}
	var sawSugar bool
	for _, raw := range entries {
		entry, ok := raw.(map[string]interface{})
		if !ok {
			t.Fatalf("entry not object: %T", raw)
		}
		if entry["kind"] == "concept-site" && entry["site_kind"] == "sugar" {
			sawSugar = true
			if entry["concept_name"] != "identity" || entry["target_library_tag"] != "go-stdlib" {
				t.Fatalf("sugar entry wrong: %v", entry)
			}
			rng, ok := entry["source_range"].(map[string]interface{})
			if !ok || rng["kind"] != "source-range" {
				t.Fatalf("source_range missing on sugar entry: %v", entry)
			}
			start := rng["start"].(map[string]interface{})
			if start["line"] != float64(4) {
				t.Fatalf("source range should point at Go-owned func line 4, got %v", rng)
			}
		}
	}
	if !sawSugar {
		t.Fatalf("missing sugar concept-site entry: %v", entries)
	}
	statuses, ok := m["statuses"].([]interface{})
	if !ok || len(statuses) == 0 {
		t.Fatalf("expected explicit statuses, got %T %v", m["statuses"], m["statuses"])
	}
	if !hasStatus(statuses, "emit", "available") {
		t.Fatalf("missing Go kit emit availability status: %v", statuses)
	}
	if !hasStatus(statuses, "prove", "unknown") {
		t.Fatalf("missing explicit non-vacuous prove unknown status: %v", statuses)
	}
}

func TestHandleAnalyzeDocumentLegacyContractDiagnostic(t *testing.T) {
	done := capture()
	src := "package sample\n\n//provekit:contract\nfunc Legacy() {}\n"
	params := map[string]interface{}{
		"kit_id": "go",
		"uri":    "file:///workspace/legacy.go",
		"file":   "legacy.go",
		"text":   src,
	}
	msg := json.RawMessage(mustMarshal(params))
	handleRequest(mustMarshal(rpcRequest{JSONRPC: "2.0", ID: 9.0, Method: "analyzeDocument", Params: msg}))
	resp := done()
	if resp.Error != nil {
		t.Fatalf("analyzeDocument error: %s", resp.Error.Message)
	}
	m := resultMap(resp)
	diagnostics, ok := m["diagnostics"].([]interface{})
	if !ok || len(diagnostics) == 0 {
		t.Fatalf("expected legacy diagnostic, got %T %v", m["diagnostics"], m["diagnostics"])
	}
	diag := diagnostics[0].(map[string]interface{})
	if diag["code"] != "provekit.lsp.lift_gap" {
		t.Fatalf("legacy diagnostic code = %v", diag)
	}
}

func TestHandleParseAnnotation(t *testing.T) {
	done := capture()
	src := "package main\n\n//provekit:contract\nfunc Greet(name string) {}\n"
	msg := json.RawMessage(mustMarshal(parseParams{Path: "test.go", Source: src}))
	handleRequest(mustMarshal(rpcRequest{JSONRPC: "2.0", ID: 2.0, Method: "parse", Params: msg}))
	resp := done()
	if resp.Error != nil {
		t.Fatalf("parse error: %s", resp.Error.Message)
	}
	m := resultMap(resp)
	decls, ok := m["declarations"]
	if !ok {
		t.Fatal("declarations missing")
	}
	list, ok := decls.([]interface{})
	if !ok {
		t.Fatalf("declarations not a list: %T", decls)
	}
	if len(list) != 1 {
		t.Fatalf("expected 1 declaration, got %d", len(list))
	}
}

func TestHandleParseAnnotationPostCondition(t *testing.T) {
	done := capture()
	src := "package main\n\n//provekit:contract post=n>0\nfunc GoCallerOk(n int) int { return n }\n"
	msg := json.RawMessage(mustMarshal(parseParams{Path: "test.go", Source: src}))
	handleRequest(mustMarshal(rpcRequest{JSONRPC: "2.0", ID: 2.5, Method: "parse", Params: msg}))
	resp := done()
	if resp.Error != nil {
		t.Fatalf("parse error: %s", resp.Error.Message)
	}
	m := resultMap(resp)
	list, ok := m["declarations"].([]interface{})
	if !ok {
		t.Fatalf("declarations not a list: %T", m["declarations"])
	}
	if len(list) != 1 {
		t.Fatalf("expected 1 declaration, got %d", len(list))
	}
	decl, ok := list[0].(map[string]interface{})
	if !ok {
		t.Fatalf("declaration not an object: %T", list[0])
	}
	post, ok := decl["post"].(map[string]interface{})
	if !ok {
		t.Fatalf("post condition missing or wrong type: %v", decl["post"])
	}
	if post["kind"] != "forall" || post["name"] != "n" {
		t.Fatalf("post should bind parameter n, got %v", post)
	}
	body, ok := post["body"].(map[string]interface{})
	if !ok {
		t.Fatalf("post body should be an object, got %v", post["body"])
	}
	if body["kind"] != "atomic" || body["name"] != ">" {
		t.Fatalf("post body should be atomic n > 0, got %v", body)
	}
	args, ok := body["args"].([]interface{})
	if !ok || len(args) != 2 {
		t.Fatalf("post args should have n and 0, got %v", body["args"])
	}
	left, _ := args[0].(map[string]interface{})
	right, _ := args[1].(map[string]interface{})
	if left["kind"] != "var" || left["name"] != "n" {
		t.Fatalf("left arg should be var n, got %v", left)
	}
	if right["kind"] != "const" || right["value"] != float64(0) {
		t.Fatalf("right arg should be const 0, got %v", right)
	}
}

func TestHandleParseStructTags(t *testing.T) {
	done := capture()
	src := "package main\ntype Score struct {\n\tValue int `validate:\"gte=0,lte=100\"`\n}\n"

	msg := json.RawMessage(mustMarshal(parseParams{Path: "test.go", Source: src}))
	handleRequest(mustMarshal(rpcRequest{JSONRPC: "2.0", ID: 3.0, Method: "parse", Params: msg}))
	resp := done()
	if resp.Error != nil {
		t.Fatalf("parse error: %s", resp.Error.Message)
	}
	m := resultMap(resp)
	decls, ok := m["declarations"]
	if !ok {
		t.Fatal("declarations missing")
	}
	list, ok := decls.([]interface{})
	if !ok {
		t.Fatalf("declarations not a list: %T", decls)
	}
	if len(list) != 1 {
		t.Fatalf("expected 1 declaration, got %d", len(list))
	}
}

func TestHandleParseNoMatch(t *testing.T) {
	done := capture()
	src := "package main\n\nfunc Add(a, b int) int { return a + b }\n"
	msg := json.RawMessage(mustMarshal(parseParams{Path: "test.go", Source: src}))
	handleRequest(mustMarshal(rpcRequest{JSONRPC: "2.0", ID: 4.0, Method: "parse", Params: msg}))
	resp := done()
	if resp.Error != nil {
		t.Fatalf("parse error: %s", resp.Error.Message)
	}
	m := resultMap(resp)
	decls, ok := m["declarations"]
	if !ok {
		t.Fatal("declarations missing")
	}
	list, ok := decls.([]interface{})
	if !ok {
		t.Fatalf("declarations not a list: %T", decls)
	}
	if len(list) != 0 {
		t.Fatalf("expected 0 declarations, got %d", len(list))
	}
}

func TestHandleParseMultiple(t *testing.T) {
	done := capture()
	src := "package main\ntype User struct {\n\tName string `validate:\"required\"`\n}\n\n//provekit:contract\nfunc Greet() {}\n"
	msg := json.RawMessage(mustMarshal(parseParams{Path: "test.go", Source: src}))
	handleRequest(mustMarshal(rpcRequest{JSONRPC: "2.0", ID: 5.0, Method: "parse", Params: msg}))
	resp := done()
	if resp.Error != nil {
		t.Fatalf("parse error: %s", resp.Error.Message)
	}
	m := resultMap(resp)
	decls, ok := m["declarations"]
	if !ok {
		t.Fatal("declarations missing")
	}
	list, ok := decls.([]interface{})
	if !ok {
		t.Fatalf("declarations not a list: %T", decls)
	}
	if len(list) != 2 {
		t.Fatalf("expected 2 declarations, got %d", len(list))
	}
}

func TestHandleShutdown(t *testing.T) {
	done := capture()
	cont := handleRequest(`{"jsonrpc":"2.0","id":6,"method":"shutdown","params":{}}`)
	resp := done()
	if resp.Error != nil {
		t.Fatalf("shutdown error: %s", resp.Error.Message)
	}
	if cont {
		t.Error("handleRequest should return false on shutdown")
	}
	if resp.Result != nil {
		t.Errorf("shutdown result should be null, got %v", resp.Result)
	}
}

func TestHandleUnknownMethod(t *testing.T) {
	done := capture()
	handleRequest(`{"jsonrpc":"2.0","id":7,"method":"bogus","params":{}}`)
	resp := done()
	if resp.Error == nil {
		t.Fatal("expected error for unknown method")
	}
	if resp.Error.Code != -32601 {
		t.Errorf("error code: got %d, want -32601", resp.Error.Code)
	}
	if resp.Error.Message == "" {
		t.Error("error message empty")
	}
}

func mustMarshal(v interface{}) string {
	b, err := json.Marshal(v)
	if err != nil {
		panic(err)
	}
	return string(b)
}

func containsString(values []interface{}, want string) bool {
	for _, value := range values {
		if value == want {
			return true
		}
	}
	return false
}

func hasStatus(values []interface{}, surface, state string) bool {
	for _, raw := range values {
		value, ok := raw.(map[string]interface{})
		if !ok {
			continue
		}
		if value["surface"] == surface && value["state"] == state {
			return true
		}
	}
	return false
}
