// SPDX-License-Identifier: Apache-2.0
//
// provekit-lift-cpp — lifts C++ contracts to canonical IR.
//
// Besides explicit provekit annotations, this lifter recognizes a narrow,
// deterministic production subset:
//   * callee preconditions from assert(...) and defensive if (...) throw;
//   * production callsites walked backward through simple local bindings;
//   * GoogleTest-style observations that describe callsite contracts.

#include "provekit/ir.hpp"

#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <iostream>
#include <regex>
#include <set>
#include <sstream>
#include <string>
#include <vector>

using namespace provekit::ir;

#ifdef _WIN32
#include <windows.h>
#else
#include <dirent.h>
#include <sys/stat.h>
#include <sys/types.h>
#endif

namespace {

struct Annotation {
    std::string function_name;
    enum Kind { Contract, Implement, Verify } kind;
    std::string target_cid;
    int line;
};

struct SourceLine {
    std::string text;
    int number;
};

struct FunctionDef {
    std::string name;
    std::vector<std::string> params;
    std::vector<SourceLine> body;
};

struct TestBlock {
    std::string name;
    std::vector<SourceLine> body;
};

struct FunctionPrecondition {
    std::string name;
    std::vector<std::string> params;
    std::shared_ptr<Formula> precondition;
};

struct ConditionFrame {
    std::shared_ptr<Formula> formula;
    int depth;
};

struct CallsiteHit {
    int line;
    int col;
    size_t stmt_index;
    std::vector<std::shared_ptr<Term>> args;
    std::vector<std::shared_ptr<Formula>> conditions;
};

struct Binding {
    std::string name;
    std::shared_ptr<Term> term;
};

struct ObservedCall {
    std::string local;
    std::string base;
    std::shared_ptr<Term> term;
};

struct ImplicationDecl {
    std::string name;
    std::string antecedent;
    std::string consequent;
    std::string antecedent_slot;
    std::string consequent_slot;
    std::string prover;
    std::string proof_witness;
};

struct LiftOutput {
    std::vector<ContractDecl> declarations;
    std::vector<ImplicationDecl> implications;
};

template <class... Ts>
struct overloaded : Ts... {
    using Ts::operator()...;
};
template <class... Ts>
overloaded(Ts...) -> overloaded<Ts...>;

std::string trim(const std::string& s) {
    size_t start = s.find_first_not_of(" \t\r\n");
    if (start == std::string::npos) return "";
    size_t end = s.find_last_not_of(" \t\r\n");
    return s.substr(start, end - start + 1);
}

bool starts_with(const std::string& s, const std::string& prefix) {
    return s.rfind(prefix, 0) == 0;
}

std::string json_quote(const std::string& s) {
    std::ostringstream out;
    write_string(out, s);
    return out.str();
}

std::string implication_json(const ImplicationDecl& imp) {
    std::ostringstream out;
    out << "{\"name\":" << json_quote(imp.name)
        << ",\"antecedent\":" << json_quote(imp.antecedent)
        << ",\"consequent\":" << json_quote(imp.consequent)
        << ",\"antecedentSlot\":" << json_quote(imp.antecedent_slot)
        << ",\"consequentSlot\":" << json_quote(imp.consequent_slot)
        << ",\"prover\":" << json_quote(imp.prover)
        << ",\"proofWitness\":" << json_quote(imp.proof_witness)
        << "}";
    return out.str();
}

std::string marshal_implications(const std::vector<ImplicationDecl>& implications) {
    std::ostringstream out;
    out << "[";
    for (size_t i = 0; i < implications.size(); i++) {
        if (i > 0) out << ",";
        out << implication_json(implications[i]);
    }
    out << "]";
    return out.str();
}

std::vector<std::string> split_lines(const std::string& text) {
    std::vector<std::string> lines;
    std::istringstream iss(text);
    std::string line;
    while (std::getline(iss, line)) lines.push_back(line);
    return lines;
}

int brace_delta(const std::string& line) {
    int delta = 0;
    for (char c : line) {
        if (c == '{') delta++;
        if (c == '}') delta--;
    }
    return delta;
}

bool only_closes_block(const std::string& line) {
    std::string s = trim(line);
    while (!s.empty() && s.back() == ';') s.pop_back();
    return trim(s) == "}";
}

std::vector<std::string> split_args_text(const std::string& text) {
    std::vector<std::string> args;
    int depth = 0;
    size_t start = 0;
    for (size_t i = 0; i < text.size(); i++) {
        if (text[i] == '(' || text[i] == '<') depth++;
        if (text[i] == ')' || text[i] == '>') depth--;
        if (text[i] == ',' && depth == 0) {
            args.push_back(trim(text.substr(start, i - start)));
            start = i + 1;
        }
    }
    std::string tail = trim(text.substr(start));
    if (!tail.empty()) args.push_back(tail);
    return args;
}

std::string extract_delimited(const std::string& line, size_t open_pos) {
    if (open_pos >= line.size() || line[open_pos] != '(') return "";
    int depth = 0;
    for (size_t i = open_pos; i < line.size(); i++) {
        if (line[i] == '(') depth++;
        if (line[i] == ')') {
            depth--;
            if (depth == 0) return trim(line.substr(open_pos + 1, i - open_pos - 1));
        }
    }
    return "";
}

std::vector<std::string> parse_param_names(const std::string& params_text) {
    std::vector<std::string> params;
    for (auto part : split_args_text(params_text)) {
        size_t eq = part.find('=');
        if (eq != std::string::npos) part = part.substr(0, eq);
        part = trim(part);
        if (part.empty() || part == "void") continue;
        while (!part.empty() && (part.back() == '&' || part.back() == '*')) part.pop_back();
        size_t end = part.find_last_not_of(" \t*&");
        if (end == std::string::npos) continue;
        size_t start = part.find_last_of(" \t*&", end);
        params.push_back(part.substr(start == std::string::npos ? 0 : start + 1, end - (start == std::string::npos ? 0 : start + 1) + 1));
    }
    return params;
}

bool parse_function_signature(const std::string& line, std::string& name, std::vector<std::string>& params) {
    std::string s = trim(line);
    if (s.empty() || s[0] == '#') return false;
    if (starts_with(s, "if") || starts_with(s, "for") || starts_with(s, "while") ||
        starts_with(s, "switch") || starts_with(s, "catch") || starts_with(s, "return") ||
        starts_with(s, "TEST") || starts_with(s, "EXPECT") || starts_with(s, "ASSERT")) {
        return false;
    }
    size_t open = s.find('(');
    size_t close = open == std::string::npos ? std::string::npos : s.find(')', open + 1);
    if (open == std::string::npos || close == std::string::npos) return false;
    if (s.find('{', close) == std::string::npos) return false;

    std::string prefix = trim(s.substr(0, open));
    std::smatch m;
    std::regex name_re(R"(([A-Za-z_][A-Za-z0-9_:]*)$)");
    if (!std::regex_search(prefix, m, name_re)) return false;
    name = m[1].str();
    size_t scope = name.rfind("::");
    if (scope != std::string::npos) name = name.substr(scope + 2);
    params = parse_param_names(s.substr(open + 1, close - open - 1));
    return true;
}

bool parse_test_name(const std::string& line, std::string& name) {
    std::smatch m;
    std::regex test_re(R"(^\s*TEST(?:_F|_P)?\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)\s*,\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)\s*\{)");
    if (!std::regex_search(line, m, test_re)) return false;
    name = m[1].str() + "." + m[2].str();
    return true;
}

std::vector<SourceLine> collect_body(const std::vector<std::string>& lines, size_t& i) {
    std::vector<SourceLine> body;
    int depth = brace_delta(lines[i]);
    while (depth > 0 && i + 1 < lines.size()) {
        i++;
        int next_depth = depth + brace_delta(lines[i]);
        if (!(next_depth <= 0 && only_closes_block(lines[i]))) {
            body.push_back({lines[i], static_cast<int>(i + 1)});
        }
        depth = next_depth;
    }
    return body;
}

void collect_top_level_blocks(const std::string& text,
                              std::vector<FunctionDef>& functions,
                              std::vector<TestBlock>& tests) {
    auto lines = split_lines(text);
    for (size_t i = 0; i < lines.size(); i++) {
        std::string test_name;
        if (parse_test_name(lines[i], test_name)) {
            tests.push_back({test_name, collect_body(lines, i)});
            continue;
        }
        std::string fn_name;
        std::vector<std::string> params;
        if (parse_function_signature(lines[i], fn_name, params)) {
            functions.push_back({fn_name, params, collect_body(lines, i)});
        }
    }
}

std::string find_ahead_fn(const std::vector<std::string>& lines, int start_line) {
    int max_line = std::min<int>(static_cast<int>(lines.size()), start_line + 10);
    for (int i = start_line + 1; i < max_line; i++) {
        std::string name;
        std::vector<std::string> params;
        if (parse_function_signature(lines[i], name, params)) return name;
    }
    return "unknown";
}

std::vector<Annotation> scan_file(const std::string& text) {
    std::vector<Annotation> anns;
    auto lines = split_lines(text);
    for (size_t i = 0; i < lines.size(); i++) {
        std::string trimmed = trim(lines[i]);
        if (starts_with(trimmed, "//provekit:contract")) {
            anns.push_back({find_ahead_fn(lines, static_cast<int>(i)), Annotation::Contract, "", static_cast<int>(i)});
        } else if (starts_with(trimmed, "//provekit:implement")) {
            anns.push_back({find_ahead_fn(lines, static_cast<int>(i)), Annotation::Implement, trim(trimmed.substr(20)), static_cast<int>(i)});
        } else if (starts_with(trimmed, "//provekit:verify")) {
            anns.push_back({find_ahead_fn(lines, static_cast<int>(i)), Annotation::Verify, "", static_cast<int>(i)});
        }
    }
    return anns;
}

void lift_annotations(std::vector<ContractDecl>& decls, const std::vector<Annotation>& anns) {
    for (const auto& ann : anns) {
        if (ann.kind == Annotation::Contract) {
            decls.push_back(ContractDecl{ann.function_name, nullptr, atomic_("true", {}), nullptr, "out", nullptr});
        }
    }
}

std::string extract_assert_condition(const std::string& line) {
    size_t pos = line.find("std::assert(");
    if (pos != std::string::npos) return extract_delimited(line, pos + std::string("std::assert").size());
    pos = line.find("assert(");
    if (pos != std::string::npos) return extract_delimited(line, pos + std::string("assert").size());
    return "";
}

std::string extract_if_condition(const std::string& line) {
    std::string s = trim(line);
    if (!starts_with(s, "if")) return "";
    size_t open = line.find('(');
    if (open == std::string::npos) return "";
    return extract_delimited(line, open);
}

bool line_throws(const std::string& line) {
    return line.find("throw") != std::string::npos;
}

std::shared_ptr<Term> term_from_expr(const std::string& expr);

std::shared_ptr<Formula> lift_formula_with_ops(const std::string& expr, bool negate) {
    struct Op {
        const char* text;
        const char* name;
        const char* inverse;
    };
    static const Op ops[] = {
        {">=", "\xe2\x89\xa5", "<"},
        {"<=", "\xe2\x89\xa4", ">"},
        {"==", "=", "\xe2\x89\xa0"},
        {"!=", "\xe2\x89\xa0", "="},
        {">", ">", "\xe2\x89\xa4"},
        {"<", "<", "\xe2\x89\xa5"},
    };

    std::string s = trim(expr);
    while (s.size() >= 2 && s.front() == '(' && s.back() == ')') {
        s = trim(s.substr(1, s.size() - 2));
    }
    for (const auto& op : ops) {
        size_t pos = s.find(op.text);
        if (pos == std::string::npos) continue;
        auto left = term_from_expr(s.substr(0, pos));
        auto right = term_from_expr(s.substr(pos + std::string(op.text).size()));
        return atomic_(negate ? op.inverse : op.name, {left, right});
    }
    return atomic_("=", {term_from_expr(s), std::make_shared<Term>(Term{ConstTerm{!negate, Bool()}})});
}

std::shared_ptr<Formula> lift_formula(const std::string& expr) {
    return lift_formula_with_ops(expr, false);
}

std::shared_ptr<Formula> lift_negated_formula(const std::string& expr) {
    return lift_formula_with_ops(expr, true);
}

std::vector<std::shared_ptr<Term>> call_args_at(const std::string& line, size_t open_paren) {
    std::vector<std::shared_ptr<Term>> out;
    for (const auto& arg : split_args_text(extract_delimited(line, open_paren))) {
        out.push_back(term_from_expr(arg));
    }
    return out;
}

std::shared_ptr<Term> term_from_expr(const std::string& expr) {
    std::string s = trim(expr);
    while (!s.empty() && s.back() == ';') s.pop_back();
    s = trim(s);
    while (s.size() >= 2 && s.front() == '(' && s.back() == ')') {
        s = trim(s.substr(1, s.size() - 2));
    }
    if (s.empty()) return make_var("");

    char* end = nullptr;
    long long n = std::strtoll(s.c_str(), &end, 10);
    if (end != nullptr && *end == '\0') return num(n);
    if (s == "true") return std::make_shared<Term>(Term{ConstTerm{true, Bool()}});
    if (s == "false") return std::make_shared<Term>(Term{ConstTerm{false, Bool()}});
    if (s.size() >= 2 && s.front() == '"' && s.back() == '"') return str_const(s.substr(1, s.size() - 2));

    size_t open = s.find('(');
    if (open != std::string::npos && s.back() == ')') {
        std::string name = trim(s.substr(0, open));
        std::vector<std::shared_ptr<Term>> args;
        for (const auto& arg : split_args_text(s.substr(open + 1, s.size() - open - 2))) {
            args.push_back(term_from_expr(arg));
        }
        return std::make_shared<Term>(Term{CtorTerm{name, args}});
    }

    return make_var(s);
}

std::shared_ptr<Term> substitute_term(const std::shared_ptr<Term>& term,
                                      const std::string& name,
                                      const std::shared_ptr<Term>& replacement) {
    return std::visit(overloaded{
        [&](const VarTerm& v) -> std::shared_ptr<Term> {
            if (v.name == name) return replacement;
            return term;
        },
        [&](const ConstTerm&) -> std::shared_ptr<Term> { return term; },
        [&](const CtorTerm& c) -> std::shared_ptr<Term> {
            std::vector<std::shared_ptr<Term>> args;
            for (const auto& arg : c.args) args.push_back(substitute_term(arg, name, replacement));
            return std::make_shared<Term>(Term{CtorTerm{c.name, args}});
        },
        [&](const LambdaTerm& l) -> std::shared_ptr<Term> {
            if (l.paramName == name) return term;
            return std::make_shared<Term>(Term{LambdaTerm{l.paramName, l.paramSort, substitute_term(l.body, name, replacement)}});
        },
        [&](const LetTerm& l) -> std::shared_ptr<Term> {
            std::vector<LetBinding> bindings;
            for (const auto& b : l.bindings) bindings.push_back({b.name, substitute_term(b.boundTerm, name, replacement)});
            return std::make_shared<Term>(Term{LetTerm{bindings, substitute_term(l.body, name, replacement)}});
        },
    }, term->v);
}

std::shared_ptr<Formula> substitute_formula(const std::shared_ptr<Formula>& formula,
                                            const std::string& name,
                                            const std::shared_ptr<Term>& replacement) {
    return std::visit(overloaded{
        [&](const AtomicFormula& a) -> std::shared_ptr<Formula> {
            std::vector<std::shared_ptr<Term>> args;
            for (const auto& arg : a.args) args.push_back(substitute_term(arg, name, replacement));
            return atomic_(a.name, args);
        },
        [&](const ConnectiveFormula& c) -> std::shared_ptr<Formula> {
            std::vector<std::shared_ptr<Formula>> operands;
            for (const auto& op : c.operands) operands.push_back(substitute_formula(op, name, replacement));
            return connective_(c.kind, operands);
        },
        [&](const QuantifierFormula&) -> std::shared_ptr<Formula> { return formula; },
        [&](const ChoiceFormula&) -> std::shared_ptr<Formula> { return formula; },
    }, formula->v);
}

std::shared_ptr<Formula> combine_and(const std::vector<std::shared_ptr<Formula>>& formulas) {
    if (formulas.size() == 1) return formulas[0];
    return and_(formulas);
}

std::string unique_name(const std::string& raw, std::set<std::string>& used_names) {
    if (used_names.insert(raw).second) return raw;
    int i = 1;
    while (true) {
        std::string candidate = raw + "::" + std::to_string(i++);
        if (used_names.insert(candidate).second) return candidate;
    }
}

std::shared_ptr<Formula> lift_function_precondition_formula(const FunctionDef& function) {
    std::vector<std::shared_ptr<Formula>> formulas;
    for (size_t i = 0; i < function.body.size(); i++) {
        std::string trimmed = trim(function.body[i].text);
        std::string asserted = extract_assert_condition(trimmed);
        if (!asserted.empty()) {
            formulas.push_back(lift_formula(asserted));
            continue;
        }
        std::string condition = extract_if_condition(trimmed);
        if (!condition.empty() &&
            (line_throws(trimmed) || (i + 1 < function.body.size() && line_throws(function.body[i + 1].text)))) {
            formulas.push_back(lift_negated_formula(condition));
        }
    }
    if (formulas.empty()) return nullptr;
    return combine_and(formulas);
}

std::vector<FunctionPrecondition> collect_preconditions(const std::vector<FunctionDef>& functions) {
    std::vector<FunctionPrecondition> out;
    for (const auto& function : functions) {
        auto pre = lift_function_precondition_formula(function);
        if (pre) out.push_back({function.name, function.params, pre});
    }
    return out;
}

std::vector<CallsiteHit> find_callsites(const FunctionDef& caller, const std::string& callee_name) {
    std::vector<CallsiteHit> hits;
    std::vector<ConditionFrame> conditions;
    int depth = 0;
    std::string needle = callee_name + "(";
    for (size_t idx = 0; idx < caller.body.size(); idx++) {
        while (!conditions.empty() && conditions.back().depth > depth) conditions.pop_back();
        std::string maybe_condition = extract_if_condition(caller.body[idx].text);
        std::shared_ptr<Formula> lifted_condition = maybe_condition.empty() ? nullptr : lift_formula(maybe_condition);

        size_t pos = caller.body[idx].text.find(needle);
        if (pos != std::string::npos) {
            std::vector<std::shared_ptr<Formula>> conds;
            for (const auto& condition : conditions) conds.push_back(condition.formula);
            if (lifted_condition) conds.push_back(lifted_condition);
            hits.push_back({
                caller.body[idx].number,
                static_cast<int>(pos + 1),
                idx,
                call_args_at(caller.body[idx].text, pos + callee_name.size()),
                conds,
            });
        }
        if (lifted_condition && caller.body[idx].text.find('{') != std::string::npos) {
            conditions.push_back({lifted_condition, depth + 1});
        }
        depth += brace_delta(caller.body[idx].text);
    }
    return hits;
}

bool binding_from_line(const std::string& line, Binding& binding) {
    std::string s = trim(line);
    std::smatch m;
    std::regex decl_re(R"(^\s*(?:const\s+)?(?:auto|int|long|size_t|bool|double|float|std::string)\s+([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.+?)\s*;\s*$)");
    if (std::regex_match(s, m, decl_re)) {
        binding = {m[1].str(), term_from_expr(m[2].str())};
        return true;
    }
    std::regex assign_re(R"(^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.+?)\s*;\s*$)");
    if (std::regex_match(s, m, assign_re)) {
        binding = {m[1].str(), term_from_expr(m[2].str())};
        return true;
    }
    return false;
}

void append_edge(LiftOutput& out,
                 std::set<std::string>& used_names,
                 const std::string& raw_name,
                 const std::shared_ptr<Formula>& pre,
                 const std::shared_ptr<Formula>& post,
                 const std::string& caller,
                 const std::string& callee,
                 const std::string& prover) {
    std::string name = unique_name(raw_name, used_names);
    out.declarations.push_back(ContractDecl{name, pre, post, nullptr, "result", nullptr});
    out.implications.push_back({
        name + "::pre-implies-post",
        name,
        name,
        "pre",
        "post",
        prover,
        caller + "->" + callee,
    });
}

void emit_walks_for_callee(const FunctionDef& caller,
                           const FunctionPrecondition& callee,
                           const std::string& source_file,
                           LiftOutput& out,
                           std::set<std::string>& used_names) {
    for (const auto& hit : find_callsites(caller, callee.name)) {
        if (hit.args.size() != callee.params.size()) continue;
        auto wp = callee.precondition;
        for (size_t i = 0; i < callee.params.size(); i++) {
            wp = substitute_formula(wp, callee.params[i], hit.args[i]);
        }
        if (!hit.conditions.empty()) {
            wp = implies(combine_and(hit.conditions), wp);
        }

        std::string base = callee.name + "@" + source_file + ":" +
                           std::to_string(hit.line) + ":" + std::to_string(hit.col);
        append_edge(out, used_names, base + "::callsite", wp, wp, caller.name, callee.name, "cpp-wp-walk");

        auto previous_wp = wp;
        size_t i = hit.stmt_index;
        while (i > 0) {
            i--;
            Binding binding;
            if (binding_from_line(caller.body[i].text, binding)) {
                auto next_wp = substitute_formula(previous_wp, binding.name, binding.term);
                append_edge(out, used_names, base + "::let:" + binding.name, next_wp, previous_wp, caller.name, callee.name, "cpp-wp-walk");
                previous_wp = next_wp;
            }
        }
        append_edge(out, used_names, base + "::entry", previous_wp, previous_wp, caller.name, callee.name, "cpp-wp-walk");
    }
}

void lift_production_walk(const std::vector<FunctionDef>& functions,
                          const std::string& source_file,
                          LiftOutput& out) {
    auto preconditions = collect_preconditions(functions);
    std::set<std::string> used_names;
    for (const auto& caller : functions) {
        for (const auto& callee : preconditions) {
            if (caller.name == callee.name) continue;
            emit_walks_for_callee(caller, callee, source_file, out, used_names);
        }
    }
}

bool call_name_from_term(const std::shared_ptr<Term>& term, std::string& name) {
    if (auto ctor = std::get_if<CtorTerm>(&term->v)) {
        name = ctor->name;
        return true;
    }
    return false;
}

bool is_known_function(const std::vector<FunctionDef>& functions, const std::string& name) {
    return std::any_of(functions.begin(), functions.end(), [&](const FunctionDef& f) { return f.name == name; });
}

bool observed_call_binding(const SourceLine& line,
                           const std::vector<FunctionDef>& functions,
                           const std::string& source_file,
                           ObservedCall& observed) {
    Binding binding;
    if (!binding_from_line(line.text, binding)) return false;
    std::string call_name;
    if (!call_name_from_term(binding.term, call_name) || !is_known_function(functions, call_name)) return false;
    std::string needle = call_name + "(";
    size_t pos = line.text.find(needle);
    observed = {
        binding.name,
        call_name + "@" + source_file + ":" + std::to_string(line.number) + ":" + std::to_string(pos == std::string::npos ? 1 : pos + 1),
        binding.term,
    };
    return true;
}

const ObservedCall* find_observed_by_local(const std::vector<ObservedCall>& observed, const std::string& local) {
    std::string needle = trim(local);
    for (const auto& call : observed) {
        if (call.local == needle) return &call;
    }
    return nullptr;
}

void append_test_value_scope(LiftOutput& out,
                             std::set<std::string>& used_names,
                             const ObservedCall& call,
                             const std::shared_ptr<Formula>& assertion,
                             const std::string& test_name) {
    std::string facts_name = unique_name(call.base + "::facts", used_names);
    std::string assertion_name = unique_name(call.base + "::assertion", used_names);
    out.declarations.push_back(ContractDecl{facts_name, nullptr, nullptr, eq(make_var(call.local), call.term), "out", nullptr});
    out.declarations.push_back(ContractDecl{assertion_name, nullptr, nullptr, assertion, "out", nullptr});
    out.implications.push_back({
        unique_name(call.base + "::facts-implies-assertion", used_names),
        facts_name,
        assertion_name,
        "inv",
        "inv",
        "cpp-test-value-scope",
        test_name + " assertion",
    });
}

bool parse_expectation(const std::string& line, std::string& macro, std::vector<std::string>& args) {
    std::smatch m;
    std::regex expect_re(R"(\b(?:EXPECT|ASSERT)_(EQ|NE|TRUE)\s*\((.*)\)\s*;)");
    if (!std::regex_search(line, m, expect_re)) return false;
    macro = m[1].str();
    args = split_args_text(m[2].str());
    return true;
}

void lift_cpp_tests(const std::vector<FunctionDef>& functions,
                    const std::vector<TestBlock>& tests,
                    const std::string& source_file,
                    LiftOutput& out) {
    std::set<std::string> used_names;
    for (const auto& test : tests) {
        std::vector<ObservedCall> observed;
        for (const auto& line : test.body) {
            ObservedCall call;
            if (observed_call_binding(line, functions, source_file, call)) {
                observed.push_back(call);
                continue;
            }

            std::string macro;
            std::vector<std::string> args;
            if (!parse_expectation(line.text, macro, args)) continue;
            if ((macro == "EQ" || macro == "NE") && args.size() >= 2) {
                const ObservedCall* lhs_call = find_observed_by_local(observed, args[0]);
                const ObservedCall* rhs_call = find_observed_by_local(observed, args[1]);
                const ObservedCall* source_call = lhs_call ? lhs_call : rhs_call;
                if (!source_call) continue;
                auto lhs = lhs_call ? source_call->term : term_from_expr(args[0]);
                auto rhs = rhs_call ? source_call->term : term_from_expr(args[1]);
                auto assertion = atomic_(macro == "EQ" ? "=" : "\xe2\x89\xa0", {lhs, rhs});
                append_test_value_scope(out, used_names, *source_call, assertion, test.name);
            } else if (macro == "TRUE" && args.size() == 1) {
                for (const auto& call_item : observed) {
                    if (args[0].find(call_item.local) == std::string::npos) continue;
                    append_test_value_scope(out, used_names, call_item, lift_formula(args[0]), test.name);
                    break;
                }
            }
        }
    }
}

LiftOutput lift_source(const std::string& text, const std::string& source_file) {
    LiftOutput out;
    lift_annotations(out.declarations, scan_file(text));
    std::vector<FunctionDef> functions;
    std::vector<TestBlock> tests;
    collect_top_level_blocks(text, functions, tests);
    lift_cpp_tests(functions, tests, source_file, out);
    lift_production_walk(functions, source_file, out);
    return out;
}

void walk_files(const std::string& dir_path, std::vector<std::string>& files) {
#ifdef _WIN32
    WIN32_FIND_DATAA fd;
    std::string pattern = dir_path + "\\*";
    HANDLE h = FindFirstFileA(pattern.c_str(), &fd);
    if (h == INVALID_HANDLE_VALUE) return;
    do {
        std::string name = fd.cFileName;
        if (name == "." || name == "..") continue;
        std::string full = dir_path + "\\" + name;
        if (fd.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) {
            walk_files(full, files);
        } else {
            std::string ext = name.substr(name.find_last_of('.') + 1);
            if (ext == "cpp" || ext == "cc" || ext == "cxx" || ext == "hpp" || ext == "h") files.push_back(full);
        }
    } while (FindNextFileA(h, &fd));
    FindClose(h);
#else
    DIR* dir = opendir(dir_path.c_str());
    if (!dir) return;
    struct dirent* entry;
    while ((entry = readdir(dir)) != nullptr) {
        std::string name = entry->d_name;
        if (name == "." || name == "..") continue;
        std::string full = dir_path + "/" + name;
        struct stat st;
        if (stat(full.c_str(), &st) != 0) continue;
        if (S_ISDIR(st.st_mode)) {
            walk_files(full, files);
        } else {
            std::string ext;
            auto dot_pos = name.find_last_of('.');
            if (dot_pos != std::string::npos) ext = name.substr(dot_pos + 1);
            if (ext == "cpp" || ext == "cc" || ext == "cxx" || ext == "hpp" || ext == "h") files.push_back(full);
        }
    }
    closedir(dir);
#endif
}

std::string read_file(const std::string& path) {
    std::ifstream f(path);
    if (!f) return "";
    return std::string((std::istreambuf_iterator<char>(f)), std::istreambuf_iterator<char>());
}

std::string source_file_name(const std::string& path) {
    size_t slash = path.find_last_of("/\\");
    return slash == std::string::npos ? path : path.substr(slash + 1);
}

std::string extract_id(const std::string& line) {
    std::regex id_re(R"("id"\s*:\s*(\d+|null))");
    std::smatch m;
    if (std::regex_search(line, m, id_re)) return m[1].str();
    return "null";
}

std::string extract_string(const std::string& line, const std::string& key) {
    std::string search = "\"" + key + "\"";
    size_t pos = line.find(search);
    if (pos == std::string::npos) return "";
    size_t colon = line.find(':', pos + search.size());
    if (colon == std::string::npos) return "";
    size_t vstart = line.find_first_not_of(" \t", colon + 1);
    if (vstart == std::string::npos || line[vstart] != '"') return "";
    std::string result;
    for (size_t j = vstart + 1; j < line.size(); j++) {
        if (line[j] == '\\' && j + 1 < line.size()) {
            result += line[j];
            result += line[j + 1];
            j++;
        } else if (line[j] == '"') {
            break;
        } else {
            result += line[j];
        }
    }
    return result;
}

std::string unescape_json(const std::string& s) {
    std::string out;
    for (size_t i = 0; i < s.size(); i++) {
        if (s[i] == '\\' && i + 1 < s.size()) {
            switch (s[i + 1]) {
                case '"': out += '"'; i++; break;
                case '\\': out += '\\'; i++; break;
                case 'n': out += '\n'; i++; break;
                case 'r': out += '\r'; i++; break;
                case 't': out += '\t'; i++; break;
                default: out += s[i]; break;
            }
        } else {
            out += s[i];
        }
    }
    return out;
}

void send_result(const std::string& id, const std::string& result_json) {
    std::cout << "{\"jsonrpc\":\"2.0\",\"id\":" << id
              << ",\"result\":" << result_json << "}\n"
              << std::flush;
}

void send_error(const std::string& id, int code, const std::string& msg) {
    std::cout << "{\"jsonrpc\":\"2.0\",\"id\":" << id
              << ",\"error\":{\"code\":" << code
              << ",\"message\":" << json_quote(msg) << "}}\n"
              << std::flush;
}

std::string lift_result_json(const LiftOutput& out) {
    return "{\"kind\":\"ir-document\",\"ir\":" + marshal_declarations(out.declarations) +
           ",\"implications\":" + marshal_implications(out.implications) +
           ",\"diagnostics\":[]}";
}

void rpc_mode(const std::string& workspace_arg) {
    std::string line;
    while (std::getline(std::cin, line)) {
        if (line.empty()) continue;
        std::string id = extract_id(line);

        if (line.find("\"initialize\"") != std::string::npos) {
            send_result(id, "{\"name\":\"provekit-lift-cpp\",\"version\":\"0.1.0\",\"capabilities\":[\"parse\",\"lift\"]}");
        } else if (line.find("\"parse\"") != std::string::npos) {
            std::string source = unescape_json(extract_string(line, "source"));
            std::string path = unescape_json(extract_string(line, "path"));
            if (path.empty()) path = "input.cpp";
            auto out = lift_source(source, source_file_name(path));
            std::string result = "{\"declarations\":" + marshal_declarations(out.declarations) +
                                 ",\"callEdges\":[],\"implications\":" + marshal_implications(out.implications) +
                                 ",\"warnings\":[]}";
            send_result(id, result);
        } else if (line.find("\"lift\"") != std::string::npos) {
            std::string workspace = unescape_json(extract_string(line, "workspace_root"));
            if (workspace.empty()) workspace = workspace_arg;
            std::vector<std::string> files;
            walk_files(workspace, files);
            LiftOutput all;
            for (const auto& path : files) {
                std::string text = read_file(path);
                if (text.empty()) continue;
                auto out = lift_source(text, source_file_name(path));
                all.declarations.insert(all.declarations.end(), out.declarations.begin(), out.declarations.end());
                all.implications.insert(all.implications.end(), out.implications.begin(), out.implications.end());
            }
            send_result(id, lift_result_json(all));
        } else if (line.find("\"shutdown\"") != std::string::npos) {
            send_result(id, "null");
            break;
        } else {
            send_error(id, -32601, "unknown method");
        }
    }
}

void standalone_mode(const std::string& workspace_path, const std::string& output_path) {
    std::vector<std::string> files;
    walk_files(workspace_path, files);
    LiftOutput all;
    for (const auto& path : files) {
        std::string text = read_file(path);
        if (text.empty()) continue;
        auto out = lift_source(text, source_file_name(path));
        all.declarations.insert(all.declarations.end(), out.declarations.begin(), out.declarations.end());
        all.implications.insert(all.implications.end(), out.implications.begin(), out.implications.end());
    }

    std::string json = marshal_declarations(all.declarations);
    if (!output_path.empty()) {
        std::string out_file = output_path + "/lifted.json";
        std::ofstream f(out_file);
        if (f) {
            f << json << "\n";
            std::printf("Wrote %zu declarations to %s\n", all.declarations.size(), out_file.c_str());
        }
    } else {
        std::cout << json << "\n";
    }
}

}  // namespace

int main(int argc, char* argv[]) {
    bool rpc_flag = false;
    std::string workspace = ".";
    std::string output;

    for (int i = 1; i < argc; i++) {
        std::string arg = argv[i];
        if (arg == "--rpc") {
            rpc_flag = true;
        } else if (arg == "--workspace" && i + 1 < argc) {
            workspace = argv[++i];
        } else if ((arg == "-o" || arg == "--out") && i + 1 < argc) {
            output = argv[++i];
        }
    }

    if (rpc_flag) {
        rpc_mode(workspace);
    } else {
        standalone_mode(workspace, output);
    }
    return 0;
}
