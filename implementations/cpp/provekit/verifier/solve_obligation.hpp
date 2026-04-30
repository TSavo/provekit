// SPDX-License-Identifier: Apache-2.0
//
// SolveObligationStage — Stage 5. Runs Z3 on the SMT-LIB script and
// classifies the verdict. Uses std::async + std::future to allow the
// orchestrator to fan callsites out in parallel while still letting
// the solver invocation itself remain a clean blocking subprocess.

#pragma once

#include "types.hpp"

#include <future>

namespace provekit::verifier {

struct SolveResult {
    ObligationVerdict verdict = ObligationVerdict::Undecidable;
    std::string solver_stdout;
    std::string solver_stderr;
    std::string error;
};

class SolveObligationStage {
   public:
    explicit SolveObligationStage(std::string z3_path = "z3") : z3_path_(std::move(z3_path)) {}

    // Synchronous (blocking) solve.
    SolveResult Run(const std::string& smt_script);

    // Asynchronous solve. The caller can fan many callsites' solves
    // out into futures and gather them in any order.
    std::future<SolveResult> RunAsync(const std::string& smt_script);

   private:
    std::string z3_path_;
};

}  // namespace provekit::verifier
