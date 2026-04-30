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
        return;
    }
    if (kind == "and" || kind == "or" || kind == "not" || kind == "implies") {
        if (f.contains("operands") && f["operands"].is_array()) {
            for (const auto& op : f["operands"]) walk_formula(op, property_name, property_cid, pool, out);
        }
        return;
    }
    if (kind == "forall" || kind == "exists") {
        if (f.contains("body")) walk_formula(f["body"], property_name, property_cid, pool, out);
        return;
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
        if (ev.value("kind", "") != "contract") continue;
        if (!ev.contains("body") || !ev["body"].is_object()) continue;
        const auto& body = ev["body"];
        std::string property_name = body.value("contractName", "");
        if (property_name.empty()) {
            property_name = cid.substr(0, 12) + "...";
        }
        // Walk pre/post/inv (whichever are present). Each can independently
        // contain ctor invocations of bridge-source symbols (call sites).
        if (body.contains("pre") && body["pre"].is_object()) {
            walk_formula(body["pre"], property_name, cid, pool, out);
        }
        if (body.contains("post") && body["post"].is_object()) {
            walk_formula(body["post"], property_name, cid, pool, out);
        }
        if (body.contains("inv") && body["inv"].is_object()) {
            walk_formula(body["inv"], property_name, cid, pool, out);
        }
    }
    return out;
}

}  // namespace provekit::verifier
