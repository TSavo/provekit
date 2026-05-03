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
