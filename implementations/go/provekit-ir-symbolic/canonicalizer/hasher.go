package canonicalizer

import (
	"encoding/hex"

	"lukechampine.com/blake3"
)

// Hasher computes the protocol's content-address hashes under v1.1.0.
//
// v1.1.0 hash-widening cut: every protocol-surface hash is BLAKE3-512
// (full 64-byte / 128-hex digest) with the self-identifying tag prefix
// "blake3-512:". No truncation, no per-purpose length parameter, no
// SHA-256 anywhere on the protocol path.
//
// Spec: protocol/specs/2026-04-30-canonicalization-grammar.md §11
//	   protocol/specs/2026-04-30-memento-envelope-grammar.md §"Self-identifying"
type Hasher struct{}

// HashTagPrefix is the only permitted v1.1.0 hash tag.
const HashTagPrefix = "blake3-512:"

// NewHasher returns a fresh hasher. The zero value is also valid.
func NewHasher() *Hasher { return &Hasher{} }

// Blake3_512Hex returns the full 128-character lowercase hex digest of
// BLAKE3-512 over bytes. NOT prefixed.
func (h *Hasher) Blake3_512Hex(bytes []byte) string {
	sum := blake3.Sum512(bytes)
	return hex.EncodeToString(sum[:])
}

// ComputeCID returns the self-identifying CID for canonical bytes:
//
//	"blake3-512:" + blake3_512_hex(bytes)
//
// Used for every hash field on the protocol surface: bindingHash,
// propertyHash, preHash / postHash / invHash, antecedentHash /
// consequentHash, member CIDs, filename CIDs.
func (h *Hasher) ComputeCID(bytes []byte) string {
	return HashTagPrefix + h.Blake3_512Hex(bytes)
}

// Package-level convenience wrappers (delegate to a singleton Hasher).
var defaultHasher = NewHasher()

// ComputeCID is the package-level helper for the public API documented
// on the v1.1.0 cut.
func ComputeCID(canonical []byte) string { return defaultHasher.ComputeCID(canonical) }

// Blake3_512Hex returns the un-prefixed full BLAKE3-512 hex digest.
func Blake3_512Hex(b []byte) string { return defaultHasher.Blake3_512Hex(b) }
