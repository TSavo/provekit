package codec

import "testing"

// Passing unit test. It exercises serialize -> content_address and checks a
// property that holds at runtime; it never asserts canonical byte order, so the
// determinism seam is invisible here. The lifted contract is where the unmet
// canonical_byte_order precondition surfaces.
func TestRoundTripLength(t *testing.T) {
	if got := ContentAddress(Serialize(1, 2)); got != 2 {
		t.Fatalf("want 2, got %d", got)
	}
}
