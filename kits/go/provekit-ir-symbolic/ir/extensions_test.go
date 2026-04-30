package ir

import (
	"testing"
)

func TestExtensionSortReturnsNamedSort(t *testing.T) {
	ResetRegistry()
	fp8 := ExtensionSort("FixedPoint8", nil,
		[]SemanticDeclaration{{Kind: "smt-lib-theory", Theory: "FixedSizeBitVectors"}},
		[]string{"smt-lib"})
	primitiveSortValue, ok := fp8.(primitiveSort)
	if !ok || primitiveSortValue.Name != "FixedPoint8" {
		t.Fatalf("expected primitive sort named FixedPoint8, got %#v", fp8)
	}
}

func TestExtensionSortRegistersInRegistry(t *testing.T) {
	ResetRegistry()
	ExtensionSort("FixedPoint8", nil,
		[]SemanticDeclaration{{Kind: "smt-lib-theory", Theory: "FixedSizeBitVectors"}},
		[]string{"smt-lib"})
	decl := LookupExtension("FixedPoint8")
	if decl == nil {
		t.Fatal("expected FixedPoint8 to be registered")
	}
	if decl.Introduces != "sort" {
		t.Errorf("expected introduces=sort, got %s", decl.Introduces)
	}
}

func TestPrimitiveBridgeRegistersDeclaration(t *testing.T) {
	ResetRegistry()
	parseInt := PrimitiveBridge(
		"parseInt",
		[]SortRef{{Named: "String"}},
		SortRef{Named: "Int"},
		"go-kit", "bafy_GO_PARSEINT", "go-runtime", "")
	term := parseInt(StrConst("42"))
	got, ok := term.(ctorTerm)
	if !ok || got.Name != "parseInt" {
		t.Fatalf("expected ctor parseInt, got %#v", term)
	}
	bridge := LookupBridge("parseInt")
	if bridge == nil {
		t.Fatal("expected parseInt bridge to be registered")
	}
	if bridge.TargetContractCID != "bafy_GO_PARSEINT" {
		t.Errorf("expected bafy_GO_PARSEINT, got %s", bridge.TargetContractCID)
	}
}

func TestRegistryCollisionReturnsError(t *testing.T) {
	ResetRegistry()
	ExtensionSort("FixedPoint8", nil,
		[]SemanticDeclaration{{Kind: "smt-lib-theory", Theory: "FixedSizeBitVectors"}},
		[]string{"smt-lib"})
	conflicting := ExtensionDeclaration{
		Introduces: "sort",
		Name:       "FixedPoint8",
		Semantics:  []SemanticDeclaration{{Kind: "natural-language", Text: "different"}},
		Compilers:  []string{"smt-lib"},
	}
	if err := RegisterExtensionDeclaration(conflicting); err == nil {
		t.Fatal("expected collision error")
	}
}

func TestKitBridgesAutoRegisterOnFirstUse(t *testing.T) {
	ResetRegistry()
	// Calling any bridged primitive triggers ensureKitBridgesRegistered
	_ = ParseInt(StrConst("123"))
	bridges := ListBridges()
	if len(bridges) < 5 {
		t.Errorf("expected at least 5 kit bridges registered after first ParseInt call, got %d", len(bridges))
	}
	// parseInt specifically should be in there
	if LookupBridge("parseInt") == nil {
		t.Error("parseInt bridge should be registered after auto-init")
	}
}

func TestDogfoodFullExtensionSet(t *testing.T) {
	ResetRegistry()
	fp8 := ExtensionSort("FixedPoint8", nil,
		[]SemanticDeclaration{{Kind: "smt-lib-theory", Theory: "FixedSizeBitVectors"}},
		[]string{"smt-lib"})
	fpMul := ExtensionCtor("fp8-mul",
		[]SortRef{{Sort: fp8}, {Sort: fp8}},
		SortRef{Sort: fp8},
		[]SemanticDeclaration{{Kind: "smt-lib-theory", Theory: "FixedSizeBitVectors"}},
		[]string{"smt-lib"})
	_ = ExtensionPredicate("fp8-eq",
		[]SortRef{{Sort: fp8}, {Sort: fp8}},
		[]SemanticDeclaration{{Kind: "smt-lib-theory", Theory: "FixedSizeBitVectors"}},
		[]string{"smt-lib"})
	a := varTerm{Name: "a", Sort: fp8}
	b := varTerm{Name: "b", Sort: fp8}
	term := fpMul(a, b)
	got, ok := term.(ctorTerm)
	if !ok || got.Name != "fp8-mul" {
		t.Fatalf("expected ctor fp8-mul, got %#v", term)
	}
	if len(ListExtensions()) != 3 {
		t.Errorf("expected 3 extensions, got %d", len(ListExtensions()))
	}
}
