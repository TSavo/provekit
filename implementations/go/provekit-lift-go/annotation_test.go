package liftgo

import (
	"go/ast"
	"go/parser"
	"go/token"
	"testing"
)

func parseFirstFunc(t *testing.T, src string) *ast.FuncDecl {
	t.Helper()
	fset := token.NewFileSet()
	f, err := parser.ParseFile(fset, "x.go", src, parser.ParseComments)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	for _, d := range f.Decls {
		if fn, ok := d.(*ast.FuncDecl); ok {
			return fn
		}
	}
	t.Fatal("no func decl")
	return nil
}

func TestParseSugarAnnotation(t *testing.T) {
	fn := parseFirstFunc(t, `package p

//provekit:sugar(concept="identity", library="builtin", version="1", family="concept:family:core")
func Id(x int) int { return x }
`)
	ann, err := parseFuncAnnotation(fn)
	if err != nil {
		t.Fatalf("parse annotation: %v", err)
	}
	if ann == nil {
		t.Fatal("expected an annotation")
	}
	if ann.Kind != AnnotationSugar {
		t.Fatalf("kind = %q, want sugar", ann.Kind)
	}
	if ann.Concept != "identity" {
		t.Fatalf("concept = %q, want identity", ann.Concept)
	}
	if ann.Library != "builtin" || ann.Version != "1" || ann.Family != "concept:family:core" {
		t.Fatalf("axes mismatch: %+v", ann)
	}
}

func TestParseBoundaryAnnotation(t *testing.T) {
	fn := parseFirstFunc(t, `package p

//provekit:boundary(concept="concept:json-parse")
func Parse(s string) int { return 0 }
`)
	ann, err := parseFuncAnnotation(fn)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	if ann == nil || ann.Kind != AnnotationBoundary || ann.Concept != "concept:json-parse" {
		t.Fatalf("boundary parse wrong: %+v", ann)
	}
}

// Discrimination: a function with no //provekit directive yields no annotation.
func TestNoAnnotationReturnsNil(t *testing.T) {
	fn := parseFirstFunc(t, `package p

// just a normal doc comment
func Plain(x int) int { return x }
`)
	ann, err := parseFuncAnnotation(fn)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	if ann != nil {
		t.Fatalf("expected nil annotation, got %+v", ann)
	}
}

// Discrimination: a malformed //provekit directive is refused loudly, never
// silently ignored (the author meant to declare something).
func TestMalformedAnnotationRefuses(t *testing.T) {
	cases := []string{
		`//provekit:sugar`,            // no parens
		`//provekit:sugar()`,          // no concept
		`//provekit:wat(concept="x")`, // unknown kind
		`//provekit:sugar(concept=)`,  // malformed value
	}
	for _, directive := range cases {
		src := "package p\n\n" + directive + "\nfunc F(x int) int { return x }\n"
		fn := parseFirstFunc(t, src)
		if _, err := parseFuncAnnotation(fn); err == nil {
			t.Fatalf("directive %q must be refused", directive)
		}
	}
}

// AnnotatedOnly gates emission: only the annotated function is lifted.
func TestLiftAnnotatedOnlyGatesEmission(t *testing.T) {
	src := `package sample

//provekit:sugar(concept="identity")
func Id(x int) int { return x }

func Other(y int) int { return y + 1 }
`
	result, err := LiftSourceWithOptions("example.com/sample", "s.go", []byte(src), LiftOptions{
		NormalizeCoreArith: true,
		AnnotatedOnly:      true,
	})
	if err != nil {
		t.Fatalf("lift: %v", err)
	}
	contracts := result.FunctionContracts()
	if len(contracts) != 1 {
		t.Fatalf("contracts = %d, want 1 (only the annotated Id)", len(contracts))
	}
	if want := "example.com/sample.Id"; contracts[0].FnName != want {
		t.Fatalf("fnName = %q, want %q", contracts[0].FnName, want)
	}
	if ann := result.Annotations[contracts[0].FnName]; ann == nil || ann.Concept != "identity" {
		t.Fatalf("annotation not carried: %+v", result.Annotations)
	}
}

// Discrimination: with AnnotatedOnly OFF, both functions are lifted (the bare
// verify surface keeps its emit-all behavior).
func TestLiftEmitAllWhenNotAnnotatedOnly(t *testing.T) {
	src := `package sample

//provekit:sugar(concept="identity")
func Id(x int) int { return x }

func Other(y int) int { return y + 1 }
`
	result, err := LiftSourceWithOptions("example.com/sample", "s.go", []byte(src), LiftOptions{
		NormalizeCoreArith: true,
	})
	if err != nil {
		t.Fatalf("lift: %v", err)
	}
	if len(result.FunctionContracts()) != 2 {
		t.Fatalf("emit-all must lift both functions, got %d", len(result.FunctionContracts()))
	}
}
