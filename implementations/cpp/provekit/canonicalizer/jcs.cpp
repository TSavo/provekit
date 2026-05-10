// SPDX-License-Identifier: Apache-2.0
//
// JCS-JSON encoder implementation. See jcs.hpp for the spec
// references; this file is the imperative realization of §7 rules.

#include "jcs.hpp"

#include <algorithm>
#include <sstream>
#include <stdexcept>
#include <string>

namespace provekit::canonicalizer {

namespace {

// §7.5: encode a string with JSON-required escaping only.
//   "  → \"
//   \  → \\
//   U+0000..U+001F → \u00XX
//   all other code points: verbatim UTF-8 bytes (per §7.2 no HTML escaping)
//
// Note: this v1 assumes input is already valid UTF-8. The spec
// requires UTF-8 throughout (§7.1); validating is the producer's
// responsibility.
void encode_string(const std::string& s, std::ostringstream& out) {
    out << '"';
    for (unsigned char c : s) {
        if (c == '"') {
            out << "\\\"";
        } else if (c == '\\') {
            out << "\\\\";
        } else if (c <= 0x1F) {
            // §7.5: control chars as \uXXXX with lowercase hex.
            // RFC 8785 §3.2.2.2 named escapes for \b \t \n \f \r could
            // alternatively be used, but \uXXXX form is unambiguous and
            // also conformant. JCS canonicalization chooses the SINGLE
            // representation; for tested control chars we go \uXXXX.
            //
            // SPEC NOTE: §7.5 doesn't mandate which form to use for
            // these particular control chars (\b \t \n \f \r). RFC 8785
            // mandates the named-escape form. Spec hole? Maybe: but
            // for this v1 fixture there are no control chars in the
            // input, so it's not exercised. If a future fixture tests
            // control chars, the spec MUST disambiguate.
            constexpr char hex[] = "0123456789abcdef";
            out << "\\u00" << hex[(c >> 4) & 0xF] << hex[c & 0xF];
        } else {
            out << static_cast<char>(c);
        }
    }
    out << '"';
}

// §7.6: render an integer as its decimal representation (ECMA-262
// toString applied to a finite Number; for an integer that's just the
// signed-decimal digits).
void encode_integer(int64_t n, std::ostringstream& out) {
    out << n;
}

// Forward declaration for recursion.
void encode_value(const Value& v, std::ostringstream& out);

void encode_array(const Value& v, std::ostringstream& out) {
    out << '[';
    bool first = true;
    for (const auto& elem : v.as_array()) {
        if (!first) out << ',';
        encode_value(*elem, out);
        first = false;
    }
    out << ']';
}

void encode_object(const Value& v, std::ostringstream& out) {
    // §7.3: keys sorted by Unicode code point order. For ASCII keys
    // (all keys produced by the canonical AST grammar are ASCII per
    // §4-§5), this is byte-order on the key strings.
    //
    // SPEC NOTE: §7.3 says "Unicode code point order"; for non-ASCII
    // keys this requires comparing code points, not bytes. For this
    // v1 fixture all keys are ASCII so byte-order suffices. A future
    // fixture with a non-ASCII key MUST exercise the code-point
    // distinction (and spec MUST clarify whether to compare code
    // points or UTF-16 code units a la RFC 8785 §3.2.3).
    std::vector<std::pair<std::string, ValuePtr>> kvs = v.as_object();
    std::sort(kvs.begin(), kvs.end(),
              [](const auto& a, const auto& b) { return a.first < b.first; });

    out << '{';
    bool first = true;
    for (const auto& kv : kvs) {
        if (!first) out << ',';
        encode_string(kv.first, out);
        out << ':';
        encode_value(*kv.second, out);
        first = false;
    }
    out << '}';
}

void encode_value(const Value& v, std::ostringstream& out) {
    switch (v.kind()) {
        case ValueKind::Null:
            // §7.8: null verbatim.
            out << "null";
            return;
        case ValueKind::Bool:
            // §7.8: true / false verbatim.
            out << (v.as_bool() ? "true" : "false");
            return;
        case ValueKind::Integer:
            encode_integer(v.as_int(), out);
            return;
        case ValueKind::String:
            encode_string(v.as_string(), out);
            return;
        case ValueKind::Array:
            encode_array(v, out);
            return;
        case ValueKind::Object:
            encode_object(v, out);
            return;
    }
    throw std::runtime_error("encode_value: unknown ValueKind");
}

}  // namespace

std::string encode_jcs(const Value& v) {
    std::ostringstream out;
    encode_value(v, out);
    return out.str();
}

}  // namespace provekit::canonicalizer
