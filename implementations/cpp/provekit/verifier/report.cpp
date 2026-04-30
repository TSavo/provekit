// SPDX-License-Identifier: Apache-2.0

#include "report.hpp"

namespace provekit::verifier {

void ReportStage::Add(const CallSite& cs,
                       ObligationVerdict verdict,
                       const std::string& reason,
                       Report* r) {
    r->total_callsites++;
    ReportRow row;
    row.callsite = cs;
    row.status = verdict_to_string(verdict);
    row.reason = reason;
    if (verdict == ObligationVerdict::Discharged) {
        r->discharged++;
    } else {
        r->violations++;
    }
    r->rows.push_back(std::move(row));
}

void ReportStage::AddLoadErrors(const std::vector<LoadError>& errs, Report* r) {
    r->load_errors = errs;
}

}  // namespace provekit::verifier
