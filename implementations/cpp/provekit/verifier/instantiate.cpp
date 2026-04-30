// SPDX-License-Identifier: Apache-2.0

#include "instantiate.hpp"

namespace provekit::verifier {

namespace {

Json substitute_term(const Json& t, const std::string& name, const Json& replacement);

Json substitute_formula(const Json& f, const std::string& name, const Json& replacement) {
    if (!f.is_object()) return f;
    Json out = f;
    const std::string kind = f.value("kind", "");
    if (kind == "atomic") {
        if (f.contains("args") && f["args"].is_array()) {
            Json arr = Json::array();
            for (const auto& a : f["args"]) arr.push_back(substitute_term(a, name, replacement));
            out["args"] = std::move(arr);
        }
        return out;
    }
    if (kind == "and" || kind == "or" || kind == "not" || kind == "implies") {
        if (f.contains("operands") && f["operands"].is_array()) {
            Json arr = Json::array();
            for (const auto& op : f["operands"]) {
                arr.push_back(substitute_formula(op, name, replacement));
            }
            out["operands"] = std::move(arr);
        }
        return out;
    }
    if (kind == "forall" || kind == "exists") {
        // Shadowing: don't substitute past a binder that re-introduces `name`.
        if (f.value("name", "") == name) return out;
        if (f.contains("body")) {
            out["body"] = substitute_formula(f["body"], name, replacement);
        }
        return out;
    }
    return out;
}

Json substitute_term(const Json& t, const std::string& name, const Json& replacement) {
    if (!t.is_object()) return t;
    if (t.value("kind", "") == "var" && t.value("name", "") == name) {
        return replacement;
    }
    if (t.value("kind", "") == "ctor") {
        Json out = t;
        if (t.contains("args") && t["args"].is_array()) {
            Json arr = Json::array();
            for (const auto& a : t["args"]) arr.push_back(substitute_term(a, name, replacement));
            out["args"] = std::move(arr);
        }
        return out;
    }
    return t;
}

}  // namespace

bool InstantiateStage::Run(const ResolvedProperty& resolved,
                            const Json& arg_term,
                            Obligation* out,
                            std::string* err) {
    if (arg_term.is_null()) {
        *err = "no argument term to substitute";
        return false;
    }
    const Json& f = resolved.ir_formula;
    if (!f.is_object() || f.value("kind", "") != "forall") {
        *err = "precondition formula is not a forall";
        return false;
    }
    // Flat quantifier shape: {kind, name, sort, body}. No nested lambda.
    const std::string var_name = f.value("name", "");
    if (var_name.empty()) {
        *err = "forall has empty bound-variable name";
        return false;
    }
    if (!f.contains("body")) {
        *err = "forall has no body";
        return false;
    }
    out->ir_formula = substitute_formula(f["body"], var_name, arg_term);
    out->property_cid = resolved.cid;
    out->ir_kit_version = resolved.ir_kit_version;
    return true;
}

Json InstantiateStage::substitute(const Json& term,
                                    const std::string& var_name,
                                    const Json& replacement) {
    return substitute_formula(term, var_name, replacement);
}

}  // namespace provekit::verifier
