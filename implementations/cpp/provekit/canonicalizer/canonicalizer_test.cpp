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
//  numbers per §7.6 — for integer 0, the digit "0").
//
// SHA-256 of those bytes (computed via shasum -a 256 — system tool,
// no implementation involved):
//   818cc781bf4356554c10d65b46112bd9210e41f1605ef071a877bbff7d9ca237
//
// propertyHash (first 16 hex chars per §11):
//   818cc781bf435655
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

// Spec-derived expected propertyHash. Derived from `shasum -a 256`
// over EXPECTED_BYTES, taking the first 16 hex chars per §11.
constexpr const char* EXPECTED_PROPERTY_HASH = "818cc781bf435655";

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

    std::printf("\n");
    if (failures == 0) {
        std::printf("CONFORMANCE OK — C++ canonicalizer matches the protocol spec.\n");
        return 0;
    }
    std::printf("CONFORMANCE FAILED — %d check(s) didn't match the spec.\n", failures);
    std::printf("This is either a C++ bug OR a SPEC HOLE. Investigate both.\n");
    return 1;
}
