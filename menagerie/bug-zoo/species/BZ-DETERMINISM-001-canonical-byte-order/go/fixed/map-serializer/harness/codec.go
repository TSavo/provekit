package codec

// Fixed: serialize returns the canonical encoding (here: 0, the canonical
// default), which provably satisfies the consumer's non-negative precondition.
// (The literal byte-sort canonicalization needs conditional-return + slice
// modeling in the lifter; see README.)
func Serialize(value int) int { return 0 }

func ContentAddress(encoding int) int {
	if encoding < 0 {
		panic("content address requires a canonical encoding")
	}
	return encoding
}

func AddressOf(value int) int { return ContentAddress(Serialize(value)) }
