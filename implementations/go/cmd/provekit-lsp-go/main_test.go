package main

import (
	"encoding/json"
	"os"
	"path/filepath"
	"runtime"
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
	if !arrayContainsString(caps["source_surfaces"], "go-source") {
		t.Fatalf("source_surfaces must contain go-source: %#v", caps["source_surfaces"])
	}
	if !arrayContainsString(caps["diagnostic_codes"], "provekit.lsp.implication_failed") {
		t.Fatalf("diagnostic_codes must contain implication failure: %#v", caps["diagnostic_codes"])
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

func TestHandleAnalyzeDocumentFloorFixtureSharedDiagnostic(t *testing.T) {
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
	params := map[string]interface{}{
		"kit_id":                         "go",
		"uri":                            "file:///project/tests/lsp/floor-fixture/go.go",
		"file":                           "tests/lsp/floor-fixture/go.go",
		"text":                           string(source),
		"document_version":               42,
		"workspace_root":                 "/project",
		"accepted_protocol_catalog_cids": []interface{}{},
		"policy_cids":                    []interface{}{},
	}
	handleRequest(mustMarshal(rpcRequest{
		JSONRPC: "2.0",
		ID:      8.0,
		Method:  "analyzeDocument",
		Params:  json.RawMessage(mustMarshal(params)),
	}))
	resp := done()
	if resp.Error != nil {
		t.Fatalf("analyzeDocument error: %s", resp.Error.Message)
	}

	m := resultMap(resp)
	if m["kind"] != "lsp-document-analysis" {
		t.Fatalf("kind = %v, want lsp-document-analysis", m["kind"])
	}
	if m["schema_version"] != "1" {
		t.Fatalf("schema_version = %v, want 1", m["schema_version"])
	}
	if m["kit_id"] != "go" {
		t.Fatalf("kit_id = %v, want go", m["kit_id"])
	}
	if m["uri"] != "file:///project/tests/lsp/floor-fixture/go.go" {
		t.Fatalf("uri = %v", m["uri"])
	}
	if m["file"] != "tests/lsp/floor-fixture/go.go" {
		t.Fatalf("file = %v", m["file"])
	}
	documentCID, ok := m["document_cid"].(string)
	if !ok || len(documentCID) != len("blake3-512:")+128 || documentCID[:len("blake3-512:")] != "blake3-512:" {
		t.Fatalf("document_cid = %v, want BLAKE3-512 CID", m["document_cid"])
	}
	if _, ok := m["entries"].([]interface{}); !ok {
		t.Fatalf("entries not an array: %T", m["entries"])
	}
	if _, ok := m["statuses"].([]interface{}); !ok {
		t.Fatalf("statuses not an array: %T", m["statuses"])
	}
	if m["project"] != nil {
		t.Fatalf("project = %v, want nil", m["project"])
	}

	diagnostics, ok := m["diagnostics"].([]interface{})
	if !ok {
		t.Fatalf("diagnostics not an array: %T", m["diagnostics"])
	}
	if len(diagnostics) != 1 {
		t.Fatalf("expected one diagnostic, got %#v", diagnostics)
	}
	diagnostic, ok := diagnostics[0].(map[string]interface{})
	if !ok {
		t.Fatalf("diagnostic not an object: %T", diagnostics[0])
	}
	if diagnostic["code"] != "provekit.lsp.implication_failed" {
		t.Fatalf("code = %v, want provekit.lsp.implication_failed", diagnostic["code"])
	}
	if diagnostic["severity"] != "error" {
		t.Fatalf("severity = %v, want error", diagnostic["severity"])
	}
	if diagnostic["producer"] != "forward-propagation" {
		t.Fatalf("producer = %v, want forward-propagation", diagnostic["producer"])
	}
	if diagnostic["kit_id"] != "go" {
		t.Fatalf("kit_id = %v, want go", diagnostic["kit_id"])
	}
	rng, ok := diagnostic["range"].(map[string]interface{})
	if !ok {
		t.Fatalf("range not an object: %T", diagnostic["range"])
	}
	if rng["start_line"] != float64(19) || rng["start_col"] != float64(11) {
		t.Fatalf("range start = %v:%v, want 19:11", rng["start_line"], rng["start_col"])
	}
	data, ok := diagnostic["data"].(map[string]interface{})
	if !ok {
		t.Fatalf("data not an object: %T", diagnostic["data"])
	}
	if data["callee"] != "checkPositive" {
		t.Fatalf("callee = %v, want checkPositive", data["callee"])
	}
}

func mustMarshal(v interface{}) string {
	b, err := json.Marshal(v)
	if err != nil {
		panic(err)
	}
	return string(b)
}

func arrayContainsString(value interface{}, expected string) bool {
	items, ok := value.([]interface{})
	if !ok {
		return false
	}
	for _, item := range items {
		if item == expected {
			return true
		}
	}
	return false
}
