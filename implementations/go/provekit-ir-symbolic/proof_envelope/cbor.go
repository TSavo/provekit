// Package proof_envelope produces deterministic-CBOR .proof files
// per protocol/specs/2026-04-30-proof-file-format.md. CBOR encoding
// follows RFC 8949 §4.2.1 (Core Deterministic Encoding): shortest-form
// integers, definite-length items, map keys sorted in bytewise lex
// order of their CBOR-encoded form.
package proof_envelope

import "encoding/binary"

// CborMajor enumerates RFC 8949 §3.1 major types we use.
type CborMajor uint8

const (
	MajorUInt   CborMajor = 0
	MajorBStr   CborMajor = 2
	MajorTStr   CborMajor = 3
	MajorArray  CborMajor = 4
	MajorMap    CborMajor = 5
)

// CBOREncoder writes deterministic-CBOR bytes to an internal buffer.
// Reusable; callers can build complex envelopes incrementally.
type CBOREncoder struct {
	buf []byte
}

// NewCBOREncoder returns a fresh encoder with an empty buffer.
func NewCBOREncoder() *CBOREncoder { return &CBOREncoder{} }

// Bytes returns the accumulated encoded bytes (caller takes ownership).
func (e *CBOREncoder) Bytes() []byte { return e.buf }

// Reset clears the buffer for reuse.
func (e *CBOREncoder) Reset() { e.buf = e.buf[:0] }

// AppendHead writes the initial byte + length encoding (§3) for a
// major type and argument value, choosing the shortest of:
// short (0..23), uint8, uint16, uint32, uint64. RFC 8949 §4.2.1
// "shortest-form" rule.
func (e *CBOREncoder) AppendHead(major CborMajor, arg uint64) {
	mt := byte(major) << 5
	switch {
	case arg < 24:
		e.buf = append(e.buf, mt|byte(arg))
	case arg <= 0xFF:
		e.buf = append(e.buf, mt|24, byte(arg))
	case arg <= 0xFFFF:
		var b [2]byte
		binary.BigEndian.PutUint16(b[:], uint16(arg))
		e.buf = append(e.buf, mt|25)
		e.buf = append(e.buf, b[:]...)
	case arg <= 0xFFFFFFFF:
		var b [4]byte
		binary.BigEndian.PutUint32(b[:], uint32(arg))
		e.buf = append(e.buf, mt|26)
		e.buf = append(e.buf, b[:]...)
	default:
		var b [8]byte
		binary.BigEndian.PutUint64(b[:], arg)
		e.buf = append(e.buf, mt|27)
		e.buf = append(e.buf, b[:]...)
	}
}

// EncodeUInt writes an unsigned integer (major 0).
func (e *CBOREncoder) EncodeUInt(value uint64) {
	e.AppendHead(MajorUInt, value)
}

// EncodeBStr writes a byte string (major 2): head + raw bytes.
func (e *CBOREncoder) EncodeBStr(bytes []byte) {
	e.AppendHead(MajorBStr, uint64(len(bytes)))
	e.buf = append(e.buf, bytes...)
}

// EncodeTStr writes a text string (major 3, UTF-8). Caller is
// responsible for the input being valid UTF-8 (Go strings are by
// convention).
func (e *CBOREncoder) EncodeTStr(s string) {
	e.AppendHead(MajorTStr, uint64(len(s)))
	e.buf = append(e.buf, s...)
}

// EncodeArrayHead writes the initial byte for an array of `count`
// elements; the caller appends element bodies in order.
func (e *CBOREncoder) EncodeArrayHead(count uint64) {
	e.AppendHead(MajorArray, count)
}

// EncodeMapHead writes the initial byte for a map of `count` (key,value)
// pairs. RFC 8949 §4.2.1: caller MUST emit keys in bytewise-lex order
// of their CBOR-encoded form. EmitSortedMap below handles that.
func (e *CBOREncoder) EncodeMapHead(count uint64) {
	e.AppendHead(MajorMap, count)
}
