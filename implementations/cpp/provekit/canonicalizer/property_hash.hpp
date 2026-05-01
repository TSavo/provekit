// SPDX-License-Identifier: Apache-2.0
//
// propertyHash computation per protocol/specs/2026-04-30-canonicalization-grammar.md §11:
//
//   propertyHash = "blake3-512:" + hex(BLAKE3_512(canonicalBytes))
//
// where canonicalBytes is the JCS-JSON encoding of the canonical AST
// (this v1) per §7. The hash is full 64 bytes (128 hex chars), self-
// identifying, no truncation.

#pragma once

#include <string>
#include "value.hpp"

namespace provekit::canonicalizer {

// Compute the propertyHash for a canonical AST value.
// Returns the self-identifying string "blake3-512:" + 128 lowercase hex chars.
std::string property_hash(const Value& canonical_ast);
inline std::string property_hash(const ValuePtr& v) { return property_hash(*v); }

}  // namespace provekit::canonicalizer
