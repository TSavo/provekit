// SPDX-License-Identifier: Apache-2.0

#include "runner.hpp"

#include "load_all_proofs.hpp"
#include "enumerate_callsites.hpp"
#include "resolve_target.hpp"
#include "instantiate.hpp"
#include "smt_emitter.hpp"
#include "solve_obligation.hpp"
#include "report.hpp"

#include <future>
#include <vector>

namespace provekit::verifier {

namespace {

struct PerCallSite {
    CallSite cs;
    ObligationVerdict verdict = ObligationVerdict::Undecidable;
    std::string reason;
};

PerCallSite work_one(const CallSite& cs,
                      const MementoPool& pool,
                      const std::string& z3_path) {
    PerCallSite r;
    r.cs = cs;

    ResolveTargetStage rs;
    auto rr = rs.Run(cs, pool);
    if (!rr.ok) {
        r.verdict = ObligationVerdict::Undecidable;
        r.reason = "resolve-target: " + rr.error;
        return r;
    }

    InstantiateStage is;
    Obligation ob;
    std::string err;
    if (!is.Run(rr.resolved, cs.arg_term, &ob, &err)) {
        r.verdict = ObligationVerdict::Undecidable;
        r.reason = "instantiate: " + err;
        return r;
    }

    SmtEmitter se;
    err.clear();
    std::string smt = se.Emit(ob.ir_formula, &err);
    if (!err.empty()) {
        r.verdict = ObligationVerdict::Undecidable;
        r.reason = "smt-emit: " + err;
        return r;
    }

    SolveObligationStage solver(z3_path);
    auto sr = solver.Run(smt);
    r.verdict = sr.verdict;
    if (!sr.error.empty()) {
        r.reason = sr.error;
    } else if (sr.verdict == ObligationVerdict::Unsatisfied) {
        r.reason = "solver returned sat (counterexample found): obligation falsifiable";
    } else if (sr.verdict == ObligationVerdict::Discharged) {
        r.reason = "solver returned unsat: obligation holds";
    }
    return r;
}

}  // namespace

Report Runner::Run() {
    Report report;

    LoadAllProofsStage stage1;
    MementoPool pool = stage1.Run(cfg_.project_root);

    EnumerateCallsitesStage stage2;
    std::vector<CallSite> callsites = stage2.Run(pool);

    // Fan stages 3-5 out per callsite.
    std::vector<std::future<PerCallSite>> futures;
    futures.reserve(callsites.size());
    const std::string z3 = cfg_.z3_path;
    for (const auto& cs : callsites) {
        futures.push_back(std::async(std::launch::async, [&pool, cs, z3]() {
            return work_one(cs, pool, z3);
        }));
    }

    ReportStage stage6;
    for (auto& f : futures) {
        PerCallSite r = f.get();
        stage6.Add(r.cs, r.verdict, r.reason, &report);
    }
    stage6.AddLoadErrors(pool.load_errors, &report);

    return report;
}

}  // namespace provekit::verifier
