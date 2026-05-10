// SPDX-License-Identifier: Apache-2.0
//
// Example .invariant.cpp: authors a parseInt precondition using the
// C++ kit's primitives. Compiles and runs to verify the collector +
// marshal_declarations pipeline produces deterministic IR-JSON.
//
// Note: marshal_declarations uses logical key order (kind, name, ...).
// The separate JCS canonicalizer (provekit/canonicalizer/jcs.hpp)
// re-sorts to alphabetical order before hashing.

#include "provekit/ir.hpp"

#include <cstdio>
#include <string>

using namespace provekit::ir;

int main() {
    reset_collector();
    begin_collecting();

    // Author a contract: parseInt requires input > 0.
    must("parseInt-requires-positive",
         forall(Int(), [](std::shared_ptr<Term> n) {
             return gt(n, num(0));
         }));

    auto decls = finish();

    if (decls.size() != 1) {
        std::fprintf(stderr, "expected 1 declaration, got %zu\n", decls.size());
        return 1;
    }

    const std::string actual = marshal_declarations(decls);

    // Deterministic output using the kit's logical key order.
    // The fresh variable counter reset above yields `_x0`.
    const std::string expected =
        "[{\"kind\":\"contract\","
        "\"name\":\"parseInt-requires-positive\","
        "\"outBinding\":\"out\","
        "\"pre\":{\"body\":{\"args\":["
        "{\"kind\":\"var\",\"name\":\"_x0\"},"
        "{\"kind\":\"const\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"},\"value\":0}],"
        "\"kind\":\"atomic\",\"name\":\">\"},"
        "\"kind\":\"forall\",\"name\":\"_x0\","
        "\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}}]";

    std::printf("collected IR JSON:\n%s\n\n", actual.c_str());

    if (actual != expected) {
        std::fprintf(stderr, "[FAIL] IR JSON diverges from expected.\n");
        std::fprintf(stderr, "  got:      %s\n", actual.c_str());
        std::fprintf(stderr, "  expected: %s\n", expected.c_str());
        return 1;
    }

    std::printf("[PASS] kit-emitted IR matches expected deterministic output.\n");
    std::printf("       must(name, formula) -> collector -> finish() -> marshal_declarations works.\n");
    return 0;
}
