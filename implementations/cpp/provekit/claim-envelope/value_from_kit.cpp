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
                // Locked key order: kind, name. (No sort.)
                return Value::object({
                    {"kind", Value::string("var")},
                    {"name", Value::string(t.name)},
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
                            value_v = Value::integer(static_cast<int64_t>(v));
                        }
                    },
                    t.value);
                // Locked key order: kind, value, sort.
                return Value::object({
                    {"kind", Value::string("const")},
                    {"value", value_v},
                    {"sort", sort_to_value(t.sort)},
                });
            } else if constexpr (std::is_same_v<T, ::provekit::ir::CtorTerm>) {
                std::vector<ValuePtr> arg_values;
                arg_values.reserve(t.args.size());
                for (const auto& a : t.args) arg_values.push_back(term_to_value(*a));
                // Locked key order: kind, name, args. (No sort.)
                return Value::object({
                    {"kind", Value::string("ctor")},
                    {"name", Value::string(t.name)},
                    {"args", Value::array(arg_values)},
                });
            } else if constexpr (std::is_same_v<T, ::provekit::ir::LambdaTerm>) {
                // Locked key order: kind, paramName, paramSort, body.
                return Value::object({
                    {"kind", Value::string("lambda")},
                    {"paramName", Value::string(t.paramName)},
                    {"paramSort", sort_to_value(t.paramSort)},
                    {"body", term_to_value(*t.body)},
                });
            } else if constexpr (std::is_same_v<T, ::provekit::ir::LetTerm>) {
                std::vector<ValuePtr> binding_values;
                binding_values.reserve(t.bindings.size());
                for (const auto& b : t.bindings) {
                    binding_values.push_back(Value::object({
                        {"name", Value::string(b.name)},
                        {"boundTerm", term_to_value(*b.boundTerm)},
                    }));
                }
                // Locked key order: kind, bindings, body.
                return Value::object({
                    {"kind", Value::string("let")},
                    {"bindings", Value::array(binding_values)},
                    {"body", term_to_value(*t.body)},
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
                // Locked key order: kind, name, args.
                return Value::object({
                    {"kind", Value::string("atomic")},
                    {"name", Value::string(f.name)},
                    {"args", Value::array(arg_values)},
                });
            } else if constexpr (std::is_same_v<F, ::provekit::ir::ConnectiveFormula>) {
                std::vector<ValuePtr> operand_values;
                operand_values.reserve(f.operands.size());
                for (const auto& op : f.operands) operand_values.push_back(formula_to_value(*op));
                // Locked key order: kind, operands.
                return Value::object({
                    {"kind", Value::string(f.kind)},
                    {"operands", Value::array(operand_values)},
                });
            } else if constexpr (std::is_same_v<F, ::provekit::ir::QuantifierFormula>) {
                // Locked key order: kind, name, sort, body. (Flat — no Lambda wrapper.)
                return Value::object({
                    {"kind", Value::string(f.kind)},
                    {"name", Value::string(f.name)},
                    {"sort", sort_to_value(f.sort)},
                    {"body", formula_to_value(*f.body)},
                });
            } else if constexpr (std::is_same_v<F, ::provekit::ir::ChoiceFormula>) {
                // Locked key order: kind, varName, sort, body.
                return Value::object({
                    {"kind", Value::string("choice")},
                    {"varName", Value::string(f.varName)},
                    {"sort", sort_to_value(f.sort)},
                    {"body", formula_to_value(*f.body)},
                });
            }
        },
        formula.v);
}

}  // namespace provekit::claim_envelope
