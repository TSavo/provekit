// SPDX-License-Identifier: Apache-2.0
//
// InstantiateStage — Stage 4. Substitutes the outermost universally
// quantified variable with the bridge call's argument term, producing
// a closed obligation IR formula.

#pragma once

#include "types.hpp"

namespace provekit::verifier {

struct Obligation {
    Json ir_formula;       // closed (no free Var of the substituted name)
    std::string property_cid;
    std::string property_name;
    std::string ir_kit_version;
};

class InstantiateStage {
   public:
    // Returns nullopt if the property formula is not of the expected
    // shape (outermost ForAll, single bound var, body Implies(p, q)).
    bool Run(const ResolvedProperty& resolved,
             const Json& arg_term,
             Obligation* out,
             std::string* err);

   private:
    Json substitute(const Json& term, const std::string& var_name, const Json& replacement);
};

}  // namespace provekit::verifier
