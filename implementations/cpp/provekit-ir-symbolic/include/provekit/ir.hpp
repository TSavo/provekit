// provekit-ir-symbolic , C++ kit (header-only).
//
// Maximal-uniformity IR per protocol/specs/2026-04-30-ir-formal-grammar.md
// (v1.1.0 catalog). Every node has `kind`, then `name` (when applicable),
// then payload. Five formula kinds, three term kinds. Reader holds the
// entire IR in their head.
//
// Authoring surface follows the per-language kit standard:
//   contract(name, { pre, post, inv, outBinding? })
//   must(name, precondition)         -- alias for contract(.., {pre: precondition})
//   forall(sort, body) / exists(sort, body)
//   and_(...), or_(...), not_(...), implies(antecedent, consequent)
//   eq, ne, gt, gte, lt, lte         -- atomic predicates
//   num, str_const, parse_int        -- term primitives
//   out()                            -- references the return value in a post

#pragma once

#include <atomic>
#include <cstdint>
#include <cstdio>
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
  std::string name;  // "Int", "Real", "String", "Bool"
};

inline Sort Int() { return Sort{"Int"}; }
inline Sort Real() { return Sort{"Real"}; }
inline Sort String() { return Sort{"String"}; }
inline Sort Bool() { return Sort{"Bool"}; }

// ---------------------------------------------------------------------------
// Term , VarTerm (no sort), ConstTerm (sort kept), CtorTerm (no sort).
// ---------------------------------------------------------------------------

struct Term;

struct VarTerm {
  std::string name;
};

struct ConstTerm {
  std::variant<int64_t, double, std::string, bool> value;
  Sort sort;
};

struct CtorTerm {
  std::string name;
  std::vector<std::shared_ptr<Term>> args;
};

struct LambdaTerm {
  std::string paramName;
  Sort paramSort;
  std::shared_ptr<Term> body;
};

struct LetBinding {
  std::string name;
  std::shared_ptr<Term> boundTerm;
};

struct LetTerm {
  std::vector<LetBinding> bindings;
  std::shared_ptr<Term> body;
};

struct Term {
  std::variant<VarTerm, ConstTerm, CtorTerm, LambdaTerm, LetTerm> v;
};

inline std::shared_ptr<Term> make_var(std::string name) {
  return std::make_shared<Term>(Term{VarTerm{std::move(name)}});
}
inline std::shared_ptr<Term> num(int64_t value) {
  return std::make_shared<Term>(Term{ConstTerm{value, Int()}});
}
inline std::shared_ptr<Term> str_const(std::string value) {
  return std::make_shared<Term>(Term{ConstTerm{std::move(value), String()}});
}

// ---------------------------------------------------------------------------
// Bridge registry (process-local; serialized into bridge mementos at mint).
// ---------------------------------------------------------------------------

struct PrimitiveBridgeDeclaration {
  std::string ir_name;
  std::vector<std::string> ir_arg_sorts;
  std::string ir_return_sort;
  std::string source_layer;
  // Optional CID of the source-layer contract this bridge originates from.
  // Empty string == absent. Mirrors v1.3.0 BridgeDeclaration.sourceContractCid
  // per protocol/specs/2026-04-30-ir-formal-grammar.md §BridgeDeclaration;
  // bridges that pin both the source-side and target-side contracts make the
  // implication chain hash-bounded across bundles.
  std::string source_contract_cid;
  std::string target_contract_cid;
  // Optional CID of the .proof bundle that contains the target contract.
  // Empty string == absent. Mirrors v1.3.0 BridgeDeclaration.targetProofCid
  // (the forward pin from a bridge to a specific consequent proof bundle).
  // Without this, verifiers must scan all available .proof files; with it,
  // lookup is O(1) by CID and the bundle reference is tamper-evident.
  std::string target_proof_cid;
  std::string target_layer;
  std::string notes;
};

inline std::map<std::string, PrimitiveBridgeDeclaration>& bridge_registry() {
  static std::map<std::string, PrimitiveBridgeDeclaration> r;
  return r;
}

inline void register_primitive_bridge(PrimitiveBridgeDeclaration decl) {
  auto& r = bridge_registry();
  if (r.find(decl.ir_name) == r.end()) {
    r[decl.ir_name] = std::move(decl);
  }
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
  // Field order matches PrimitiveBridgeDeclaration: ir_name, ir_arg_sorts,
  // ir_return_sort, source_layer, source_contract_cid (v1.3.0 add),
  // target_contract_cid, target_proof_cid (v1.3.0 add), target_layer, notes.
  // The two v1.3.0 CID slots are empty placeholders here; concrete kits set
  // them when minting a real bridge envelope. The Rust peer's BridgeDecl in
  // implementations/rust/provekit-ir-symbolic/src/lib.rs follows the same
  // optional-empty pattern.
  register_primitive_bridge({"parseInt", {"String"}, "Int", "cpp-kit",
                             "", "bafy_CPP_PARSEINT_PLACEHOLDER", "",
                             "libcxx", ""});
}

inline std::shared_ptr<Term> parse_int(std::shared_ptr<Term> s) {
  ensure_kit_bridges_registered();
  return std::make_shared<Term>(Term{CtorTerm{"parseInt", {std::move(s)}}});
}

// out() , references the return value within a post formula. Compiles to
// a VarTerm whose `name` matches the enclosing contract's outBinding
// (default "out"). The kit's contract() primitive enforces outBinding=out;
// custom outBindings can use make_var(name) directly.
inline std::shared_ptr<Term> out() {
  return std::make_shared<Term>(Term{VarTerm{"out"}});
}

// ---------------------------------------------------------------------------
// Formula , three kinds: AtomicFormula, ConnectiveFormula, QuantifierFormula.
// ---------------------------------------------------------------------------

struct Formula;

struct AtomicFormula {
  std::string name;  // ">", "=", "≥", "≠", "<", "≤", or kit-defined
  std::vector<std::shared_ptr<Term>> args;
};

struct ConnectiveFormula {
  std::string kind;  // "and" / "or" / "not" / "implies"
  std::vector<std::shared_ptr<Formula>> operands;
};

struct QuantifierFormula {
  std::string kind;  // "forall" / "exists"
  std::string name;  // bound variable identifier
  Sort sort;
  std::shared_ptr<Formula> body;
};

struct ChoiceFormula {
  std::string varName;
  Sort sort;
  std::shared_ptr<Formula> body;
};

struct Formula {
  std::variant<AtomicFormula, ConnectiveFormula, QuantifierFormula, ChoiceFormula> v;
};

// ---------------------------------------------------------------------------
// Quantifier counter , fresh names for bound variables.
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

inline std::shared_ptr<Formula> atomic_(std::string name, std::vector<std::shared_ptr<Term>> args) {
  return std::make_shared<Formula>(Formula{AtomicFormula{std::move(name), std::move(args)}});
}
inline std::shared_ptr<Formula> gt(std::shared_ptr<Term> a, std::shared_ptr<Term> b) {
  return atomic_(">", {std::move(a), std::move(b)});
}
inline std::shared_ptr<Formula> gte(std::shared_ptr<Term> a, std::shared_ptr<Term> b) {
  return atomic_("\xe2\x89\xa5", {std::move(a), std::move(b)});  // ≥
}
inline std::shared_ptr<Formula> lt(std::shared_ptr<Term> a, std::shared_ptr<Term> b) {
  return atomic_("<", {std::move(a), std::move(b)});
}
inline std::shared_ptr<Formula> lte(std::shared_ptr<Term> a, std::shared_ptr<Term> b) {
  return atomic_("\xe2\x89\xa4", {std::move(a), std::move(b)});  // ≤
}
inline std::shared_ptr<Formula> eq(std::shared_ptr<Term> a, std::shared_ptr<Term> b) {
  return atomic_("=", {std::move(a), std::move(b)});
}
inline std::shared_ptr<Formula> ne(std::shared_ptr<Term> a, std::shared_ptr<Term> b) {
  return atomic_("\xe2\x89\xa0", {std::move(a), std::move(b)});  // ≠
}

// ---------------------------------------------------------------------------
// Connectives , unified shape with `operands` array.
// ---------------------------------------------------------------------------

inline std::shared_ptr<Formula> connective_(std::string kind,
                                              std::vector<std::shared_ptr<Formula>> operands) {
  return std::make_shared<Formula>(Formula{ConnectiveFormula{std::move(kind), std::move(operands)}});
}
inline std::shared_ptr<Formula> not_(std::shared_ptr<Formula> a) {
  return connective_("not", {std::move(a)});
}
inline std::shared_ptr<Formula> implies(std::shared_ptr<Formula> antecedent,
                                         std::shared_ptr<Formula> consequent) {
  return connective_("implies", {std::move(antecedent), std::move(consequent)});
}
inline std::shared_ptr<Formula> and_(std::vector<std::shared_ptr<Formula>> operands) {
  return connective_("and", std::move(operands));
}
inline std::shared_ptr<Formula> or_(std::vector<std::shared_ptr<Formula>> operands) {
  return connective_("or", std::move(operands));
}

// ---------------------------------------------------------------------------
// Quantifiers , flat shape, no Lambda wrapper.
// ---------------------------------------------------------------------------

template <typename Body>
std::shared_ptr<Formula> forall(Sort sort, Body body) {
  std::string vname = fresh_var_name();
  auto var = make_var(vname);
  auto inner = body(var);
  return std::make_shared<Formula>(Formula{
      QuantifierFormula{"forall", std::move(vname), std::move(sort), std::move(inner)}});
}

template <typename Body>
std::shared_ptr<Formula> exists(Sort sort, Body body) {
  std::string vname = fresh_var_name();
  auto var = make_var(vname);
  auto inner = body(var);
  return std::make_shared<Formula>(Formula{
      QuantifierFormula{"exists", std::move(vname), std::move(sort), std::move(inner)}});
}

// ---------------------------------------------------------------------------
// Lambda terms (first-class functions)
// ---------------------------------------------------------------------------

inline std::shared_ptr<Term> lambda(std::string paramName, Sort paramSort, std::shared_ptr<Term> body) {
  return std::make_shared<Term>(Term{LambdaTerm{std::move(paramName), std::move(paramSort), std::move(body)}});
}

// ---------------------------------------------------------------------------
// Let terms (local bindings)
// ---------------------------------------------------------------------------

inline std::shared_ptr<Term> let_(std::vector<LetBinding> bindings, std::shared_ptr<Term> body) {
  return std::make_shared<Term>(Term{LetTerm{std::move(bindings), std::move(body)}});
}

// ---------------------------------------------------------------------------
// Choice formula (definite description)
// ---------------------------------------------------------------------------

template <typename Body>
std::shared_ptr<Formula> choice(std::string varName, Sort sort, Body body) {
  auto var = make_var(varName);
  auto inner = body(var);
  return std::make_shared<Formula>(Formula{
      ChoiceFormula{std::move(varName), std::move(sort), std::move(inner)}});
}

// ---------------------------------------------------------------------------
// EvidenceTerm , proof-certificate carrier per v1.3.0 catalog.
// ---------------------------------------------------------------------------
//
// Mirrors implementations/rust/provekit-ir-symbolic/src/lib.rs `EvidenceTerm`
// + `EvidenceCertificate`. Locked IR-JSON shape per the spec is:
//
//   {"kind":"evidence",
//    "proofType":"smt-lib"|"coq"|"custom",
//    "certificate":{"tool":"...","version":"...",
//                   "formulaHash":"...","proofData":"..."}}
//
// All fields are required strings. The kit emits insertion order; the
// canonicalizer's JCS pass re-sorts before hashing.
//
// `proofType` values are open-set strings; the protocol catalog lists
// "smt-lib", "coq", and "custom" as the three known values for v1.3.0.
//
// Cross-impl JCS conformance: the byte-identical fixture lives at
// implementations/cpp/provekit-ir-symbolic/example/evidence_term_test.cpp;
// any change to write_evidence below MUST keep that fixture passing.

struct EvidenceCertificate {
  std::string tool;
  std::string version;
  std::string formula_hash;
  std::string proof_data;
};

struct EvidenceTerm {
  std::string proof_type;
  EvidenceCertificate certificate;
};

inline std::shared_ptr<EvidenceTerm> evidence(std::string proof_type,
                                               EvidenceCertificate cert) {
  return std::make_shared<EvidenceTerm>(
      EvidenceTerm{std::move(proof_type), std::move(cert)});
}

// ---------------------------------------------------------------------------
// Contract collector
// ---------------------------------------------------------------------------

struct ContractDecl {
  std::string name;
  std::shared_ptr<Formula> pre;   // nullable
  std::shared_ptr<Formula> post;  // nullable
  std::shared_ptr<Formula> inv;   // nullable
  std::string outBinding;         // conventionally "out"
  // Optional v1.3.0 evidence certificate carrier. nullptr == absent
  // (matches the "omit absent" JCS rule and Rust's `Option<EvidenceTerm>`).
  std::shared_ptr<EvidenceTerm> evidence;
};

inline std::vector<ContractDecl>& collector() {
  static std::vector<ContractDecl> v;
  return v;
}

inline void begin_collecting() {
  collector().clear();
}

// contract() is the full authoring primitive; pre/post/inv each optional but
// at least one must be non-null (kit-side check below fails fast).
//
// The `evidence` parameter is the v1.3.0 EvidenceTerm slot; nullptr (default)
// means absent and is omitted from marshal_declarations output.
inline void contract(std::string name,
                     std::shared_ptr<Formula> pre = nullptr,
                     std::shared_ptr<Formula> post = nullptr,
                     std::shared_ptr<Formula> inv = nullptr,
                     std::string outBinding = "out",
                     std::shared_ptr<EvidenceTerm> evidence = nullptr) {
  if (!pre && !post && !inv) {
    std::fprintf(stderr,
                 "ERROR: contract(\"%s\"): at least one of pre/post/inv must be non-null\n",
                 name.c_str());
    std::abort();
  }
  collector().push_back(ContractDecl{
      std::move(name), std::move(pre), std::move(post), std::move(inv),
      std::move(outBinding), std::move(evidence)});
}

// must() is the precondition-only convenience alias.
inline void must(std::string name, std::shared_ptr<Formula> precondition) {
  contract(std::move(name), std::move(precondition));
}

inline std::vector<ContractDecl> finish() {
  std::vector<ContractDecl> out;
  out.swap(collector());
  return out;
}

// ---------------------------------------------------------------------------
// Bridge declaration collector (kit-side; processed by mint adapter).
// ---------------------------------------------------------------------------

struct BridgeDecl {
  std::string source_symbol;
  std::string source_layer;
  std::string target_contract_name;
  std::string target_layer;
  std::vector<std::string> ir_arg_sorts;
  std::string ir_return_sort;
  std::string notes;
};

inline std::vector<BridgeDecl>& bridge_collector() {
  static std::vector<BridgeDecl> v;
  return v;
}

inline void bridge_decl(BridgeDecl d) {
  bridge_collector().push_back(std::move(d));
}

inline std::vector<BridgeDecl> finish_bridges() {
  std::vector<BridgeDecl> out;
  out.swap(bridge_collector());
  return out;
}

inline void reset_bridge_collector() {
  bridge_collector().clear();
}

// ---------------------------------------------------------------------------
// JSON serialization , emits the protocol-locked uniform IR shape.
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
  // Locked key order: kind, name. (No sort.)
  out << "{\"kind\":\"var\",\"name\":";
  write_string(out, v.name);
  out << "}";
}

inline void write_const(std::ostringstream& out, const ConstTerm& c) {
  // JCS alphabetical: kind, sort, value.
  out << "{\"kind\":\"const\",\"sort\":";
  write_sort(out, c.sort);
  out << ",\"value\":";
  std::visit([&out](const auto& val) {
    using T = std::decay_t<decltype(val)>;
    if constexpr (std::is_same_v<T, int64_t>) out << val;
    else if constexpr (std::is_same_v<T, double>) out << val;
    else if constexpr (std::is_same_v<T, bool>) out << (val ? "true" : "false");
    else if constexpr (std::is_same_v<T, std::string>) {
      write_string(out, val);
    }
  }, c.value);
  out << "}";
}

inline void write_ctor(std::ostringstream& out, const CtorTerm& c) {
  // JCS alphabetical: args, kind, name. (No sort in protocol v1.1.)
  out << "{\"args\":[";
  for (size_t i = 0; i < c.args.size(); i++) {
    if (i > 0) out << ",";
    write_term(out, *c.args[i]);
  }
  out << "],\"kind\":\"ctor\",\"name\":";
  write_string(out, c.name);
  out << "}";
}

inline void write_lambda(std::ostringstream& out, const LambdaTerm& l) {
  // JCS alphabetical: body, kind, paramName, paramSort.
  out << "{\"body\":";
  write_term(out, *l.body);
  out << ",\"kind\":\"lambda\",\"paramName\":";
  write_string(out, l.paramName);
  out << ",\"paramSort\":";
  write_sort(out, l.paramSort);
  out << "}";
}

inline void write_let(std::ostringstream& out, const LetTerm& l) {
  // JCS alphabetical: bindings, body, kind.
  out << "{\"bindings\":[";
  for (size_t i = 0; i < l.bindings.size(); i++) {
    if (i > 0) out << ",";
    out << "{\"boundTerm\":";
    write_term(out, *l.bindings[i].boundTerm);
    out << ",\"name\":";
    write_string(out, l.bindings[i].name);
    out << "}";
  }
  out << "],\"body\":";
  write_term(out, *l.body);
  out << ",\"kind\":\"let\"}";
}

inline void write_term(std::ostringstream& out, const Term& t) {
  std::visit([&out](const auto& v) {
    using T = std::decay_t<decltype(v)>;
    if constexpr (std::is_same_v<T, VarTerm>) write_var(out, v);
    else if constexpr (std::is_same_v<T, ConstTerm>) write_const(out, v);
    else if constexpr (std::is_same_v<T, CtorTerm>) write_ctor(out, v);
    else if constexpr (std::is_same_v<T, LambdaTerm>) write_lambda(out, v);
    else if constexpr (std::is_same_v<T, LetTerm>) write_let(out, v);
  }, t.v);
}

inline void write_formula(std::ostringstream& out, const Formula& f);

inline void write_atomic(std::ostringstream& out, const AtomicFormula& a) {
  // JCS alphabetical: args, kind, name.
  out << "{\"args\":[";
  for (size_t i = 0; i < a.args.size(); i++) {
    if (i > 0) out << ",";
    write_term(out, *a.args[i]);
  }
  out << "],\"kind\":\"atomic\",\"name\":";
  write_string(out, a.name);
  out << "}";
}

inline void write_connective(std::ostringstream& out, const ConnectiveFormula& c) {
  // Locked key order: kind, operands.
  out << "{\"kind\":";
  write_string(out, c.kind);
  out << ",\"operands\":[";
  for (size_t i = 0; i < c.operands.size(); i++) {
    if (i > 0) out << ",";
    write_formula(out, *c.operands[i]);
  }
  out << "]}";
}

inline void write_quantifier(std::ostringstream& out, const QuantifierFormula& q) {
  // JCS alphabetical: body, kind, name, sort.
  out << "{\"body\":";
  write_formula(out, *q.body);
  out << ",\"kind\":";
  write_string(out, q.kind);
  out << ",\"name\":";
  write_string(out, q.name);
  out << ",\"sort\":";
  write_sort(out, q.sort);
  out << "}";
}

inline void write_choice(std::ostringstream& out, const ChoiceFormula& c) {
  // JCS alphabetical: body, kind, sort, varName.
  out << "{\"body\":";
  write_formula(out, *c.body);
  out << ",\"kind\":\"choice\",\"sort\":";
  write_sort(out, c.sort);
  out << ",\"varName\":";
  write_string(out, c.varName);
  out << "}";
}

inline void write_formula(std::ostringstream& out, const Formula& f) {
  std::visit([&out](const auto& v) {
    using T = std::decay_t<decltype(v)>;
    if constexpr (std::is_same_v<T, AtomicFormula>) write_atomic(out, v);
    else if constexpr (std::is_same_v<T, ConnectiveFormula>) write_connective(out, v);
    else if constexpr (std::is_same_v<T, QuantifierFormula>) write_quantifier(out, v);
    else if constexpr (std::is_same_v<T, ChoiceFormula>) write_choice(out, v);
  }, f.v);
}

// Emit an EvidenceTerm in locked IR-JSON shape per the v1.3.0 catalog.
// Byte-pinned with the Rust peer's write_evidence in
// implementations/rust/provekit-ir-symbolic/src/serialize.rs:
//   {"kind":"evidence","proofType":"<x>","certificate":{"tool":"<x>",
//    "version":"<x>","formulaHash":"<x>","proofData":"<x>"}}
inline void write_evidence(std::ostringstream& out, const EvidenceTerm& e) {
  out << "{\"kind\":\"evidence\",\"proofType\":";
  write_string(out, e.proof_type);
  out << ",\"certificate\":{\"tool\":";
  write_string(out, e.certificate.tool);
  out << ",\"version\":";
  write_string(out, e.certificate.version);
  out << ",\"formulaHash\":";
  write_string(out, e.certificate.formula_hash);
  out << ",\"proofData\":";
  write_string(out, e.certificate.proof_data);
  out << "}}";
}

// Marshal an array of ContractDecl into the IR-JSON Document shape.
// JCS alphabetical key order: evidence?, inv?, kind, name, outBinding, post?, pre?
// pre/post/inv/evidence omitted when null (JCS "omit absent" rule).
inline std::string marshal_declarations(const std::vector<ContractDecl>& decls) {
  std::ostringstream out;
  out << "[";
  for (size_t i = 0; i < decls.size(); i++) {
    if (i > 0) out << ",";
    out << "{";
    if (decls[i].evidence) {
      out << "\"evidence\":";
      write_evidence(out, *decls[i].evidence);
      out << ",";
    }
    if (decls[i].inv) {
      out << "\"inv\":";
      write_formula(out, *decls[i].inv);
      out << ",";
    }
    out << "\"kind\":\"contract\",\"name\":";
    write_string(out, decls[i].name);
    out << ",\"outBinding\":";
    write_string(out, decls[i].outBinding);
    if (decls[i].post) {
      out << ",\"post\":";
      write_formula(out, *decls[i].post);
    }
    if (decls[i].pre) {
      out << ",\"pre\":";
      write_formula(out, *decls[i].pre);
    }
    out << "}";
  }
  out << "]";
  return out.str();
}

}  // namespace provekit::ir
