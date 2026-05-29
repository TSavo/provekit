package emitgotestify

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func op(name string, args ...map[string]any) map[string]any {
	return map[string]any{"kind": "op", "name": name, "args": args}
}

func v(name string) map[string]any {
	return map[string]any{"kind": "var", "name": name}
}

func TestEmitUsesTestifyAssertions(t *testing.T) {
	emission := Emit(EmitPlan{
		PackageName: "sample",
		Function:    "clamp",
		Params:      []string{"x", "lo"},
		ParamTypes:  []string{"int", "int"},
		Predicates: []map[string]any{
			op("concept:ge", v("x"), v("lo")),
			op("concept:le", v("x"), v("hi")),
		},
	})

	if !strings.HasPrefix(emission.Source, "package sample\n") {
		t.Fatalf("emitted Go test must preserve package name; got:\n%s", emission.Source)
	}
	if !strings.Contains(emission.Source, "\"testing\"") {
		t.Fatalf("emitted Go test must import testing; got:\n%s", emission.Source)
	}
	if !strings.Contains(emission.Source, "\"github.com/stretchr/testify/assert\"") {
		t.Fatalf("testify emitter must import assert; got:\n%s", emission.Source)
	}
	if !strings.Contains(emission.Source, "func TestProvekitGe0(t *testing.T)") {
		t.Fatalf("missing ge test function; got:\n%s", emission.Source)
	}
	if !strings.Contains(emission.Source, "assert.GreaterOrEqual(t, x, lo)") {
		t.Fatalf("missing ge testify assertion; got:\n%s", emission.Source)
	}
	if !strings.Contains(emission.Source, "assert.LessOrEqual(t, x, hi)") {
		t.Fatalf("missing le testify assertion; got:\n%s", emission.Source)
	}
	if got, want := emission.EmittedPredicates, []string{"ge", "le"}; strings.Join(got, ",") != strings.Join(want, ",") {
		t.Fatalf("emitted predicates = %v, want %v", got, want)
	}
	if len(emission.UnsupportedPredicates) != 0 {
		t.Fatalf("unexpected unsupported predicates: %v", emission.UnsupportedPredicates)
	}
	if !emission.IsComplete {
		t.Fatalf("all supported predicates should make a complete emission")
	}
	if !strings.HasPrefix(emission.EmittedArtifactCID, "blake3-512:") {
		t.Fatalf("missing artifact cid: %q", emission.EmittedArtifactCID)
	}
	if emission.Path != "provekit_emitted_test.go" {
		t.Fatalf("emission path = %q", emission.Path)
	}
}

func TestFallibleErrUsesTestifyError(t *testing.T) {
	emission := Emit(EmitPlan{
		PackageName: "sample",
		Function:    "load",
		Predicates:  []map[string]any{op("concept:fallible-err", v("err"))},
	})

	if !strings.Contains(emission.Source, "\"errors\"") {
		t.Fatalf("fallible error placeholder must import errors; got:\n%s", emission.Source)
	}
	if !strings.Contains(emission.Source, "err := errors.New(\"contract error\")") {
		t.Fatalf("fallible error placeholder must be a non-nil error; got:\n%s", emission.Source)
	}
	if !strings.Contains(emission.Source, "assert.Error(t, err)") {
		t.Fatalf("fallible-err must render assert.Error; got:\n%s", emission.Source)
	}
}

func TestGeneratedTestifyFileCompilesAndRuns(t *testing.T) {
	emission := Emit(EmitPlan{
		PackageName: "sample",
		Function:    "clamp",
		Predicates: []map[string]any{
			op("concept:eq", v("a"), v("b")),
			op("concept:lt", v("lo"), v("hi")),
			op("concept:option-is-none", v("maybe")),
		},
	})

	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "go.mod"), []byte("module emitted.test\n\ngo 1.22\n\nrequire github.com/stretchr/testify v1.9.0\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "provekit_emitted_test.go"), []byte(emission.Source), 0o644); err != nil {
		t.Fatal(err)
	}
	report := Check(dir)
	if report["ok"] != true {
		t.Fatalf("generated Go testify emission must compile and pass: %#v\nsource:\n%s", report, emission.Source)
	}
}
