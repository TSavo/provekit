// SPDX-License-Identifier: Apache-2.0
//
// LoadAllProofsStage — Stage 1 of the bridge enforcement workflow.
// Walks every .proof file under projectRoot (project root +
// node_modules/{*,@*/*}/) and builds the unified MementoPool that
// every downstream stage hash-looks up against.

#pragma once

#include "types.hpp"

namespace provekit::verifier {

class LoadAllProofsStage {
   public:
    // Run loads every .proof in projectRoot and returns the unified pool.
    MementoPool Run(const std::string& project_root);

   private:
    void load_one(const std::string& proof_path, MementoPool& pool);
    std::vector<std::string> enumerate_proof_files(const std::string& project_root);
};

}  // namespace provekit::verifier
