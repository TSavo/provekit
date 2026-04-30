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
    } else if (kind == "and" || kind == "or") {
        const std::string key = (kind == "and") ? "conjuncts" : "disjuncts";
        if (f.contains(key) && f[key].is_array()) {
            Json arr = Json::array();
            for (const auto& e : f[key]) arr.push_back(substitute_formula(e, name, replacement));
            out[key] = std::move(arr);
        }
    } else if (kind == "not") {
        if (f.contains("body")) out["body"] = substitute_formula(f["body"], name, replacement);
    } else if (kind == "implies") {
        if (f.contains("antecedent")) out["antecedent"] = substitute_formula(f["antecedent"], name, replacement);
        if (f.contains("consequent")) out["consequent"] = substitute_formula(f["consequent"], name, replacement);
    } else if (kind == "forall" || kind == "exists") {
        if (f.contains("predicate") && f["predicate"].is_object()) {
            const auto& pred = f["predicate"];
            if (pred.value("varName", "") == name) return out;  // shadowed
            Json new_pred = pred;
            if (pred.contains("body")) new_pred["body"] = substitute_formula(pred["body"], name, replacement);
            out["predicate"] = std::move(new_pred);
        }
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
        *err = "property formula is not a forall";
        return false;
    }
    if (!f.contains("predicate") || !f["predicate"].is_object()) {
        *err = "forall has no predicate";
        return false;
    }
    const auto& pred = f["predicate"];
    const std::string var_name = pred.value("varName", "");
    if (var_name.empty()) {
        *err = "forall predicate has empty varName";
        return false;
    }
    if (!pred.contains("body")) {
        *err = "forall predicate has no body";
        return false;
    }
    out->ir_formula = substitute_formula(pred["body"], var_name, arg_term);
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
