package codec

// Serialize emits an encoding in source order: it may be non-canonical.
func Serialize(value int) int { return value }

// ContentAddress requires a canonical (non-negative) encoding; the guard is the
// precondition. Lifts to pre = NOT(encoding < 0).
func ContentAddress(encoding int) int {
	if encoding < 0 {
		panic("content address requires a canonical encoding")
	}
	return encoding
}
