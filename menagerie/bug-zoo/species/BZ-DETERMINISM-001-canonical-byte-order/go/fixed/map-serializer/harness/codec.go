package codec

func CanonicalByteOrder(b []byte) bool { return len(b) < 2 || b[0] <= b[1] }

// Producer (fixed): emits the smaller byte first, establishing canonical order.
func Serialize(hi byte, lo byte) []byte {
	if hi <= lo {
		return []byte{hi, lo}
	}
	return []byte{lo, hi}
}

func ContentAddress(b []byte) int {
	if !CanonicalByteOrder(b) {
		panic("content address requires canonical byte order")
	}
	return len(b)
}

func AddressOf(hi byte, lo byte) int {
	return ContentAddress(Serialize(hi, lo))
}
