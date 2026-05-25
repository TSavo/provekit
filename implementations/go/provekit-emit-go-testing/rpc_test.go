package emitgotesting

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
	if content["target_framework"] != "testing" {
		t.Fatalf("target framework = %#v", content["target_framework"])
	}
	if _, err := json.Marshal(response); err != nil {
		t.Fatalf("response must be json serializable: %v", err)
	}
}

func TestInvokeEmitsGoTestingFile(t *testing.T) {
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
	if result["kind"] != "go-testing-test-emission" {
		t.Fatalf("kind = %#v", result["kind"])
	}
	if result["extension"] != "go" {
		t.Fatalf("extension = %#v", result["extension"])
	}
	if result["path"] != "provekit_emitted_test.go" {
		t.Fatalf("path = %#v", result["path"])
	}
	if !strings.Contains(result["source"].(string), "import \"testing\"") {
		t.Fatalf("source did not import testing: %s", result["source"])
	}
	if result["is_complete"] != true {
		t.Fatalf("is_complete = %#v", result["is_complete"])
	}
}

func TestInvokeRejectsNonObjectParams(t *testing.T) {
	response := Dispatch(map[string]any{
		"jsonrpc": "2.0",
		"id":      float64(3),
		"method":  "provekit.plugin.invoke",
		"params":  []any{},
	})

	errObj, ok := response["error"].(map[string]any)
	if !ok {
		t.Fatalf("expected error response: %#v", response)
	}
	if errObj["code"] != -32602 {
		t.Fatalf("error code = %#v", errObj["code"])
	}
}

func TestShutdownReturnsNullResult(t *testing.T) {
	response := Dispatch(map[string]any{
		"jsonrpc": "2.0",
		"id":      float64(4),
		"method":  "provekit.plugin.shutdown",
	})
	if _, ok := response["result"]; !ok {
		t.Fatalf("shutdown must return result key: %#v", response)
	}
	if response["result"] != nil {
		t.Fatalf("shutdown result = %#v", response["result"])
	}
}
