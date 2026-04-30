// provekit-ir-symbolic — C++ kit (header-only, minimal v0).
//
// Mirrors the IR shape used by the TS reference (src/ir/symbolic/) and the
// Rust kit (kits/rust/provekit-ir-symbolic/). The contract is byte-identical
// compact JSON for the same logical claim.
//
// Scope: enough types and serialization to express the cross-language
// regression fixtures. Not a full parallel of the TS/Rust kits yet —
// missing connectives, exists, bridge, parseInt, full primitive set, AST
// canonicalizer. Tracked for future expansion. The minimum-viable surface
// here proves the architecture composes across four languages, which is
// the load-bearing claim.

#pragma once

#include <atomic>
#include <cstdint>
#include <map>
#include <memory>
#include <sstream>
#include <string>
#include <utility>
#include <variant>
#include <vector>

namespace provekit::ir {

// ---------------------------------------------------------------------------
// Sort
// ---------------------------------------------------------------------------

struct Sort {
  std::string name; // "Int", "Real", "String", "Bool"
};

inline Sort Int() { return Sort{"Int"}; }
inline Sort Real() { return Sort{"Real"}; }
inline Sort String() { return Sort{"String"}; }
inline Sort Bool() { return Sort{"Bool"}; }

// ---------------------------------------------------------------------------
// Term
// ---------------------------------------------------------------------------

struct Term;

struct VarTerm {
  std::string name;
  Sort sort;
};

struct ConstTerm {
  std::variant<int64_t, double, std::string, bool> value;
  Sort sort;
};

struct CtorTerm {
  std::string name;
  std::vector<std::shared_ptr<Term>> args;
  Sort sort;
};

struct Term {
  std::variant<VarTerm, ConstTerm, CtorTerm> v;
};

inline std::shared_ptr<Term> make_var(std::string name, Sort sort) {
  return std::make_shared<Term>(Term{VarTerm{std::move(name), std::move(sort)}});
}
inline std::shared_ptr<Term> num(int64_t value) {
  return std::make_shared<Term>(Term{ConstTerm{value, Int()}});
}
inline std::shared_ptr<Term> str_const(std::string value) {
  return std::make_shared<Term>(Term{ConstTerm{std::move(value), String()}});
}

// ---------------------------------------------------------------------------
// Extension authoring + primitive-bridge registry
//
// Same architectural shape as TS/Rust/Go kits: kit primitives that
// aren't actually owned by C++ (the basic JSON-shape kit semantics
// are what they are; deeper layers — LLVM, libcxx, hardware — own
// the meaning of names like parse_int). The kit BRIDGES via a
// process-local registry; verifiers resolve through the protocol.
// ---------------------------------------------------------------------------

struct PrimitiveBridgeDeclaration {
  std::string ir_name;
  std::vector<std::string> ir_arg_sorts;
  std::string ir_return_sort;
  std::string source_layer;
  std::string target_contract_cid;
  std::string target_layer;
  std::string notes;
};

inline std::map<std::string, PrimitiveBridgeDeclaration>& bridge_registry() {
  static std::map<std::string, PrimitiveBridgeDeclaration> r;
  return r;
}

inline void register_primitive_bridge(PrimitiveBridgeDeclaration decl) {
  auto& r = bridge_registry();
  auto it = r.find(decl.ir_name);
  if (it == r.end()) {
    r[decl.ir_name] = std::move(decl);
  }
  // Idempotent on collision; production verifier would error on
  // collision-with-different-target per the protocol's fail-closed gate.
}

inline std::vector<PrimitiveBridgeDeclaration> list_bridges() {
  std::vector<PrimitiveBridgeDeclaration> out;
  for (auto& [_, v] : bridge_registry()) out.push_back(v);
  return out;
}

inline void reset_registry() { bridge_registry().clear(); }

inline void ensure_kit_bridges_registered() {
  static bool done = false;
  if (done) return;
  done = true;
  // The C++ kit's bridges. Targets are placeholders pending the
  // upstream catalogs being published with signed declarations.
  register_primitive_bridge({"parseInt", {"String"}, "Int", "cpp-kit",
                              "bafy_CPP_PARSEINT_PLACEHOLDER",
                              "libcxx", ""});
}

inline std::shared_ptr<Term> parse_int(std::shared_ptr<Term> s) {
  ensure_kit_bridges_registered();
  return std::make_shared<Term>(Term{CtorTerm{"parseInt", {std::move(s)}, Int()}});
}

// ---------------------------------------------------------------------------
// Formula
// ---------------------------------------------------------------------------

struct Formula;

struct AtomicFormula {
  std::string predicate;
  std::vector<std::shared_ptr<Term>> args;
};

struct LambdaFormula {
  std::string varName;
  Sort sort;
  std::shared_ptr<Formula> body;
};

struct ForallFormula {
  Sort sort;
  std::shared_ptr<LambdaFormula> predicate;
};

struct Formula {
  std::variant<AtomicFormula, ForallFormula> v;
};

// ---------------------------------------------------------------------------
// Quantifier counter — thread-local mirror of TS / Rust kits.
// ---------------------------------------------------------------------------

inline std::atomic<int>& quantifier_counter() {
  static std::atomic<int> counter{0};
  return counter;
}

inline std::string fresh_var_name() {
  int n = quantifier_counter().fetch_add(1);
  std::ostringstream oss;
  oss << "_x" << n;
  return oss.str();
}

inline void reset_collector() {
  quantifier_counter().store(0);
}

// ---------------------------------------------------------------------------
// Atomic predicates
// ---------------------------------------------------------------------------

inline std::shared_ptr<Formula> gt(std::shared_ptr<Term> a, std::shared_ptr<Term> b) {
  return std::make_shared<Formula>(Formula{AtomicFormula{">", {std::move(a), std::move(b)}}});
}
inline std::shared_ptr<Formula> gte(std::shared_ptr<Term> a, std::shared_ptr<Term> b) {
  return std::make_shared<Formula>(Formula{AtomicFormula{"\xe2\x89\xa5", {std::move(a), std::move(b)}}});
}
inline std::shared_ptr<Formula> eq(std::shared_ptr<Term> a, std::shared_ptr<Term> b) {
  return std::make_shared<Formula>(Formula{AtomicFormula{"=", {std::move(a), std::move(b)}}});
}

// ---------------------------------------------------------------------------
// Quantifiers
// ---------------------------------------------------------------------------

template <typename Body>
std::shared_ptr<Formula> forall(Sort sort, Body body) {
  std::string vname = fresh_var_name();
  auto var = make_var(vname, sort);
  auto innerFormula = body(var);
  auto lam = std::make_shared<LambdaFormula>(LambdaFormula{vname, sort, std::move(innerFormula)});
  return std::make_shared<Formula>(Formula{ForallFormula{std::move(sort), std::move(lam)}});
}

// ---------------------------------------------------------------------------
// Property collector
// ---------------------------------------------------------------------------

struct PropertyDecl {
  std::string name;
  std::shared_ptr<Formula> formula;
};

inline std::vector<PropertyDecl>& collector() {
  static std::vector<PropertyDecl> v;
  return v;
}

inline void begin_collecting() {
  collector().clear();
}

inline void property(std::string name, std::shared_ptr<Formula> formula) {
  collector().push_back(PropertyDecl{std::move(name), std::move(formula)});
}

// `must` is the obligation-verb alias: `must("input non-empty", forall...)`
// reads as a constraint ("this is required"), where `property` is the
// neutral noun. Same storage; different authoring register.
inline void must(std::string name, std::shared_ptr<Formula> formula) {
  property(std::move(name), std::move(formula));
}

inline std::vector<PropertyDecl> finish() {
  std::vector<PropertyDecl> out;
  out.swap(collector());
  return out;
}

// ---------------------------------------------------------------------------
// JSON serialization (compact, byte-identical with TS/Rust/Go kits)
// ---------------------------------------------------------------------------

inline void write_string(std::ostringstream& out, const std::string& s) {
  out << '"';
  for (char c : s) {
    switch (c) {
      case '"': out << "\\\""; break;
      case '\\': out << "\\\\"; break;
      case '\b': out << "\\b"; break;
      case '\f': out << "\\f"; break;
      case '\n': out << "\\n"; break;
      case '\r': out << "\\r"; break;
      case '\t': out << "\\t"; break;
      default:
        if (static_cast<unsigned char>(c) < 0x20) {
          char buf[8];
          std::snprintf(buf, sizeof(buf), "\\u%04x", c);
          out << buf;
        } else {
          out << c;
        }
    }
  }
  out << '"';
}

inline void write_sort(std::ostringstream& out, const Sort& s) {
  out << "{\"kind\":\"primitive\",\"name\":";
  write_string(out, s.name);
  out << "}";
}

inline void write_term(std::ostringstream& out, const Term& t);

inline void write_var(std::ostringstream& out, const VarTerm& v) {
  out << "{\"kind\":\"var\",\"name\":";
  write_string(out, v.name);
  out << ",\"sort\":";
  write_sort(out, v.sort);
  out << "}";
}

inline void write_const(std::ostringstream& out, const ConstTerm& c) {
  out << "{\"kind\":\"const\",\"value\":";
  std::visit([&out](const auto& val) {
    using T = std::decay_t<decltype(val)>;
    if constexpr (std::is_same_v<T, int64_t>) out << val;
    else if constexpr (std::is_same_v<T, double>) out << val;
    else if constexpr (std::is_same_v<T, bool>) out << (val ? "true" : "false");
    else if constexpr (std::is_same_v<T, std::string>) {
      out << '"' << val << '"';
    }
  }, c.value);
  out << ",\"sort\":";
  write_sort(out, c.sort);
  out << "}";
}

inline void write_ctor(std::ostringstream& out, const CtorTerm& c) {
  out << "{\"kind\":\"ctor\",\"name\":";
  write_string(out, c.name);
  out << ",\"args\":[";
  for (size_t i = 0; i < c.args.size(); i++) {
    if (i > 0) out << ",";
    write_term(out, *c.args[i]);
  }
  out << "],\"sort\":";
  write_sort(out, c.sort);
  out << "}";
}

inline void write_term(std::ostringstream& out, const Term& t) {
  std::visit([&out](const auto& v) {
    using T = std::decay_t<decltype(v)>;
    if constexpr (std::is_same_v<T, VarTerm>) write_var(out, v);
    else if constexpr (std::is_same_v<T, ConstTerm>) write_const(out, v);
    else if constexpr (std::is_same_v<T, CtorTerm>) write_ctor(out, v);
  }, t.v);
}

inline void write_formula(std::ostringstream& out, const Formula& f);

inline void write_atomic(std::ostringstream& out, const AtomicFormula& a) {
  out << "{\"kind\":\"atomic\",\"predicate\":";
  write_string(out, a.predicate);
  out << ",\"args\":[";
  for (size_t i = 0; i < a.args.size(); i++) {
    if (i > 0) out << ",";
    write_term(out, *a.args[i]);
  }
  out << "]}";
}

inline void write_lambda(std::ostringstream& out, const LambdaFormula& l) {
  out << "{\"kind\":\"lambda\",\"varName\":";
  write_string(out, l.varName);
  out << ",\"sort\":";
  write_sort(out, l.sort);
  out << ",\"body\":";
  write_formula(out, *l.body);
  out << "}";
}

inline void write_forall(std::ostringstream& out, const ForallFormula& fa) {
  out << "{\"kind\":\"forall\",\"sort\":";
  write_sort(out, fa.sort);
  out << ",\"predicate\":";
  write_lambda(out, *fa.predicate);
  out << "}";
}

inline void write_formula(std::ostringstream& out, const Formula& f) {
  std::visit([&out](const auto& v) {
    using T = std::decay_t<decltype(v)>;
    if constexpr (std::is_same_v<T, AtomicFormula>) write_atomic(out, v);
    else if constexpr (std::is_same_v<T, ForallFormula>) write_forall(out, v);
  }, f.v);
}

inline std::string marshal_declarations(const std::vector<PropertyDecl>& decls) {
  std::ostringstream out;
  out << "[";
  for (size_t i = 0; i < decls.size(); i++) {
    if (i > 0) out << ",";
    out << "{\"kind\":\"property\",\"name\":";
    write_string(out, decls[i].name);
    out << ",\"formula\":";
    write_formula(out, *decls[i].formula);
    out << "}";
  }
  out << "]";
  return out.str();
}

} // namespace provekit::ir
