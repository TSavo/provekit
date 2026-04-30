// SPDX-License-Identifier: Apache-2.0
//
// Runner — orchestrates all 6 stages of the C++ bridge enforcement
// pipeline. Mirrors the Go verifier's `runner.go`. Per-callsite work
// (resolve → instantiate → emit → solve) is fanned out via std::async
// for parallelism.

#pragma once

#include "types.hpp"

namespace provekit::verifier {

struct RunnerConfig {
    std::string project_root;
    std::string z3_path = "z3";
};

class Runner {
   public:
    explicit Runner(RunnerConfig cfg) : cfg_(std::move(cfg)) {}

    Report Run();

   private:
    RunnerConfig cfg_;
};

}  // namespace provekit::verifier
