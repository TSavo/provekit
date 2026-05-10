// SPDX-License-Identifier: Apache-2.0
//
// EnumerateCallsitesStage: Stage 2. Walks every property memento in
// the pool, finds Ctor terms whose name matches a bridge envelope's
// sourceSymbol, emits one CallSite per match.

#pragma once

#include "types.hpp"

namespace provekit::verifier {

class EnumerateCallsitesStage {
   public:
    std::vector<CallSite> Run(const MementoPool& pool);
};

}  // namespace provekit::verifier
