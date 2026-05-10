// SPDX-License-Identifier: Apache-2.0
//
// SmtEmitter: translates an obligation IR formula into an SMT-LIB
// v2.6 script for a Z3-compatible solver.
//
// Strategy: assert (not <formula>); a sat reply means the obligation
// is FALSIFIABLE (counterexample exists, status = unsatisfied);
// unsat means the obligation HOLDS (discharged).

#pragma once

#include "types.hpp"

namespace provekit::verifier {

class SmtEmitter {
   public:
    std::string Emit(const Json& ir_formula, std::string* err);

   private:
    std::string emit_term(const Json& term, std::string* err);
};

}  // namespace provekit::verifier
