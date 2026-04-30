// SPDX-License-Identifier: Apache-2.0

#include "resolve_target.hpp"

namespace provekit::verifier {

ResolveResult ResolveTargetStage::Run(const CallSite& cs, const MementoPool& pool) {
    ResolveResult r;
    auto it = pool.mementos.find(cs.bridge_target_cid);
    if (it == pool.mementos.end()) {
        r.error = "bridge target CID " + cs.bridge_target_cid + " not in pool";
        return r;
    }
    const auto& env = it->second;
    if (!env.contains("evidence") || !env["evidence"].is_object()) {
        r.error = "target memento has no evidence object";
        return r;
    }
    const auto& ev = env["evidence"];
    if (ev.value("kind", "") != "property") {
        r.error = "target memento is not a property memento";
        return r;
    }
    const auto& body = ev["body"];
    r.resolved.cid = cs.bridge_target_cid;
    if (body.contains("irFormula")) r.resolved.ir_formula = body["irFormula"];
    if (body.contains("scope")) r.resolved.scope = body["scope"];
    r.resolved.ir_kit_version = body.value("irKitVersion", "");
    r.ok = true;
    return r;
}

}  // namespace provekit::verifier
