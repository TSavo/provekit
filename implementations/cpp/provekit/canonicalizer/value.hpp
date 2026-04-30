// SPDX-License-Identifier: Apache-2.0
//
// A minimal JSON-shaped DOM the canonical AST uses as its in-memory
// representation. Mirrors the protocol's per-spec value model
// (null, bool, integer, string, array, object).
//
// Implements the canonical AST grammar's value space per
// protocol/specs/2026-04-30-canonicalization-grammar.md §4 + §7.
//
// Clean-room. Derived from the spec, not from any reference impl.

#pragma once

#include <cstdint>
#include <map>
#include <memory>
#include <stdexcept>
#include <string>
#include <vector>

namespace provekit::canonicalizer {

class Value;
using ValuePtr = std::shared_ptr<Value>;

enum class ValueKind {
    Null,
    Bool,
    Integer,
    String,
    Array,
    Object,
};

class Value {
   public:
    // Constructors per kind.
    static ValuePtr null_value() {
        return std::make_shared<Value>(ValueKind::Null);
    }
    static ValuePtr boolean(bool b) {
        auto v = std::make_shared<Value>(ValueKind::Bool);
        v->bool_ = b;
        return v;
    }
    static ValuePtr integer(int64_t n) {
        auto v = std::make_shared<Value>(ValueKind::Integer);
        v->int_ = n;
        return v;
    }
    static ValuePtr string(std::string s) {
        auto v = std::make_shared<Value>(ValueKind::String);
        v->str_ = std::move(s);
        return v;
    }
    static ValuePtr array(std::vector<ValuePtr> elems) {
        auto v = std::make_shared<Value>(ValueKind::Array);
        v->array_ = std::move(elems);
        return v;
    }
    // Object preserves insertion order at this layer; the encoder
    // applies the spec's sorted-key rule at serialize time.
    static ValuePtr object(std::vector<std::pair<std::string, ValuePtr>> kvs) {
        auto v = std::make_shared<Value>(ValueKind::Object);
        v->object_ = std::move(kvs);
        return v;
    }

    explicit Value(ValueKind k) : kind_(k) {}

    ValueKind kind() const { return kind_; }
    bool as_bool() const { ensure(ValueKind::Bool); return bool_; }
    int64_t as_int() const { ensure(ValueKind::Integer); return int_; }
    const std::string& as_string() const { ensure(ValueKind::String); return str_; }
    const std::vector<ValuePtr>& as_array() const { ensure(ValueKind::Array); return array_; }
    const std::vector<std::pair<std::string, ValuePtr>>& as_object() const {
        ensure(ValueKind::Object);
        return object_;
    }

   private:
    void ensure(ValueKind expected) const {
        if (kind_ != expected) {
            throw std::runtime_error("Value kind mismatch");
        }
    }

    ValueKind kind_;
    bool bool_ = false;
    int64_t int_ = 0;
    std::string str_;
    std::vector<ValuePtr> array_;
    std::vector<std::pair<std::string, ValuePtr>> object_;
};

}  // namespace provekit::canonicalizer
