// SPDX-License-Identifier: Apache-2.0
//
// propertyHash = JCS-encode(canonical AST) then SHA-256 prefix-16.

#include "property_hash.hpp"

#include "jcs.hpp"
#include "sha256.hpp"

namespace provekit::canonicalizer {

std::string property_hash(const Value& canonical_ast) {
    const std::string bytes = encode_jcs(canonical_ast);
    const std::string full_digest = sha256_hex(bytes);
    return full_digest.substr(0, 16);
}

}  // namespace provekit::canonicalizer
