// SPDX-License-Identifier: Apache-2.0
//
// Verifier types — POD-style structs the 6-stage bridge enforcement
// pipeline passes between stages. Mirrors the Go verifier's types.go.

#pragma once

#include <map>
#include <string>
#include <vector>

#include <nlohmann/json.hpp>

namespace provekit::verifier {

using Json = nlohmann::json;

// LoadError captures per-file failures during load-all-proofs.
struct LoadError {
    std::string proof_path;
    std::string reason;
};

// MementoPool is the unified store every downstream stage hash-looks
// up against. Keyed by envelope CID.
struct MementoPool {
    // CID → parsed envelope JSON (the canonical-bytes JSON object).
    std::map<std::string, Json> mementos;
    // sourceSymbol (IR name) → bridge envelope JSON. Index for
    // callsite enumeration.
    std::map<std::string, Json> bridges_by_symbol;
    std::vector<LoadError> load_errors;
};

// CallSite — a (bridge, property memento, arg term) triple.
struct CallSite {
    std::string bridge_ir_name;
    std::string bridge_target_cid;
    std::string bridge_source_layer;
    std::string bridge_target_layer;
    std::string property_name;
    std::string property_cid;
    Json arg_term;  // an IrTerm object; null if missing
};

// ResolvedProperty — what resolve-bridge-target returns.
struct ResolvedProperty {
    std::string cid;
    Json ir_formula;
    Json scope;
    std::string ir_kit_version;
};

// Verdict from solve-obligation.
enum class ObligationVerdict {
    Discharged,
    Unsatisfied,
    Undecidable,
    Disagreement,
};

inline std::string verdict_to_string(ObligationVerdict v) {
    switch (v) {
        case ObligationVerdict::Discharged: return "discharged";
        case ObligationVerdict::Unsatisfied: return "unsatisfied";
        case ObligationVerdict::Undecidable: return "undecidable";
        case ObligationVerdict::Disagreement: return "disagreement";
    }
    return "unknown";
}

struct ReportRow {
    CallSite callsite;
    std::string status;
    std::string reason;
};

struct Report {
    int total_callsites = 0;
    int discharged = 0;
    int violations = 0;
    std::vector<ReportRow> rows;
    std::vector<LoadError> load_errors;
};

}  // namespace provekit::verifier
