// SPDX-License-Identifier: Apache-2.0
//
// Deterministic CBOR decoder: inverse of cbor.hpp's encoder. Subset
// matching the encoder: uint, tstr, bstr, array, map. Used by the
// verifier to read .proof files written by any conformant kit.

#pragma once

#include <cstdint>
#include <map>
#include <memory>
#include <stdexcept>
#include <string>
#include <variant>
#include <vector>

namespace provekit::proof_envelope {

// CborValue is a tagged union for the subset we decode. Maps preserve
// insertion order from the wire (which is the lex-sort order under
// §4.2.1, but the decoder doesn't enforce sort: just records).
struct CborValue;
using CborValuePtr = std::shared_ptr<CborValue>;

struct CborValue {
    using Variant = std::variant<
        uint64_t,
        std::string,
        std::vector<uint8_t>,
        std::vector<CborValuePtr>,
        std::map<std::string, CborValuePtr>>;
    Variant v;

    bool is_uint() const { return std::holds_alternative<uint64_t>(v); }
    bool is_tstr() const { return std::holds_alternative<std::string>(v); }
    bool is_bstr() const { return std::holds_alternative<std::vector<uint8_t>>(v); }
    bool is_array() const { return std::holds_alternative<std::vector<CborValuePtr>>(v); }
    bool is_map() const { return std::holds_alternative<std::map<std::string, CborValuePtr>>(v); }

    uint64_t as_uint() const { return std::get<uint64_t>(v); }
    const std::string& as_tstr() const { return std::get<std::string>(v); }
    const std::vector<uint8_t>& as_bstr() const { return std::get<std::vector<uint8_t>>(v); }
    const std::vector<CborValuePtr>& as_array() const { return std::get<std::vector<CborValuePtr>>(v); }
    const std::map<std::string, CborValuePtr>& as_map() const { return std::get<std::map<std::string, CborValuePtr>>(v); }
};

// CBORDecoder reads bytes incrementally. Reentrant within a single
// thread; callers must use separate instances for concurrent reads.
class CBORDecoder {
   public:
    CBORDecoder(const uint8_t* data, size_t len) : data_(data), len_(len), pos_(0) {}
    CBORDecoder(const std::vector<uint8_t>& bytes) : CBORDecoder(bytes.data(), bytes.size()) {}

    CborValuePtr decode();

   private:
    void read_head(uint8_t* major, uint64_t* arg);
    CborValuePtr read_value();

    const uint8_t* data_;
    size_t len_;
    size_t pos_;
};

}  // namespace provekit::proof_envelope
