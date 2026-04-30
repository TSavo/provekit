// SPDX-License-Identifier: Apache-2.0

#include "smt_emitter.hpp"

#include <map>
#include <set>
#include <sstream>

namespace provekit::verifier {

namespace {

std::string smt_predicate(const std::string& p) {
    if (p == "≠") return "distinct";
    if (p == "≤") return "<=";
    if (p == "≥") return ">=";
    return p;  // =, <, >, kit-defined predicates passthrough
}

std::string smt_sort(const Json& s) {
    if (s.is_object()) {
        const std::string n = s.value("name", "");
        if (n == "Bool" || n == "Real" || n == "String" || n == "Int") return n;
        if (!n.empty()) return n;
    }
    return "Int";
}

std::string emit_term(const Json& t, std::string* err) {
    if (!t.is_object()) {
        *err = "non-object IR term";
        return "";
    }
    const std::string kind = t.value("kind", "");
    if (kind == "var") {
        const std::string nm = t.value("name", "");
        if (nm.empty()) { *err = "var: empty name"; return ""; }
        return nm;
    }
    if (kind == "const") {
        if (!t.contains("value")) { *err = "const: missing value"; return ""; }
        const auto& v = t["value"];
        if (v.is_number_integer()) return std::to_string(v.get<int64_t>());
        if (v.is_number_unsigned()) return std::to_string(v.get<uint64_t>());
        if (v.is_number_float()) {
            double d = v.get<double>();
            if (d == static_cast<double>(static_cast<int64_t>(d))) {
                return std::to_string(static_cast<int64_t>(d));
            }
            std::ostringstream os; os << d; return os.str();
        }
        if (v.is_boolean()) return v.get<bool>() ? "true" : "false";
        if (v.is_string()) return "\"" + v.get<std::string>() + "\"";
        *err = "const: unsupported value type";
        return "";
    }
    if (kind == "ctor") {
        const std::string name = t.value("name", "");
        if (!t.contains("args") || !t["args"].is_array()) return name;
        const auto& args = t["args"];
        if (args.empty()) return name;
        std::ostringstream os;
        os << "(" << name;
        for (const auto& a : args) {
            std::string s = emit_term(a, err);
            if (!err->empty()) return "";
            os << " " << s;
        }
        os << ")";
        return os.str();
    }
    *err = "emit_term: unknown kind '" + kind + "'";
    return "";
}

std::string emit_formula(const Json& f, std::string* err) {
    if (!f.is_object()) { *err = "non-object IR formula"; return ""; }
    const std::string kind = f.value("kind", "");
    if (kind == "atomic") {
        const std::string predicate = f.value("predicate", "");
        const std::string smt_pred = smt_predicate(predicate);
        if (!f.contains("args") || !f["args"].is_array()) { *err = "atomic: no args"; return ""; }
        std::ostringstream os;
        os << "(" << smt_pred;
        for (const auto& a : f["args"]) {
            std::string s = emit_term(a, err);
            if (!err->empty()) return "";
            os << " " << s;
        }
        os << ")";
        return os.str();
    }
    auto connective = [&](const char* op, const char* key) -> std::string {
        if (!f.contains(key) || !f[key].is_array()) {
            *err = std::string(op) + ": missing " + key;
            return "";
        }
        std::ostringstream os;
        os << "(" << op;
        for (const auto& e : f[key]) {
            std::string s = emit_formula(e, err);
            if (!err->empty()) return "";
            os << " " << s;
        }
        os << ")";
        return os.str();
    };
    if (kind == "and") return connective("and", "conjuncts");
    if (kind == "or") return connective("or", "disjuncts");
    if (kind == "not") {
        if (!f.contains("body")) { *err = "not: missing body"; return ""; }
        std::string b = emit_formula(f["body"], err);
        if (!err->empty()) return "";
        return "(not " + b + ")";
    }
    if (kind == "implies") {
        if (!f.contains("antecedent") || !f.contains("consequent")) {
            *err = "implies: missing antecedent/consequent";
            return "";
        }
        std::string a = emit_formula(f["antecedent"], err); if (!err->empty()) return "";
        std::string c = emit_formula(f["consequent"], err); if (!err->empty()) return "";
        return "(=> " + a + " " + c + ")";
    }
    if (kind == "forall" || kind == "exists") {
        if (!f.contains("predicate") || !f["predicate"].is_object()) {
            *err = kind + ": missing predicate";
            return "";
        }
        const auto& pred = f["predicate"];
        const std::string vn = pred.value("varName", "");
        const std::string srt = pred.contains("sort") ? smt_sort(pred["sort"]) : "Int";
        if (!pred.contains("body")) { *err = kind + ": missing body"; return ""; }
        std::string body = emit_formula(pred["body"], err);
        if (!err->empty()) return "";
        return "(" + kind + " ((" + vn + " " + srt + ")) " + body + ")";
    }
    *err = "emit_formula: unknown kind '" + kind + "'";
    return "";
}

void collect_free_vars_term(const Json& t, std::map<std::string, std::string>& out, const std::set<std::string>& bound);

void collect_free_vars(const Json& f, std::map<std::string, std::string>& out, const std::set<std::string>& bound) {
    if (!f.is_object()) return;
    const std::string kind = f.value("kind", "");
    if (kind == "atomic") {
        if (f.contains("args") && f["args"].is_array()) {
            for (const auto& a : f["args"]) collect_free_vars_term(a, out, bound);
        }
    } else if (kind == "and") {
        if (f.contains("conjuncts") && f["conjuncts"].is_array()) {
            for (const auto& c : f["conjuncts"]) collect_free_vars(c, out, bound);
        }
    } else if (kind == "or") {
        if (f.contains("disjuncts") && f["disjuncts"].is_array()) {
            for (const auto& d : f["disjuncts"]) collect_free_vars(d, out, bound);
        }
    } else if (kind == "not") {
        if (f.contains("body")) collect_free_vars(f["body"], out, bound);
    } else if (kind == "implies") {
        if (f.contains("antecedent")) collect_free_vars(f["antecedent"], out, bound);
        if (f.contains("consequent")) collect_free_vars(f["consequent"], out, bound);
    } else if (kind == "forall" || kind == "exists") {
        if (f.contains("predicate") && f["predicate"].is_object()) {
            const auto& pred = f["predicate"];
            std::set<std::string> nb = bound;
            nb.insert(pred.value("varName", ""));
            if (pred.contains("body")) collect_free_vars(pred["body"], out, nb);
        }
    }
}

void collect_free_vars_term(const Json& t, std::map<std::string, std::string>& out, const std::set<std::string>& bound) {
    if (!t.is_object()) return;
    if (t.value("kind", "") == "var") {
        const std::string n = t.value("name", "");
        if (!bound.count(n)) {
            std::string srt = "Int";
            if (t.contains("sort")) srt = smt_sort(t["sort"]);
            out[n] = srt;
        }
    } else if (t.value("kind", "") == "ctor") {
        if (t.contains("args") && t["args"].is_array()) {
            for (const auto& a : t["args"]) collect_free_vars_term(a, out, bound);
        }
    }
}

}  // namespace

std::string SmtEmitter::Emit(const Json& ir_formula, std::string* err) {
    std::string body = emit_formula(ir_formula, err);
    if (!err->empty()) return "";

    std::map<std::string, std::string> free_vars;
    std::set<std::string> bound;
    collect_free_vars(ir_formula, free_vars, bound);

    std::ostringstream os;
    os << "(set-logic ALL)\n";
    for (const auto& [name, srt] : free_vars) {
        os << "(declare-const " << name << " " << srt << ")\n";
    }
    os << "(assert (not " << body << "))\n";
    os << "(check-sat)\n";
    return os.str();
}

std::string SmtEmitter::emit_term(const Json& term, std::string* err) {
    return ::provekit::verifier::emit_formula(term, err);
}

}  // namespace provekit::verifier
