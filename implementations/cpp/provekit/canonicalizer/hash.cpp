// SPDX-License-Identifier: Apache-2.0
//
// BLAKE3-512 wrapper over the official C library
// (github.com/BLAKE3-team/BLAKE3, vendored or via `brew install blake3`).
//
// Per protocol/specs/2026-04-30-canonicalization-grammar.md §11, the
// protocol-level hash is BLAKE3 with 512-bit (64-byte) output, hex-
// encoded lowercase, prefixed with the self-identifying tag
// "blake3-512:".

#include "hash.hpp"

#include <blake3.h>

#include <cstdint>
#include <string>

namespace provekit::canonicalizer {

namespace {

// BLAKE3 supports arbitrary output length via blake3_hasher_finalize.
// 512 bits = 64 bytes.
constexpr size_t kBlake3_512_OutLen = 64;
constexpr const char* kCidPrefix = "blake3-512:";

}  // namespace

std::string blake3_512_hex(const std::string& bytes) {
    blake3_hasher hasher;
    blake3_hasher_init(&hasher);
    blake3_hasher_update(&hasher, bytes.data(), bytes.size());
    uint8_t out[kBlake3_512_OutLen];
    blake3_hasher_finalize(&hasher, out, kBlake3_512_OutLen);

    static constexpr char hex[] = "0123456789abcdef";
    std::string s;
    s.reserve(kBlake3_512_OutLen * 2);
    for (size_t i = 0; i < kBlake3_512_OutLen; ++i) {
        uint8_t b = out[i];
        s.push_back(hex[(b >> 4) & 0xF]);
        s.push_back(hex[b & 0xF]);
    }
    return s;
}

std::string compute_cid(const std::string& canonical_bytes) {
    return std::string(kCidPrefix) + blake3_512_hex(canonical_bytes);
}

}  // namespace provekit::canonicalizer
