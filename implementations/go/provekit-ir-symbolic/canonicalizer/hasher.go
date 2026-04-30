package canonicalizer

import (
	"crypto/sha256"
	"encoding/hex"
)

// Hasher computes the protocol's content-address hashes:
//   - PropertyHash: 16-char hex prefix (binding/property hashes).
//   - EnvelopeCID:  32-char hex prefix (memento envelope CIDs).
//   - FilenameCID:  32-char hex prefix (.proof file's bytes hash).
//
// All three are SHA-256[:N] over canonical bytes; the prefix length
// is the only thing that varies. Spec §11 (canonicalization grammar)
// + §3 of the proof-file-format spec.
type Hasher struct{}

// NewHasher returns a fresh hasher. The zero value is also valid.
func NewHasher() *Hasher { return &Hasher{} }

// Hex returns the full SHA-256 hex digest.
func (h *Hasher) Hex(bytes []byte) string {
	sum := sha256.Sum256(bytes)
	return hex.EncodeToString(sum[:])
}

// PropertyHash16 returns the 16-char hex prefix.
func (h *Hasher) PropertyHash16(bytes []byte) string {
	return h.Hex(bytes)[:16]
}

// EnvelopeCID32 returns the 32-char hex prefix.
func (h *Hasher) EnvelopeCID32(bytes []byte) string {
	return h.Hex(bytes)[:32]
}

// FilenameCID32 is the same shape as EnvelopeCID32; named separately
// because callers think of the .proof filename CID as a different role
// even though the bytes are the same.
func (h *Hasher) FilenameCID32(bytes []byte) string {
	return h.Hex(bytes)[:32]
}

// Package-level convenience wrappers (delegate to a singleton Hasher).
var defaultHasher = NewHasher()

func SHA256Hex(b []byte) string        { return defaultHasher.Hex(b) }
func PropertyHash16(b []byte) string   { return defaultHasher.PropertyHash16(b) }
func EnvelopeCID32(b []byte) string    { return defaultHasher.EnvelopeCID32(b) }
func FilenameCID32(b []byte) string    { return defaultHasher.FilenameCID32(b) }
