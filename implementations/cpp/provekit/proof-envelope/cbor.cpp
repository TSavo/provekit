// SPDX-License-Identifier: Apache-2.0
//
// Deterministic CBOR encoder. RFC 8949 §4.2.1 rules:
//   - shortest-form integer encoding (smallest of short/u8/u16/u32/u64)
//   - definite-length items only (no indefinite-length 0x5f/0x7f/0x9f/0xbf)
//   - map keys sorted in bytewise lex order of their CBOR form
//   - no NaN diversity (we don't emit floats here)
//   - canonical order applies to the head + body bytes

#include "cbor.hpp"

#include <cstring>

namespace provekit::proof_envelope {

void cbor_append_head(std::vector<uint8_t>& out, CborMajor major, uint64_t arg) {
    const uint8_t mt = static_cast<uint8_t>(major) << 5;
    if (arg < 24) {
        out.push_back(mt | static_cast<uint8_t>(arg));
        return;
    }
    if (arg <= 0xFF) {
        out.push_back(mt | 24);
        out.push_back(static_cast<uint8_t>(arg));
        return;
    }
    if (arg <= 0xFFFF) {
        out.push_back(mt | 25);
        out.push_back(static_cast<uint8_t>(arg >> 8));
        out.push_back(static_cast<uint8_t>(arg));
        return;
    }
    if (arg <= 0xFFFFFFFFULL) {
        out.push_back(mt | 26);
        out.push_back(static_cast<uint8_t>(arg >> 24));
        out.push_back(static_cast<uint8_t>(arg >> 16));
        out.push_back(static_cast<uint8_t>(arg >> 8));
        out.push_back(static_cast<uint8_t>(arg));
        return;
    }
    out.push_back(mt | 27);
    for (int i = 7; i >= 0; --i) {
        out.push_back(static_cast<uint8_t>(arg >> (i * 8)));
    }
}

void cbor_encode_uint(std::vector<uint8_t>& out, uint64_t value) {
    cbor_append_head(out, CborMajor::UnsignedInt, value);
}

void cbor_encode_bstr(std::vector<uint8_t>& out, const uint8_t* bytes, size_t len) {
    cbor_append_head(out, CborMajor::ByteString, len);
    out.insert(out.end(), bytes, bytes + len);
}

void cbor_encode_tstr(std::vector<uint8_t>& out, const std::string& utf8) {
    cbor_append_head(out, CborMajor::TextString, utf8.size());
    out.insert(out.end(), utf8.begin(), utf8.end());
}

void cbor_encode_array_head(std::vector<uint8_t>& out, uint64_t count) {
    cbor_append_head(out, CborMajor::Array, count);
}

void cbor_encode_map_head(std::vector<uint8_t>& out, uint64_t count) {
    cbor_append_head(out, CborMajor::Map, count);
}

}  // namespace provekit::proof_envelope
