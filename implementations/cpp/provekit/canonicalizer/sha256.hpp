// SPDX-License-Identifier: Apache-2.0
//
// SHA-256 implementation, FIPS 180-4. Public-domain reference
// implementation (Brad Conte's algorithm from
// https://github.com/B-Con/crypto-algorithms — public domain).
//
// Self-contained; no system crypto dep. Used by property_hash to
// produce the 16-hex-char propertyHash per protocol/specs spec §8.

#pragma once

#include <cstdint>
#include <string>

namespace provekit::canonicalizer {

// Compute SHA-256(bytes); return the full 64-hex-char digest.
std::string sha256_hex(const std::string& bytes);

}  // namespace provekit::canonicalizer
