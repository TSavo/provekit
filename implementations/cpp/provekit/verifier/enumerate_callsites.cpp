// SPDX-License-Identifier: Apache-2.0

#include "enumerate_callsites.hpp"

namespace provekit::verifier {

namespace {

void walk_term(const Json& t,
               const std::string& property_name,
               const std::string& property_cid,
               const MementoPool& pool,
               std::vector<CallSite>& out);

void walk_formula(const Json& f,
                  const std::string& property_name,
                  const std::string& property_cid,
                  const MementoPool& pool,
                  std::vector<CallSite>& out) {
    if (!f.is_object()) return;
    const std::string kind = f.value("kind", "");
    if (kind == "atomic") {
        if (f.contains("args") && f["args"].is_array()) {
            for (const auto& a : f["args"]) walk_term(a, property_name, property_cid, pool, out);
        }
    } else if (kind == "and") {
        if (f.contains("conjuncts") && f["conjuncts"].is_array()) {
            for (const auto& c : f["conjuncts"]) walk_formula(c, property_name, property_cid, pool, out);
        }
    } else if (kind == "or") {
        if (f.contains("disjuncts") && f["disjuncts"].is_array()) {
            for (const auto& d : f["disjuncts"]) walk_formula(d, property_name, property_cid, pool, out);
        }
    } else if (kind == "not") {
        if (f.contains("body")) walk_formula(f["body"], property_name, property_cid, pool, out);
    } else if (kind == "implies") {
        if (f.contains("antecedent")) walk_formula(f["antecedent"], property_name, property_cid, pool, out);
        if (f.contains("consequent")) walk_formula(f["consequent"], property_name, property_cid, pool, out);
    } else if (kind == "forall" || kind == "exists") {
        if (f.contains("predicate") && f["predicate"].is_object()) {
            const auto& pred = f["predicate"];
            if (pred.contains("body")) walk_formula(pred["body"], property_name, property_cid, pool, out);
        }
    }
}

void walk_term(const Json& t,
               const std::string& property_name,
               const std::string& property_cid,
               const MementoPool& pool,
               std::vector<CallSite>& out) {
    if (!t.is_object()) return;
    if (t.value("kind", "") != "ctor") return;
    const std::string name = t.value("name", "");
    auto it = pool.bridges_by_symbol.find(name);
    if (it != pool.bridges_by_symbol.end()) {
        const auto& benv = it->second;
        const auto& bbody = benv["evidence"]["body"];
        CallSite cs;
        cs.bridge_ir_name = name;
        cs.bridge_target_cid = bbody.value("targetContractCid", "");
        cs.bridge_source_layer = bbody.value("sourceLayer", "");
        cs.bridge_target_layer = bbody.value("targetLayer", "");
        cs.property_name = property_name;
        cs.property_cid = property_cid;
        if (t.contains("args") && t["args"].is_array() && !t["args"].empty()) {
            cs.arg_term = t["args"][0];
        }
        out.push_back(std::move(cs));
    }
    if (t.contains("args") && t["args"].is_array()) {
        for (const auto& a : t["args"]) walk_term(a, property_name, property_cid, pool, out);
    }
}

}  // namespace

std::vector<CallSite> EnumerateCallsitesStage::Run(const MementoPool& pool) {
    std::vector<CallSite> out;
    for (const auto& [cid, env] : pool.mementos) {
        if (!env.contains("evidence") || !env["evidence"].is_object()) continue;
        const auto& ev = env["evidence"];
        if (ev.value("kind", "") != "property") continue;
        if (!ev.contains("body") || !ev["body"].is_object()) continue;
        const auto& body = ev["body"];
        std::string property_name;
        if (body.contains("scope") && body["scope"].is_object()) {
            property_name = body["scope"].value("name", "");
        }
        if (property_name.empty()) {
            property_name = cid.substr(0, 12) + "...";
        }
        if (!body.contains("irFormula") || !body["irFormula"].is_object()) continue;
        walk_formula(body["irFormula"], property_name, cid, pool, out);
    }
    return out;
}

}  // namespace provekit::verifier
