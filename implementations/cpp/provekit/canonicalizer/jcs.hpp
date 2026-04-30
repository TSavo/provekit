// SPDX-License-Identifier: Apache-2.0
//
// JCS-JSON encoder, per protocol/specs/2026-04-30-canonicalization-grammar.md §7
// (RFC 8785 JSON Canonicalization Scheme).
//
// Implements the rules normatively from the spec — clean-room. No
// reference to the TypeScript implementation. If a case below cannot
// be derived from the spec alone, that is a SPEC HOLE — flag it.
//
// Spec rules implemented:
//   §7.1  UTF-8 output, no BOM
//   §7.2  No HTML escaping
//   §7.3  Object keys sorted by Unicode code-point order
//   §7.4  Compact form (no whitespace)
//   §7.5  String escaping: ", \, control chars (U+0000..U+001F)
//   §7.6  Numbers per ECMA-262 (this v1 only handles integers; floats
//         and bigints are out of scope until the fixture grows)
//   §7.8  Booleans / null verbatim
//   §7.9  Arrays preserve order
//
// Out-of-scope for this v1 (spec rules NOT YET implemented; will fail
// loudly if encountered):
//   §7.6  Float rendering (V8/ECMA-262 toString)
//   §7.7  BigInt outside Number.MAX_SAFE_INTEGER

#pragma once

#include <string>
#include "value.hpp"

namespace provekit::canonicalizer {

// Encode `v` to canonical JSON bytes per the protocol spec's JCS rules.
// Returns UTF-8 bytes ready for SHA-256.
std::string encode_jcs(const Value& v);
inline std::string encode_jcs(const ValuePtr& v) { return encode_jcs(*v); }

}  // namespace provekit::canonicalizer
