package codec

// CanonicalByteOrder reports whether b is in canonical (sorted) order.
func CanonicalByteOrder(b []byte) bool { return len(b) < 2 || b[0] <= b[1] }

// Serialize emits the two bytes in argument order: valid, NOT canonicalized.
func Serialize(hi byte, lo byte) []byte { return []byte{hi, lo} }

// ContentAddress requires canonical byte order (its guard is the precondition).
func ContentAddress(b []byte) int {
	if !CanonicalByteOrder(b) {
		panic("content address requires canonical byte order")
	}
	return len(b)
}
