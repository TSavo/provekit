// SPDX-License-Identifier: Apache-2.0

#include "smt_emitter.hpp"

#include <map>
#include <set>
#include <sstream>

namespace provekit::verifier {

namespace {

std::string smt_atomic_name(const std::string& n) {
    if (n == "\xe2\x89\xa0") return "distinct";  // ≠
    if (n == "\xe2\x89\xa4") return "<=";          // ≤
    if (n == "\xe2\x89\xa5") return ">=";          // ≥
    return n;
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
        // New IR: atomic uses `name` (not `predicate`).
        const std::string nm = f.value("name", "");
        const std::string smt_n = smt_atomic_name(nm);
        if (!f.contains("args") || !f["args"].is_array()) { *err = "atomic: no args"; return ""; }
        std::ostringstream os;
        os << "(" << smt_n;
        for (const auto& a : f["args"]) {
            std::string s = emit_term(a, err);
            if (!err->empty()) return "";
            os << " " << s;
        }
        os << ")";
        return os.str();
    }
    // Connectives: unified `operands` array.
    if (kind == "and" || kind == "or" || kind == "not" || kind == "implies") {
        if (!f.contains("operands") || !f["operands"].is_array()) {
            *err = kind + ": missing operands";
            return "";
        }
        const auto& ops = f["operands"];
        const char* smt_op = kind == "implies" ? "=>" : kind.c_str();
        std::ostringstream os;
        os << "(" << smt_op;
        for (const auto& op : ops) {
            std::string s = emit_formula(op, err);
            if (!err->empty()) return "";
            os << " " << s;
        }
        os << ")";
        return os.str();
    }
    if (kind == "forall" || kind == "exists") {
        // Flat quantifier shape: {kind, name, sort, body}.
        const std::string vn = f.value("name", "");
        const std::string srt = f.contains("sort") ? smt_sort(f["sort"]) : "Int";
        if (!f.contains("body")) { *err = kind + ": missing body"; return ""; }
        std::string body = emit_formula(f["body"], err);
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
        return;
    }
    if (kind == "and" || kind == "or" || kind == "not" || kind == "implies") {
        if (f.contains("operands") && f["operands"].is_array()) {
            for (const auto& op : f["operands"]) collect_free_vars(op, out, bound);
        }
        return;
    }
    if (kind == "forall" || kind == "exists") {
        std::set<std::string> nb = bound;
        nb.insert(f.value("name", ""));
        if (f.contains("body")) collect_free_vars(f["body"], out, nb);
        return;
    }
}

void collect_free_vars_term(const Json& t, std::map<std::string, std::string>& out, const std::set<std::string>& bound) {
    if (!t.is_object()) return;
    if (t.value("kind", "") == "var") {
        const std::string n = t.value("name", "");
        if (!bound.count(n)) {
            // VarTerm carries no sort under the new IR; default to Int when
            // we can't infer (the caller may override via context-aware
            // sort-derivation in a future commit).
            out[n] = "Int";
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
