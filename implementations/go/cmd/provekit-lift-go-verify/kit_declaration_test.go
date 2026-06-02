package main

import (
	"bytes"
	"encoding/json"
	"reflect"
	"strings"
	"testing"
)

const kitDeclarationRPCMethod = "provekit.plugin.kit_declaration"

func runRPCForKitDeclarationTest(t *testing.T, requests []string) []map[string]any {
	t.Helper()
	input := strings.Join(requests, "\n") + "\n"
	var out bytes.Buffer
	if err := runRPC(strings.NewReader(input), &out); err != nil {
		t.Fatalf("runRPC: %v", err)
	}
	lines := strings.Split(strings.TrimSpace(out.String()), "\n")
	responses := make([]map[string]any, 0, len(lines))
	for _, line := range lines {
		if strings.TrimSpace(line) == "" {
			continue
		}
		var response map[string]any
		if err := json.Unmarshal([]byte(line), &response); err != nil {
			t.Fatalf("parse RPC response %q: %v", line, err)
		}
		responses = append(responses, response)
	}
	return responses
}

func responseByID(t *testing.T, responses []map[string]any, id float64) map[string]any {
	t.Helper()
	for _, response := range responses {
		if response["id"] == id {
			return response
		}
	}
	t.Fatalf("missing response id %.0f in %#v", id, responses)
	return nil
}

func expectedGoVerifyKitDeclaration() map[string]any {
	return map[string]any{
		"kit": map[string]any{
			"id":       "go",
			"language": "go",
			"version":  "0.1.0",
		},
		"rpc": map[string]any{
			"methods": []any{
				map[string]any{"name": "initialize", "required": true},
				map[string]any{"name": kitDeclarationRPCMethod, "required": true},
				map[string]any{"name": "lift", "required": true},
				map[string]any{"name": "shutdown", "required": false},
			},
		},
		"proofResolution": map[string]any{"strategy": "go-mod"},
		"effectKinds":     []any{"concept:panic-freedom"},
		"effectLeaves": []any{
			map[string]any{
				"surface": "go",
				"local":   "go:panic",
				"concept": "concept:panic-freedom.leaf.runtime-failure-site",
			},
		},
		"guardPredicates":   []any{},
		"controlCarriers":   []any{},
		"residueCategories": []any{},
	}
}

func TestKitDeclarationReturnsEmpiricalGoVerifySurface(t *testing.T) {
	responses := runRPCForKitDeclarationTest(t, []string{
		`{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}`,
		`{"jsonrpc":"2.0","id":2,"method":"provekit.plugin.kit_declaration"}`,
		`{"jsonrpc":"2.0","id":3,"method":"shutdown"}`,
	})

	declaration := responseByID(t, responses, 2)
	if errValue, ok := declaration["error"]; ok {
		t.Fatalf("kit_declaration returned error: %#v", errValue)
	}
	if !reflect.DeepEqual(declaration["result"], expectedGoVerifyKitDeclaration()) {
		t.Fatalf("kit_declaration result mismatch:\n got: %#v\nwant: %#v", declaration["result"], expectedGoVerifyKitDeclaration())
	}
}

func TestKitDeclarationResponseIsDeterministic(t *testing.T) {
	responses := runRPCForKitDeclarationTest(t, []string{
		`{"jsonrpc":"2.0","id":7,"method":"provekit.plugin.kit_declaration"}`,
		`{"jsonrpc":"2.0","id":8,"method":"provekit.plugin.kit_declaration"}`,
	})

	first := responseByID(t, responses, 7)
	second := responseByID(t, responses, 8)
	if errValue, ok := first["error"]; ok {
		t.Fatalf("first kit_declaration returned error: %#v", errValue)
	}
	if errValue, ok := second["error"]; ok {
		t.Fatalf("second kit_declaration returned error: %#v", errValue)
	}
	if !reflect.DeepEqual(first["result"], second["result"]) {
		t.Fatalf("kit_declaration not deterministic:\nfirst:  %#v\nsecond: %#v", first["result"], second["result"])
	}
}

func TestInitializeStaysSeparateFromKitDeclarationContent(t *testing.T) {
	responses := runRPCForKitDeclarationTest(t, []string{
		`{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}`,
	})

	initialize := responseByID(t, responses, 1)
	if errValue, ok := initialize["error"]; ok {
		t.Fatalf("initialize returned error: %#v", errValue)
	}
	result, ok := initialize["result"].(map[string]any)
	if !ok {
		t.Fatalf("initialize result = %#v, want object", initialize["result"])
	}
	for _, forbidden := range []string{"effectKinds", "effectLeaves", "kit"} {
		if _, ok := result[forbidden]; ok {
			t.Fatalf("initialize result must not contain kit declaration field %q: %#v", forbidden, result)
		}
	}
}
