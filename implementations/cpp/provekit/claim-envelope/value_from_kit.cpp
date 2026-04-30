// SPDX-License-Identifier: Apache-2.0

#include "value_from_kit.hpp"

#include <variant>

namespace provekit::claim_envelope {

using ::provekit::canonicalizer::Value;
using ::provekit::canonicalizer::ValuePtr;

ValuePtr sort_to_value(const ::provekit::ir::Sort& sort) {
    return Value::object({
        {"kind", Value::string("primitive")},
        {"name", Value::string(sort.name)},
    });
}

ValuePtr term_to_value(const ::provekit::ir::Term& term) {
    return std::visit(
        [&](const auto& t) -> ValuePtr {
            using T = std::decay_t<decltype(t)>;
            if constexpr (std::is_same_v<T, ::provekit::ir::VarTerm>) {
                return Value::object({
                    {"kind", Value::string("var")},
                    {"name", Value::string(t.name)},
                    {"sort", sort_to_value(t.sort)},
                });
            } else if constexpr (std::is_same_v<T, ::provekit::ir::ConstTerm>) {
                ValuePtr value_v;
                std::visit(
                    [&](const auto& v) {
                        using V = std::decay_t<decltype(v)>;
                        if constexpr (std::is_same_v<V, int64_t>) {
                            value_v = Value::integer(v);
                        } else if constexpr (std::is_same_v<V, std::string>) {
                            value_v = Value::string(v);
                        } else if constexpr (std::is_same_v<V, bool>) {
                            value_v = Value::boolean(v);
                        } else {
                            // double — narrow to integer for v1
                            value_v = Value::integer(static_cast<int64_t>(v));
                        }
                    },
                    t.value);
                return Value::object({
                    {"kind", Value::string("const")},
                    {"value", value_v},
                    {"sort", sort_to_value(t.sort)},
                });
            } else if constexpr (std::is_same_v<T, ::provekit::ir::CtorTerm>) {
                std::vector<ValuePtr> arg_values;
                arg_values.reserve(t.args.size());
                for (const auto& a : t.args) arg_values.push_back(term_to_value(*a));
                return Value::object({
                    {"kind", Value::string("ctor")},
                    {"name", Value::string(t.name)},
                    {"args", Value::array(arg_values)},
                    {"sort", sort_to_value(t.sort)},
                });
            }
        },
        term.v);
}

ValuePtr formula_to_value(const ::provekit::ir::Formula& formula) {
    return std::visit(
        [&](const auto& f) -> ValuePtr {
            using F = std::decay_t<decltype(f)>;
            if constexpr (std::is_same_v<F, ::provekit::ir::AtomicFormula>) {
                std::vector<ValuePtr> arg_values;
                arg_values.reserve(f.args.size());
                for (const auto& a : f.args) arg_values.push_back(term_to_value(*a));
                return Value::object({
                    {"kind", Value::string("atomic")},
                    {"predicate", Value::string(f.predicate)},
                    {"args", Value::array(arg_values)},
                });
            } else if constexpr (std::is_same_v<F, ::provekit::ir::ForallFormula>) {
                ValuePtr lambda = Value::object({
                    {"kind", Value::string("lambda")},
                    {"varName", Value::string(f.predicate->varName)},
                    {"sort", sort_to_value(f.predicate->sort)},
                    {"body", formula_to_value(*f.predicate->body)},
                });
                return Value::object({
                    {"kind", Value::string("forall")},
                    {"sort", sort_to_value(f.sort)},
                    {"predicate", lambda},
                });
            }
        },
        formula.v);
}

}  // namespace provekit::claim_envelope
