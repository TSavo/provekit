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
    if (ev.value("kind", "") != "contract") {
        r.error = "target memento is not a contract memento";
        return r;
    }
    const auto& body = ev["body"];
    r.resolved.cid = cs.bridge_target_cid;
    // Resolve the consumer-side discharge to the contract's `pre` formula
    // (the precondition the caller must establish at the call site).
    // Postconditions and invariants live alongside but participate in
    // the handshake algorithm via their own slots; this stage targets pre.
    if (body.contains("pre")) r.resolved.ir_formula = body["pre"];
    r.resolved.ir_kit_version = "";  // contract memento doesn't carry irKitVersion
    r.ok = true;
    return r;
}

}  // namespace provekit::verifier
