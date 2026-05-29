package emitgotestify

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestDescribeReturnsPluginMementoShape(t *testing.T) {
	response := Dispatch(map[string]any{
		"jsonrpc": "2.0",
		"id":      float64(1),
		"method":  "provekit.plugin.describe",
	})

	result, ok := response["result"].(map[string]any)
	if !ok {
		t.Fatalf("describe result must be object: %#v", response)
	}
	for _, key := range []string{"envelope", "header", "metadata"} {
		if _, ok := result[key]; !ok {
			t.Fatalf("describe result missing %s: %#v", key, result)
		}
	}
	header := result["header"].(map[string]any)
	if header["schemaVersion"] != "1" {
		t.Fatalf("schemaVersion = %#v", header["schemaVersion"])
	}
	if !strings.HasPrefix(header["cid"].(string), "blake3-512:") {
		t.Fatalf("header cid = %#v", header["cid"])
	}
	content := header["content"].(map[string]any)
	if content["kind"] != "emit" {
		t.Fatalf("plugin kind = %#v", content["kind"])
	}
	if content["target_framework"] != "testify" {
		t.Fatalf("target framework = %#v", content["target_framework"])
	}
	if _, err := json.Marshal(response); err != nil {
		t.Fatalf("response must be json serializable: %v", err)
	}
}

func TestInvokeEmitsGoTestifyFile(t *testing.T) {
	response := Dispatch(map[string]any{
		"jsonrpc": "2.0",
		"id":      float64(2),
		"method":  "provekit.plugin.invoke",
		"params": map[string]any{
			"package_name": "sample",
			"function":     "Id",
			"predicates":   []any{op("concept:eq", v("x"), v("x"))},
		},
	})

	result, ok := response["result"].(map[string]any)
	if !ok {
		t.Fatalf("invoke result must be object: %#v", response)
	}
	if result["kind"] != "go-testify-test-emission" {
		t.Fatalf("kind = %#v", result["kind"])
	}
	if result["extension"] != "go" {
		t.Fatalf("extension = %#v", result["extension"])
	}
	if result["path"] != "provekit_emitted_test.go" {
		t.Fatalf("path = %#v", result["path"])
	}
	source := result["source"].(string)
	if !strings.Contains(source, "\"github.com/stretchr/testify/assert\"") {
		t.Fatalf("source did not import testify assert: %s", source)
	}
	if !strings.Contains(source, "assert.Equal(t, x, x)") {
		t.Fatalf("source did not render assert.Equal: %s", source)
	}
	if result["is_complete"] != true {
		t.Fatalf("is_complete = %#v", result["is_complete"])
	}
}
