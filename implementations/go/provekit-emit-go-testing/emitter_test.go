package emitgotesting

import (
	"os"
	"os/exec"
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

func TestEmitUsesStdlibTestingOnly(t *testing.T) {
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
	if !strings.Contains(emission.Source, "import \"testing\"") {
		t.Fatalf("emitted Go test must import stdlib testing; got:\n%s", emission.Source)
	}
	if strings.Contains(emission.Source, "testify") {
		t.Fatalf("stdlib testing emitter must not mention testify; got:\n%s", emission.Source)
	}
	if !strings.Contains(emission.Source, "func TestProvekitGe0(t *testing.T)") {
		t.Fatalf("missing ge test function; got:\n%s", emission.Source)
	}
	if !strings.Contains(emission.Source, "if !(x >= lo)") {
		t.Fatalf("missing ge assertion; got:\n%s", emission.Source)
	}
	if !strings.Contains(emission.Source, "if !(x <= hi)") {
		t.Fatalf("missing le assertion; got:\n%s", emission.Source)
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

func TestUnsupportedPredicateRecordedAsGapNotEmitted(t *testing.T) {
	emission := Emit(EmitPlan{
		PackageName: "sample",
		Function:    "f",
		Predicates: []map[string]any{
			op("concept:eq", v("a"), v("b")),
			op("concept:mystery", v("a")),
		},
	})

	if got, want := emission.EmittedPredicates, []string{"eq"}; strings.Join(got, ",") != strings.Join(want, ",") {
		t.Fatalf("emitted predicates = %v, want %v", got, want)
	}
	if got, want := emission.UnsupportedPredicates, []string{"mystery"}; strings.Join(got, ",") != strings.Join(want, ",") {
		t.Fatalf("unsupported predicates = %v, want %v", got, want)
	}
	if emission.IsComplete {
		t.Fatalf("unsupported predicates must make emission incomplete")
	}
	if strings.Contains(emission.Source, "mystery") {
		t.Fatalf("unsupported predicate must not be emitted as a passing test; got:\n%s", emission.Source)
	}
}

func TestFallibleErrUsesGoErrorStyle(t *testing.T) {
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
	if !strings.Contains(emission.Source, "if err == nil") {
		t.Fatalf("Go fallible-err should assert non-nil error; got:\n%s", emission.Source)
	}
}

func TestGeneratedTestingFileCompilesAndRuns(t *testing.T) {
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
	if err := os.WriteFile(filepath.Join(dir, "go.mod"), []byte("module emitted.test\n\ngo 1.22\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "provekit_emitted_test.go"), []byte(emission.Source), 0o644); err != nil {
		t.Fatal(err)
	}
	cmd := exec.Command("go", "test", "./...")
	cmd.Dir = dir
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("generated Go testing emission must compile and pass: %v\n%s\nsource:\n%s", err, out, emission.Source)
	}
}

func TestEmitPlanFromParamsParsesRPCObject(t *testing.T) {
	plan := EmitPlanFromParams(map[string]any{
		"contract_id":  "concept:eq",
		"package_name": "sample",
		"function":     "Id",
		"params":       []any{"x"},
		"param_types":  []any{"int"},
		"predicates":   []any{op("concept:eq", v("x"), v("x"))},
	})

	if plan.ContractID != "concept:eq" {
		t.Fatalf("contract id = %q", plan.ContractID)
	}
	if plan.PackageName != "sample" {
		t.Fatalf("package name = %q", plan.PackageName)
	}
	if plan.Function != "Id" {
		t.Fatalf("function = %q", plan.Function)
	}
	if len(plan.Predicates) != 1 {
		t.Fatalf("predicates = %d", len(plan.Predicates))
	}
}
