// SPDX-License-Identifier: Apache-2.0
//
// Deterministic CBOR encoder, per RFC 8949 §4.2.1 ("Core Deterministic
// Encoding"). Implements the subset needed for .proof envelopes:
// unsigned integers, byte strings (bstr), text strings (tstr), arrays,
// and maps. No floats, no tags, no indefinite-length items.
//
// Spec: protocol/specs/2026-04-30-proof-file-format.md §1
//       (CBOR envelope; deterministic per RFC 8949 §4.2.1)
//
// Clean-room: derived from RFC 8949 alone. The TypeScript reference
// (which uses @ipld/dag-cbor) is NOT consulted; conformance is proven
// by both impls producing byte-identical bytes on the same inputs.

#pragma once

#include <cstdint>
#include <string>
#include <vector>

namespace provekit::proof_envelope {

// CBOR major types per RFC 8949 §3.1.
enum class CborMajor : uint8_t {
    UnsignedInt = 0,  // 0x00..0x17 small; 0x18=u8, 0x19=u16, 0x1a=u32, 0x1b=u64
    NegativeInt = 1,  // not used in this v1
    ByteString  = 2,  // major 2
    TextString  = 3,  // major 3
    Array       = 4,  // major 4
    Map         = 5,  // major 5
    Tag         = 6,  // not used in this v1
    Simple      = 7,  // not used in this v1
};

// Append the initial byte + length encoding (§3) for a major type +
// argument value, choosing the smallest of: short (0..23), uint8,
// uint16, uint32, uint64. This is the "shortest form" rule of §4.2.1.
void cbor_append_head(std::vector<uint8_t>& out, CborMajor major, uint64_t arg);

// Encode an unsigned integer.
void cbor_encode_uint(std::vector<uint8_t>& out, uint64_t value);

// Encode a byte string (major 2).
void cbor_encode_bstr(std::vector<uint8_t>& out, const uint8_t* bytes, size_t len);
inline void cbor_encode_bstr(std::vector<uint8_t>& out, const std::string& bytes) {
    cbor_encode_bstr(out, reinterpret_cast<const uint8_t*>(bytes.data()), bytes.size());
}

// Encode a text string (major 3); caller is responsible for valid UTF-8.
void cbor_encode_tstr(std::vector<uint8_t>& out, const std::string& utf8);

// Open an array with `count` elements; caller appends element bodies.
void cbor_encode_array_head(std::vector<uint8_t>& out, uint64_t count);

// Open a map with `count` (key,value) pairs.
//
// IMPORTANT (§4.2.1): map keys MUST be sorted in bytewise lexicographic
// order of their CBOR-encoded form. The caller is responsible for
// emitting keys in that order; this function only writes the map head.
void cbor_encode_map_head(std::vector<uint8_t>& out, uint64_t count);

}  // namespace provekit::proof_envelope
