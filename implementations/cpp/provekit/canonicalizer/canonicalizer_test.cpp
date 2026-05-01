// SPDX-License-Identifier: Apache-2.0
//
// Cross-language conformance test. Asserts the C++ canonicalizer
// produces the same bytes + propertyHash that the protocol spec
// (protocol/specs/2026-04-30-canonicalization-grammar.md §7 + §11)
// dictates for a fixed canonical AST.
//
// EXPECTED VALUES DERIVED FROM SPEC, NOT FROM TS REFERENCE IMPL:
//
// The fixture is the canonical AST for the formula `x > 0` where x is
// de Bruijn index 0 of sort Int:
//
//   {"args":[{"index":0,"kind":"var","sort":{"kind":"primitive","name":"Int"}},
//            {"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":0}],
//    "kind":"atomic","predicate":">"}
//
// (rendered with all keys sorted per §7.3, no whitespace per §7.4,
//  numbers per §7.6, integer 0 emits digit "0").
//
// BLAKE3-512 of those bytes (v1.1.0 protocol hash, full 64-byte digest,
// computed by feeding the bytes to the official BLAKE3 C library):
//   c592f83501c1cfbb9ae69fe89b7738896d0309f1493e3b3f89dbbe78ebbcdb5d
//   6a519307b558b89e37a68d0443a564719d57f30e6a53f4d014b48e9d7fba23a5
//
// propertyHash per §11 (self-identifying tag + full hex):
//   blake3-512:c592...23a5
//
// Any conformant implementation in any language must produce these
// exact bytes and this exact hash. If C++ doesn't match, EITHER the
// C++ impl is wrong OR the spec has a hole. Both are bugs we
// surface as test failures.

#include <cassert>
#include <cstdio>
#include <cstdlib>
#include <string>

#include "jcs.hpp"
#include "property_hash.hpp"
#include "value.hpp"

using namespace provekit::canonicalizer;

namespace {

// Build the canonical AST for `x > 0` (x = de Bruijn 0, sort Int).
// Construction order doesn't matter — the encoder sorts keys per §7.3.
ValuePtr make_fixture() {
    auto int_sort = Value::object({
        {"kind", Value::string("primitive")},
        {"name", Value::string("Int")},
    });
    auto var_x = Value::object({
        {"kind", Value::string("var")},
        {"index", Value::integer(0)},
        {"sort", int_sort},
    });
    auto const_zero = Value::object({
        {"kind", Value::string("const")},
        {"value", Value::integer(0)},
        {"sort", int_sort},
    });
    auto atomic = Value::object({
        {"kind", Value::string("atomic")},
        {"predicate", Value::string(">")},
        {"args", Value::array({var_x, const_zero})},
    });
    return atomic;
}

// Spec-derived expected canonical bytes. Derived BY HAND from
// the JCS rules in §7, not by running the TS impl.
constexpr const char* EXPECTED_BYTES =
    R"({"args":[{"index":0,"kind":"var","sort":{"kind":"primitive","name":"Int"}},)"
    R"({"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":0}],)"
    R"("kind":"atomic","predicate":">"})";

// Spec-derived expected propertyHash for v1.1.0: BLAKE3-512 over
// EXPECTED_BYTES, prefixed with the self-identifying tag "blake3-512:".
constexpr const char* EXPECTED_PROPERTY_HASH =
    "blake3-512:"
    "c592f83501c1cfbb9ae69fe89b7738896d0309f1493e3b3f89dbbe78ebbcdb5d"
    "6a519307b558b89e37a68d0443a564719d57f30e6a53f4d014b48e9d7fba23a5";

bool check(const char* name, bool ok, const std::string& got, const std::string& want) {
    if (ok) {
        std::printf("  [PASS] %s\n", name);
        return true;
    }
    std::printf("  [FAIL] %s\n", name);
    std::printf("    got:  %s\n", got.c_str());
    std::printf("    want: %s\n", want.c_str());
    return false;
}

}  // namespace

int main() {
    std::printf("Cross-language propertyHash conformance test:\n");
    std::printf("  Fixture: x > 0 (canonical AST)\n");
    std::printf("\n");

    auto ast = make_fixture();

    int failures = 0;

    // §7 conformance: JCS bytes must match what the spec dictates.
    const std::string actual_bytes = encode_jcs(*ast);
    if (!check("§7 JCS bytes byte-identical to spec",
               actual_bytes == EXPECTED_BYTES,
               actual_bytes,
               EXPECTED_BYTES)) {
        failures++;
    }

    // §11 conformance: propertyHash matches spec-derived expected value.
    const std::string actual_hash = property_hash(*ast);
    if (!check("§11 propertyHash matches spec-derived expected",
               actual_hash == EXPECTED_PROPERTY_HASH,
               actual_hash,
               EXPECTED_PROPERTY_HASH)) {
        failures++;
    }

    // Normative conformance test per protocol-catalog-format §5: the
    // unicode atomic predicates (≥, ≤, ≠) MUST round-trip verbatim.
    // The kit's atomic predicate names use exactly these UTF-8
    // sequences. Cross-language hash agreement depends on this.
    //
    // U+2265 ≥ encodes as e2 89 a5; U+2264 ≤ as e2 89 a4; U+2260 ≠
    // as e2 89 a0. Any encoder that re-encodes per byte (treating
    // each continuation byte as a code point) will corrupt these.
    {
        const char* unicode_predicates[] = {"\xe2\x89\xa5", "\xe2\x89\xa4", "\xe2\x89\xa0"};
        for (const char* sym : unicode_predicates) {
            auto v = Value::string(sym);
            std::string encoded = encode_jcs(*v);
            // Encoded form is "<sym>" — the input plus surrounding quotes.
            std::string expected = std::string("\"") + sym + "\"";
            std::string label = std::string("unicode predicate round-trip: ") + sym;
            if (!check(label.c_str(), encoded == expected, encoded, expected)) {
                failures++;
            }
        }
    }
    {
        // Mixed ASCII + unicode in one string, as appears in IR atomic
        // names like "x ≥ 0" if ever used as a name field.
        auto v = Value::string("x \xe2\x89\xa5 0");
        std::string encoded = encode_jcs(*v);
        std::string expected = "\"x \xe2\x89\xa5 0\"";
        if (!check("mixed ASCII + unicode preserved", encoded == expected, encoded, expected)) {
            failures++;
        }
    }
    {
        // Object with a unicode name field, mirroring an IR atomic node:
        // {"name":"≥"} canonicalizes to literally those bytes.
        auto v = Value::object({{"name", Value::string("\xe2\x89\xa5")}});
        std::string encoded = encode_jcs(*v);
        std::string expected = "{\"name\":\"\xe2\x89\xa5\"}";
        if (!check("unicode in object name field", encoded == expected, encoded, expected)) {
            failures++;
        }
    }

    std::printf("\n");
    if (failures == 0) {
        std::printf("CONFORMANCE OK — C++ canonicalizer matches the protocol spec.\n");
        return 0;
    }
    std::printf("CONFORMANCE FAILED — %d check(s) didn't match the spec.\n", failures);
    std::printf("This is either a C++ bug OR a SPEC HOLE. Investigate both.\n");
    return 1;
}
