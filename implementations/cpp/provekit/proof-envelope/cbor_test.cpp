// SPDX-License-Identifier: Apache-2.0
//
// Cross-language conformance test for the deterministic CBOR encoder.
// Spec: RFC 8949 §4.2.1 ("Core Deterministic Encoding").
//
// All expected byte sequences are derived BY HAND from RFC 8949 §3 +
// §4.2.1 — no reference impl consulted. Two impls converging on these
// bytes is the conformance proof.

#include <cassert>
#include <cstdio>
#include <cstdint>
#include <string>
#include <vector>

#include "cbor.hpp"

using namespace provekit::proof_envelope;

namespace {

std::string hex(const std::vector<uint8_t>& bytes) {
    static constexpr char H[] = "0123456789abcdef";
    std::string out;
    out.reserve(bytes.size() * 2);
    for (uint8_t b : bytes) {
        out.push_back(H[(b >> 4) & 0xF]);
        out.push_back(H[b & 0xF]);
    }
    return out;
}

bool check(const char* name, const std::vector<uint8_t>& got, const std::string& want) {
    const std::string got_hex = hex(got);
    if (got_hex == want) {
        std::printf("  [PASS] %s\n", name);
        return true;
    }
    std::printf("  [FAIL] %s\n", name);
    std::printf("    got:  %s\n", got_hex.c_str());
    std::printf("    want: %s\n", want.c_str());
    return false;
}

}  // namespace

int main() {
    std::printf("CBOR deterministic-encoding conformance:\n");
    int failures = 0;

    // §3 unsigned int "shortest form" rule (§4.2.1):
    //   value 0       → 0x00 (major 0, short 0)
    //   value 23      → 0x17 (major 0, short 23)
    //   value 24      → 0x18 18 (major 0, uint8)
    //   value 255     → 0x18 ff
    //   value 256     → 0x19 0100 (uint16)
    //   value 65535   → 0x19 ffff
    //   value 65536   → 0x1a 00010000 (uint32)
    {
        std::vector<uint8_t> b;
        cbor_encode_uint(b, 0);   if (!check("uint 0",     b, "00")) failures++;
        b.clear();
        cbor_encode_uint(b, 23);  if (!check("uint 23",    b, "17")) failures++;
        b.clear();
        cbor_encode_uint(b, 24);  if (!check("uint 24",    b, "1818")) failures++;
        b.clear();
        cbor_encode_uint(b, 255); if (!check("uint 255",   b, "18ff")) failures++;
        b.clear();
        cbor_encode_uint(b, 256); if (!check("uint 256",   b, "190100")) failures++;
        b.clear();
        cbor_encode_uint(b, 0xFFFFu);     if (!check("uint 65535",  b, "19ffff")) failures++;
        b.clear();
        cbor_encode_uint(b, 0x10000u);    if (!check("uint 65536",  b, "1a00010000")) failures++;
    }

    // §3 text string (major 3):
    //   "" → 0x60 (length 0)
    //   "hi" → 0x62 6869 (length 2 + UTF-8 bytes)
    //   "kind" (4 bytes) → 0x64 6b 69 6e 64
    {
        std::vector<uint8_t> b;
        cbor_encode_tstr(b, "");      if (!check("tstr empty",  b, "60")) failures++;
        b.clear();
        cbor_encode_tstr(b, "hi");    if (!check("tstr 'hi'",   b, "626869")) failures++;
        b.clear();
        cbor_encode_tstr(b, "kind");  if (!check("tstr 'kind'", b, "646b696e64")) failures++;
    }

    // §3 byte string (major 2):
    //   {} (empty) → 0x40
    //   {0xab, 0xcd} → 0x42 abcd
    {
        std::vector<uint8_t> b;
        cbor_encode_bstr(b, std::string(""));       if (!check("bstr empty",  b, "40")) failures++;
        b.clear();
        const uint8_t bytes[] = {0xAB, 0xCD};
        cbor_encode_bstr(b, bytes, 2);              if (!check("bstr abcd",   b, "42abcd")) failures++;
    }

    // §3 + §4.2.1 map shape: {"a": 1, "b": 2} (keys already in lex order):
    //   0xa2 (map of 2)
    //     61 61 (tstr "a") 01 (uint 1)
    //     61 62 (tstr "b") 02 (uint 2)
    {
        std::vector<uint8_t> b;
        cbor_encode_map_head(b, 2);
        cbor_encode_tstr(b, "a");
        cbor_encode_uint(b, 1);
        cbor_encode_tstr(b, "b");
        cbor_encode_uint(b, 2);
        if (!check("map {a:1,b:2}", b, "a2616101616202")) failures++;
    }

    // §3 + §4.2.1 array: [1, 2, 3]
    //   0x83 (array of 3) 01 02 03
    {
        std::vector<uint8_t> b;
        cbor_encode_array_head(b, 3);
        cbor_encode_uint(b, 1);
        cbor_encode_uint(b, 2);
        cbor_encode_uint(b, 3);
        if (!check("array [1,2,3]", b, "83010203")) failures++;
    }

    std::printf("\n");
    if (failures == 0) {
        std::printf("CBOR CONFORMANCE OK — encoder matches RFC 8949 §4.2.1.\n");
        return 0;
    }
    std::printf("CBOR CONFORMANCE FAILED — %d check(s) didn't match the spec.\n", failures);
    return 1;
}
