// SPDX-License-Identifier: Apache-2.0
//
// Verifier types — POD-style structs the 6-stage bridge enforcement
// pipeline passes between stages. Mirrors the Go verifier's types.go.
//
// Architecture: the memento IS the verification. The .proof protocol IS the cache.
// The hash IS the boundary.

#pragma once

#include <map>
#include <string>
#include <vector>
#include <queue>
#include <set>
#include <optional>
#include <utility>

#include <nlohmann/json.hpp>

namespace provekit::verifier {

using Json = nlohmann::json;

// LoadError captures per-file failures during load-all-proofs.
struct LoadError {
    std::string proof_path;
    std::string reason;
};

// MementoPool is the unified store every downstream stage hash-looks
// up against. The memento IS the verification. The .proof protocol IS the cache.
// The hash IS the boundary.
struct MementoPool {
    // CID → parsed envelope JSON (the canonical-bytes JSON object).
    std::map<std::string, Json> mementos;
    // Formula CID → memento CID. Index for fast formula lookup.
    std::map<std::string, std::string> formula_to_memento;
    // sourceSymbol (IR name) → bridge envelope JSON. Index for
    // callsite enumeration.
    std::map<std::string, Json> bridges_by_symbol;
    std::vector<LoadError> load_errors;

    /// Look up a formula by its content hash. The memento IS the verification.
    std::optional<Json> verify_by_hash(const std::string& formula_cid) const {
        auto it = formula_to_memento.find(formula_cid);
        if (it == formula_to_memento.end()) return std::nullopt;
        auto jt = mementos.find(it->second);
        if (jt == mementos.end()) return std::nullopt;
        return jt->second;
    }

    /// Insert a memento and index it by formula hash.
    void insert(const std::string& memento_cid, const Json& envelope) {
        if (envelope.contains("evidence") && envelope["evidence"].contains("body")) {
            const auto& body = envelope["evidence"]["body"];
            for (const auto& field : {"preHash", "postHash", "invHash", "antecedentHash", "consequentHash"}) {
                if (body.contains(field) && body[field].is_string()) {
                    formula_to_memento[body[field].get<std::string>()] = memento_cid;
                }
            }
        }
        mementos[memento_cid] = envelope;
    }

    /// Check if antecedent → consequent is already proven.
    std::optional<Json> verify_implication(const std::string& antecedent_cid, const std::string& consequent_cid) const {
        for (const auto& [_, envelope] : mementos) {
            if (!envelope.contains("evidence")) continue;
            const auto& evidence = envelope["evidence"];
            if (!evidence.contains("kind") || evidence["kind"] != "implication") continue;
            if (!evidence.contains("body")) continue;
            const auto& body = evidence["body"];
            if (body.contains("antecedentHash") && body.contains("consequentHash") &&
                body["antecedentHash"].is_string() && body["consequentHash"].is_string() &&
                body["antecedentHash"].get<std::string>() == antecedent_cid &&
                body["consequentHash"].get<std::string>() == consequent_cid) {
                return envelope;
            }
        }
        return std::nullopt;
    }

    enum class ImplicationResult {
        Unknown,
        ProvenDirect,
        ProvenTransitive,
        ProvenReflexive,
    };

    /// Check if antecedent → consequent holds via direct, transitive, or reflexive.
    std::pair<ImplicationResult, std::vector<std::string>> can_imply(
        const std::string& antecedent_cid,
        const std::string& consequent_cid) const {
        
        if (antecedent_cid == consequent_cid) {
            return {ImplicationResult::ProvenReflexive, {antecedent_cid}};
        }

        if (verify_implication(antecedent_cid, consequent_cid).has_value()) {
            return {ImplicationResult::ProvenDirect, {antecedent_cid, consequent_cid}};
        }

        // Build graph and BFS
        std::map<std::string, std::vector<std::string>> graph;
        for (const auto& [_, envelope] : mementos) {
            if (!envelope.contains("evidence")) continue;
            const auto& evidence = envelope["evidence"];
            if (!evidence.contains("kind") || evidence["kind"] != "implication") continue;
            if (!evidence.contains("body")) continue;
            const auto& body = evidence["body"];
            if (body.contains("antecedentHash") && body.contains("consequentHash") &&
                body["antecedentHash"].is_string() && body["consequentHash"].is_string()) {
                graph[body["antecedentHash"].get<std::string>()].push_back(
                    body["consequentHash"].get<std::string>());
            }
        }

        std::set<std::string> visited;
        std::queue<std::vector<std::string>> queue;
        queue.push({antecedent_cid});

        while (!queue.empty()) {
            auto path = queue.front();
            queue.pop();
            auto current = path.back();

            if (!visited.insert(current).second) continue;

            auto it = graph.find(current);
            if (it != graph.end()) {
                for (const auto& neighbor : it->second) {
                    auto new_path = path;
                    new_path.push_back(neighbor);
                    if (neighbor == consequent_cid) {
                        return {ImplicationResult::ProvenTransitive, new_path};
                    }
                    queue.push(new_path);
                }
            }
        }

        return {ImplicationResult::Unknown, {}};
    }
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
