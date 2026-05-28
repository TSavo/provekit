package main

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	liftgo "github.com/tsavo/provekit/go/provekit-lift-go"
)

func TestExtractGoFuncBodyUsesGoSyntaxPositions(t *testing.T) {
	tests := []struct {
		name string
		src  string
		fn   string
		want string
	}{
		{
			name: "ordinary function body",
			src: `package sample

func Ordinary(x int) int {
	return x + 1
}
`,
			fn:   "Ordinary",
			want: `return x + 1`,
		},
		{
			name: "interpreted string braces",
			src: `package sample

func Interpreted(x int) int {
	s := "not a body close: }"
	_ = s
	return x
}
`,
			fn: "Interpreted",
			want: `s := "not a body close: }"
	_ = s
	return x`,
		},
		{
			name: "raw string braces",
			src:  "package sample\n\nfunc Raw(x int) int {\n\ts := `not a body close: }`\n\t_ = s\n\treturn x\n}\n",
			fn:   "Raw",
			want: "s := `not a body close: }`\n\t_ = s\n\treturn x",
		},
		{
			name: "comment braces",
			src: `package sample

func Commented(x int) int {
	// not a body close: }
	return x
}
`,
			fn: "Commented",
			want: `// not a body close: }
	return x`,
		},
		{
			name: "composite literals and nested blocks",
			src: `package sample

func Nested(x int) int {
	values := []int{1, 2, 3}
	if x > 0 {
		return values[0] + x
	}
	return values[1]
}
`,
			fn: "Nested",
			want: `values := []int{1, 2, 3}
	if x > 0 {
		return values[0] + x
	}
	return values[1]`,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := extractGoFuncBody(tt.src, tt.fn); got != tt.want {
				t.Fatalf("body mismatch:\n got:\n%s\nwant:\n%s", got, tt.want)
			}
		})
	}
}

func TestLiftWorkspaceBindingsPreservesBodySourceShape(t *testing.T) {
	root := t.TempDir()
	source := `package sample

//provekit:sugar(concept="identity", library="builtin", version="1")
func Double(x int) int {
	return x * 2
}
`
	if err := os.WriteFile(filepath.Join(root, "sample.go"), []byte(source), 0o644); err != nil {
		t.Fatalf("write source: %v", err)
	}

	ir, diagnostics, err := liftWorkspace(root, modeBindings)
	if err != nil {
		t.Fatalf("liftWorkspace: %v", err)
	}
	if len(diagnostics) != 0 {
		t.Fatalf("diagnostics = %+v, want none", diagnostics)
	}

	entry := findBindingEntry(t, ir, "Double")
	bodyStruct, ok := entry["body_source"].(liftgo.SugarBodySource)
	if !ok {
		t.Fatalf("body_source = %#v, want liftgo.SugarBodySource", entry["body_source"])
	}
	bodySource := objectFromJSON(t, entry["body_source"])
	bodyText, ok := bodySource["body_text"].(string)
	if !ok {
		t.Fatalf("body_text = %#v, want string", bodySource["body_text"])
	}
	if bodyText != "return x * 2" {
		t.Fatalf("body_text = %q, want trimmed source body", bodyText)
	}
	if bodySource["file"] != "sample.go" {
		t.Fatalf("file = %#v, want sample.go", bodySource["file"])
	}
	if bodySource["source_cid"] != cidOf([]byte(bodyText)) {
		t.Fatalf("source_cid = %#v, want cid of body_text", bodySource["source_cid"])
	}
	if _, ok := bodySource["span"].(map[string]any); !ok {
		t.Fatalf("span = %#v, want object", bodySource["span"])
	}
	template := bodySource["ast_template"]
	templateObj := objectFromJSON(t, template)
	if templateObj["kind"] != "block" {
		t.Fatalf("ast_template.kind = %#v, want block", templateObj["kind"])
	}
	paramNames, ok := bodySource["param_names"].([]any)
	if !ok || len(paramNames) != 1 || paramNames[0] != "x" {
		t.Fatalf("param_names = %#v, want [x]", bodySource["param_names"])
	}
	templateBytes := compactJSONNoHTML(t, bodyStruct.ASTTemplate)
	if bodySource["template_cid"] != cidOf(templateBytes) {
		t.Fatalf("template_cid = %#v, want cid of %s", bodySource["template_cid"], templateBytes)
	}
}

func objectFromJSON(t *testing.T, v any) map[string]any {
	t.Helper()
	var out map[string]any
	if err := json.Unmarshal(compactJSONNoHTML(t, v), &out); err != nil {
		t.Fatalf("unmarshal object from %#v: %v", v, err)
	}
	return out
}

func compactJSONNoHTML(t *testing.T, v any) []byte {
	t.Helper()
	var buf bytes.Buffer
	enc := json.NewEncoder(&buf)
	enc.SetEscapeHTML(false)
	if err := enc.Encode(v); err != nil {
		t.Fatalf("marshal: %v", err)
	}
	return bytes.TrimSuffix(buf.Bytes(), []byte("\n"))
}

func findBindingEntry(t *testing.T, ir []any, sourceName string) map[string]any {
	t.Helper()
	var kinds []string
	for _, item := range ir {
		entry, ok := item.(map[string]any)
		if !ok {
			continue
		}
		if kind, _ := entry["kind"].(string); kind != "" {
			kinds = append(kinds, kind)
		}
		if entry["kind"] == "library-sugar-binding-entry" && entry["source_function_name"] == sourceName {
			return entry
		}
	}
	t.Fatalf("missing binding entry for %s; saw kinds %s", sourceName, strings.Join(kinds, ", "))
	return nil
}
