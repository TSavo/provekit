// SPDX-License-Identifier: Apache-2.0
//
// ResolveTargetStage: Stage 3. Hash-looks up the bridge target CID
// in the pool and returns the resolved property memento contents.

#pragma once

#include "types.hpp"

namespace provekit::verifier {

struct ResolveResult {
    bool ok = false;
    ResolvedProperty resolved;
    std::string error;
};

class ResolveTargetStage {
   public:
    ResolveResult Run(const CallSite& cs, const MementoPool& pool);
};

}  // namespace provekit::verifier
