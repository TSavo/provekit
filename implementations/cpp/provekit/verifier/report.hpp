// SPDX-License-Identifier: Apache-2.0
//
// ReportStage: Stage 6. Aggregates per-callsite verdicts into a Report.

#pragma once

#include "types.hpp"

namespace provekit::verifier {

class ReportStage {
   public:
    void Add(const CallSite& cs, ObligationVerdict verdict, const std::string& reason, Report* r);
    void AddLoadErrors(const std::vector<LoadError>& errs, Report* r);
};

}  // namespace provekit::verifier
