// SPDX-License-Identifier: Apache-2.0
//
// Example .invariant.cpp — authors a parseInt precondition using the
// C++ kit's primitives. Compiled and RUN to emit IR; the kit's
// collector captures the resulting PropertyDecl. `provekit mint` then
// bundles the collected declarations into a .proof file.
//
// This file is the C++ analogue of a TS `.invariant.ts`. The
// architectural shape is the same: kit primitives emit IR when run;
// mint bundles the run output for shipping.
//
// Spec: protocol/specs/2026-04-29-per-language-kit-standard.md

#include "provekit/ir.hpp"

#include <cstdio>
#include <sstream>
#include <string>

using namespace provekit::ir;

namespace {

std::string serialize_decls(const std::vector<PropertyDecl>& decls) {
    std::ostringstream out;
    out << "[";
    bool first = true;
    for (const auto& d : decls) {
        if (!first) out << ",";
        out << "{\"kind\":\"property\",\"name\":";
        write_string(out, d.name);
        out << ",\"formula\":";
        write_formula(out, *d.formula);
        out << "}";
        first = false;
    }
    out << "]";
    return out.str();
}

}  // namespace

int main() {
    reset_collector();
    begin_collecting();

    // The C++ kit's authoring API. Reads as English:
    //   "must: parseInt requires positive input"
    must("parseInt-requires-positive",
        forall(Int(), [](std::shared_ptr<Term> n) {
            return gt(n, num(0));
        }));

    auto decls = finish();

    if (decls.size() != 1) {
        std::fprintf(stderr, "expected 1 declaration, got %zu\n", decls.size());
        return 1;
    }

    const std::string actual = serialize_decls(decls);

    // Cross-kit conformance: this exact JSON is what every conformant
    // host language's kit MUST emit for `must("parseInt-requires-positive",
    // forAll(Int, n => gt(n, num(0))))`. The shape is dictated by:
    //   protocol/specs/2026-04-30-ir-formal-grammar.md §IrFormula
    //   (the kit-emitted IR-JSON encoding, byte-equal across kits today)
    // Var name `_x0` is the first fresh-name from the per-thread quantifier
    // counter (reset above), per the canonicalization grammar §9.
    const std::string expected =
        "[{\"kind\":\"property\","
        "\"name\":\"parseInt-requires-positive\","
        "\"formula\":{\"kind\":\"forall\","
        "\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"},"
        "\"predicate\":{\"kind\":\"lambda\","
        "\"varName\":\"_x0\","
        "\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"},"
        "\"body\":{\"kind\":\"atomic\","
        "\"predicate\":\">\","
        "\"args\":["
        "{\"kind\":\"var\",\"name\":\"_x0\","
        "\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}},"
        "{\"kind\":\"const\",\"value\":0,"
        "\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}"
        "]}}}}]";

    std::printf("collected IR JSON:\n%s\n\n", actual.c_str());

    if (actual != expected) {
        std::fprintf(stderr, "[FAIL] IR JSON diverges from spec.\n");
        std::fprintf(stderr, "  got:      %s\n", actual.c_str());
        std::fprintf(stderr, "  expected: %s\n", expected.c_str());
        return 1;
    }

    std::printf("[PASS] kit-emitted IR matches the protocol's IR-JSON encoding.\n");
    std::printf("       must(name, formula) → collector → finish() works end-to-end.\n");
    return 0;
}
