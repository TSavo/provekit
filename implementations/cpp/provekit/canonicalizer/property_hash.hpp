// SPDX-License-Identifier: Apache-2.0
//
// propertyHash computation per protocol/specs/2026-04-30-canonicalization-grammar.md §11:
//
//   propertyHash = hex(SHA-256(canonicalBytes))[0:16]
//
// where canonicalBytes is the JCS-JSON encoding of the canonical AST
// (this v1) per §7. 16 hex chars = 64 bits of hash.

#pragma once

#include <string>
#include "value.hpp"

namespace provekit::canonicalizer {

// Compute the propertyHash for a canonical AST value.
// Returns the 16-character lowercase hex string (first 64 bits of
// SHA-256 over the JCS bytes).
std::string property_hash(const Value& canonical_ast);
inline std::string property_hash(const ValuePtr& v) { return property_hash(*v); }

}  // namespace provekit::canonicalizer
