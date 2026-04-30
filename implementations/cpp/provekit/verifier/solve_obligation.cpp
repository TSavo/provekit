// SPDX-License-Identifier: Apache-2.0

#include "solve_obligation.hpp"

#include <array>
#include <cstdio>
#include <cstdlib>
#include <sstream>
#include <unistd.h>
#include <sys/wait.h>

namespace provekit::verifier {

namespace {

// Write the script to a temp file, exec the solver synchronously,
// capture stdout. Avoids piping issues on macOS / Linux.
SolveResult run_z3_blocking(const std::string& z3_path, const std::string& script) {
    SolveResult r;

    // Temp file for script.
    char path_template[] = "/tmp/provekit_smt_XXXXXX.smt2";
    int fd = mkstemps(path_template, 5);
    if (fd < 0) {
        r.error = "mkstemps failed";
        return r;
    }
    {
        ssize_t total = 0;
        while (total < static_cast<ssize_t>(script.size())) {
            ssize_t n = write(fd, script.data() + total, script.size() - total);
            if (n < 0) {
                close(fd);
                unlink(path_template);
                r.error = "write to script tempfile failed";
                return r;
            }
            total += n;
        }
        close(fd);
    }

    // Run solver via popen.
    std::string cmd = z3_path + " -smt2 " + path_template + " 2>&1";
    FILE* p = popen(cmd.c_str(), "r");
    if (!p) {
        unlink(path_template);
        r.error = "popen failed for " + cmd;
        return r;
    }
    std::array<char, 4096> buf;
    std::ostringstream out;
    while (size_t n = fread(buf.data(), 1, buf.size(), p)) {
        out.write(buf.data(), n);
    }
    int status = pclose(p);
    unlink(path_template);
    r.solver_stdout = out.str();

    // First non-empty line is the verdict.
    std::string verdict_line;
    {
        std::istringstream is(r.solver_stdout);
        std::string line;
        while (std::getline(is, line)) {
            if (!line.empty() && line.back() == '\r') line.pop_back();
            if (line.empty()) continue;
            verdict_line = line;
            break;
        }
    }

    (void)status;
    if (verdict_line == "unsat") {
        r.verdict = ObligationVerdict::Discharged;
    } else if (verdict_line == "sat") {
        r.verdict = ObligationVerdict::Unsatisfied;
    } else {
        r.verdict = ObligationVerdict::Undecidable;
        r.error = "unrecognized solver verdict: " + verdict_line;
    }
    return r;
}

}  // namespace

SolveResult SolveObligationStage::Run(const std::string& smt_script) {
    return run_z3_blocking(z3_path_, smt_script);
}

std::future<SolveResult> SolveObligationStage::RunAsync(const std::string& smt_script) {
    std::string z3 = z3_path_;
    return std::async(std::launch::async, [z3, smt_script]() {
        return run_z3_blocking(z3, smt_script);
    });
}

}  // namespace provekit::verifier
