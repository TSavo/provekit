// Cross-language conformance test for the JCS encoder. Asserts the
// rules in protocol/specs/2026-04-30-protocol-catalog-format.md §5.
// The unicode atomic predicates (≥, ≤, ≠) MUST round-trip verbatim
// across every conformant implementation. The kit's atomic predicate
// names use these exact UTF-8 sequences; cross-language hash
// agreement depends on this preservation.
package canonicalizer

import (
	"bytes"
	"testing"
)

func TestUnicodeAtomicPredicatesRoundTripVerbatim(t *testing.T) {
	// U+2265 ≥ encodes as e2 89 a5; U+2264 ≤ as e2 89 a4;
	// U+2260 ≠ as e2 89 a0. Any encoder that re-encodes per
	// byte (treating each continuation byte as a code point)
	// would corrupt these.
	for _, sym := range []string{"≥", "≤", "≠"} {
		got, err := EncodeJCS(sym)
		if err != nil {
			t.Fatalf("EncodeJCS(%q) error: %v", sym, err)
		}
		want := []byte("\"" + sym + "\"")
		if !bytes.Equal(got, want) {
			t.Errorf("unicode predicate %q: got %x, want %x", sym, got, want)
		}
		// The bytes inside the quotes match the input's UTF-8 bytes.
		inner := got[1 : len(got)-1]
		if !bytes.Equal(inner, []byte(sym)) {
			t.Errorf("inner bytes for %q: got %x, want %x", sym, inner, []byte(sym))
		}
	}
}

func TestMixedASCIIAndUnicodePreserved(t *testing.T) {
	s := "x ≥ 0"
	got, err := EncodeJCS(s)
	if err != nil {
		t.Fatalf("EncodeJCS error: %v", err)
	}
	want := []byte("\"x ≥ 0\"")
	if !bytes.Equal(got, want) {
		t.Errorf("mixed ASCII+unicode: got %x, want %x", got, want)
	}
}

func TestUnicodeInObjectNameField(t *testing.T) {
	// Mirrors an IR atomic node: {"name":"≥"} canonicalizes to
	// literally those bytes. Sibling C++ and Rust impls produce
	// the same byte sequence.
	v := map[string]interface{}{"name": "≥"}
	got, err := EncodeJCS(v)
	if err != nil {
		t.Fatalf("EncodeJCS error: %v", err)
	}
	want := []byte("{\"name\":\"\xe2\x89\xa5\"}")
	if !bytes.Equal(got, want) {
		t.Errorf("unicode in object name field: got %x, want %x", got, want)
	}
}
