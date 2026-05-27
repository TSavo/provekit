package codec

import "testing"

// Passing test: never exercises a non-canonical encoding, so the seam is
// invisible at runtime. The lifted contract is where it surfaces.
func TestRoundTrip(t *testing.T) {
	if got := ContentAddress(Serialize(5)); got != 5 {
		t.Fatalf("want 5, got %d", got)
	}
}
