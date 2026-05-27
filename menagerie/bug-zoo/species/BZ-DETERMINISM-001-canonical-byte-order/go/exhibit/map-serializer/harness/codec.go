package codec

func CanonicalByteOrder(b []byte) bool { return len(b) < 2 || b[0] <= b[1] }

// Producer: emits bytes in argument order — valid encoding, NOT canonical.
func Serialize(hi byte, lo byte) []byte { return []byte{hi, lo} }

// Consumer: the guard lifts to pre = canonical_byte_order(b).
func ContentAddress(b []byte) int {
	if !CanonicalByteOrder(b) {
		panic("content address requires canonical byte order")
	}
	return len(b)
}

// Seam: the producer's output flows into the consumer whose precondition it
// does not establish. The round-trip test passes; the lift exposes the gap.
func AddressOf(hi byte, lo byte) int {
	return ContentAddress(Serialize(hi, lo))
}
