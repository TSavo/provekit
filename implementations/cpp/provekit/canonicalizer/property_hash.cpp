// SPDX-License-Identifier: Apache-2.0
//
// propertyHash = JCS-encode(canonical AST) then BLAKE3-512, prefixed
// with the self-identifying tag "blake3-512:".

#include "property_hash.hpp"

#include "hash.hpp"
#include "jcs.hpp"

namespace provekit::canonicalizer {

std::string property_hash(const Value& canonical_ast) {
    const std::string bytes = encode_jcs(canonical_ast);
    return compute_cid(bytes);
}

}  // namespace provekit::canonicalizer
