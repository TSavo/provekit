// SPDX-License-Identifier: Apache-2.0
//
// Self-identifying hash primitives for protocol v1.1.0.
//
// Spec: protocol/specs/2026-04-30-canonicalization-grammar.md §11 +
//       protocol/specs/2026-04-30-memento-envelope-grammar.md §"Self-identifying"
//
// Every hash that crosses the protocol surface carries its algorithm tag
// inline:
//
//     <algorithm>-<bits>:<lowercase-hex-digest>
//
// v1.1.0 ships with `blake3-512` (full 64-byte / 128 hex BLAKE3 digest)
// as the only permitted hash tag. Verifiers dispatch on the tag and
// reject anything else loud. There is no truncation. No per-purpose
// length parameter. The hash IS the trust AND tells you how to check it.

#pragma once

#include <string>

namespace provekit::canonicalizer {

// Compute BLAKE3 over `bytes`, return the full 128-character lowercase
// hex digest (64-byte / 512-bit output). NOT prefixed.
std::string blake3_512_hex(const std::string& bytes);

// Compute the self-identifying CID for canonical bytes:
//     "blake3-512:" + blake3_512_hex(canonical_bytes)
//
// Used for every hash field in the protocol: bindingHash, propertyHash,
// preHash, postHash, invHash, antecedentHash, consequentHash, member
// CIDs, filename CIDs.
std::string compute_cid(const std::string& canonical_bytes);

}  // namespace provekit::canonicalizer
