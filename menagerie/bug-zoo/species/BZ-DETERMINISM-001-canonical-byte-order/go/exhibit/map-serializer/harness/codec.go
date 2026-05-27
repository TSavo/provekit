package codec

func Serialize(value int) int { return value }

func ContentAddress(encoding int) int {
	if encoding < 0 {
		panic("content address requires a canonical encoding")
	}
	return encoding
}

// Seam: producer output flows into the consumer whose canonical precondition it
// does not establish.
func AddressOf(value int) int { return ContentAddress(Serialize(value)) }
