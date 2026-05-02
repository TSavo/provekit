// SPDX-License-Identifier: Apache-2.0
//
// Pipeline types. Mirrors implementations/cpp/.../verifier/types.hpp.

use std::collections::BTreeMap;

use serde_json::Value as Json;

#[derive(Debug, Clone)]
pub struct LoadError {
    pub proof_path: String,
    pub reason: String,
}

#[derive(Debug, Default, Clone)]
pub struct MementoPool {
    /// CID -> the canonical-bytes-decoded memento envelope (as JSON).
    /// The memento IS the verification. To verify something is to find
    /// its memento in this map.
    pub mementos: BTreeMap<String, Json>,
    /// Formula CID -> memento CID. Index for fast formula lookup.
    /// The hash IS the boundary: systems don't exchange formulas,
    /// they exchange hashes. This index lets us find the memento
    /// for a given formula hash without scanning all mementos.
    pub formula_to_memento: BTreeMap<String, String>,
    /// sourceSymbol (IR ctor name) -> bridge envelope JSON.
    pub bridges_by_symbol: BTreeMap<String, Json>,
    /// Bundle (.proof file) CID -> set of member CIDs the bundle contained.
    ///
    /// Required to enforce `BridgeDeclaration.ConsequentBundlePinned`
    /// (see `protocol/specs/2026-04-30-ir-formal-grammar.md`
    /// § "Bridge target pinning: the shim-poisoning vector"). A bridge's
    /// `targetProofCid` names the bundle that is allowed to discharge
    /// it; we must answer "is this contract member from THAT bundle?".
    /// Multi-valued because the same member CID can legitimately appear
    /// in two bundles (an honest one and a poisoned one); we never want
    /// last-writer-wins to silently swap them.
    pub bundle_members: BTreeMap<String, std::collections::BTreeSet<String>>,
    pub load_errors: Vec<LoadError>,
}

/// Key for implication lookups: (antecedent CID, consequent CID).
/// The implication memento itself has a CID derived from this pair.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ImplicationKey(pub String, pub String);

impl MementoPool {
    /// The fundamental verification operation: look up a formula by its
    /// content hash. The memento IS the verification; if found, the
    /// formula is verified. No solver is invoked.
    ///
    /// Returns the memento envelope that verifies this formula.
    pub fn verify_by_hash(&self, formula_cid: &str) -> Option<&Json> {
        self.formula_to_memento
            .get(formula_cid)
            .and_then(|memento_cid| self.mementos.get(memento_cid))
    }

    /// Compute the CID for a formula JSON node, then look it up.
    /// The canonicalization + hash IS the boundary between systems.
    pub fn verify(&self, formula: &Json) -> Option<&Json> {
        let cid = compute_formula_cid(formula);
        self.verify_by_hash(&cid)
    }

    /// Check if P → Q is already proven in the pool.
    /// Looks for an implication memento whose evidence body contains
    /// both antecedentHash = P and consequentHash = Q.
    ///
    /// This is the core of bridge enforcement: "does the publisher's
    /// post imply the consumer's pre?"
    pub fn verify_implication(&self, antecedent_cid: &str, consequent_cid: &str) -> Option<&Json> {
        // Direct lookup: find a memento that indexes both hashes
        // and is an implication evidence kind.
        let _ant_memento = self.verify_by_hash(antecedent_cid)?;
        let _con_memento = self.verify_by_hash(consequent_cid)?;

        // Scan for implication mementos that link these two
        for (_, envelope) in &self.mementos {
            if let Some(evidence) = envelope.get("evidence") {
                if evidence.get("kind").and_then(|v| v.as_str()) == Some("implication") {
                    if let Some(body) = evidence.get("body") {
                        let ant = body.get("antecedentHash").and_then(|v| v.as_str());
                        let con = body.get("consequentHash").and_then(|v| v.as_str());
                        if ant == Some(antecedent_cid) && con == Some(consequent_cid) {
                            return Some(envelope);
                        }
                    }
                }
            }
        }
        None
    }

    /// Check if P → Q via transitive chaining.
    /// If P → R and R → Q are both in the pool, then P → Q.
    /// Uses BFS on the implication graph.
    pub fn implies_transitive(&self, antecedent_cid: &str, consequent_cid: &str) -> Option<Vec<String>> {
        if antecedent_cid == consequent_cid {
            return Some(vec![antecedent_cid.to_string()]);
        }

        // Build implication graph adjacency list on-the-fly
        let mut graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (_, envelope) in &self.mementos {
            if let Some(evidence) = envelope.get("evidence") {
                if evidence.get("kind").and_then(|v| v.as_str()) == Some("implication") {
                    if let Some(body) = evidence.get("body") {
                        if let (Some(ant), Some(con)) = (
                            body.get("antecedentHash").and_then(|v| v.as_str()),
                            body.get("consequentHash").and_then(|v| v.as_str()),
                        ) {
                            graph.entry(ant.to_string()).or_default().push(con.to_string());
                        }
                    }
                }
            }
        }

        // BFS
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        let mut path_map = BTreeMap::new();

        queue.push_back(antecedent_cid.to_string());
        visited.insert(antecedent_cid.to_string());
        path_map.insert(antecedent_cid.to_string(), vec![antecedent_cid.to_string()]);

        while let Some(current) = queue.pop_front() {
            if let Some(neighbors) = graph.get(&current) {
                for neighbor in neighbors {
                    if neighbor == consequent_cid {
                        let mut path = path_map.get(&current).unwrap().clone();
                        path.push(neighbor.clone());
                        return Some(path);
                    }
                    if visited.insert(neighbor.clone()) {
                        let mut path = path_map.get(&current).unwrap().clone();
                        path.push(neighbor.clone());
                        path_map.insert(neighbor.clone(), path);
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }

        None
    }

    /// Full implication check: direct, transitive, or via sub-formula composition.
    /// Returns the proof path if P → Q holds.
    pub fn can_implies(&self, antecedent_cid: &str, consequent_cid: &str) -> ImplicationResult {
        // 1. Reflexivity: P → P always holds (check first)
        if antecedent_cid == consequent_cid {
            return ImplicationResult::ProvenReflexive;
        }

        // 2. Direct implication
        if let Some(memento) = self.verify_implication(antecedent_cid, consequent_cid) {
            return ImplicationResult::ProvenDirect {
                memento_cid: memento.get("cid").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
            };
        }

        // 3. Transitive implication
        if let Some(path) = self.implies_transitive(antecedent_cid, consequent_cid) {
            return ImplicationResult::ProvenTransitive { path };
        }

        ImplicationResult::Unknown
    }

    /// Insert a memento into the pool and index it by formula hash.
    /// The .proof protocol IS the cache: storing a memento IS caching
    /// the verification result.
    pub fn insert(&mut self, memento_cid: String, envelope: Json) {
        // Index by the formula hashes referenced in the evidence
        if let Some(evidence) = envelope.get("evidence") {
            if let Some(body) = evidence.get("body") {
                // Contract evidence: preHash/postHash/invHash
                for field in &["preHash", "postHash", "invHash"] {
                    if let Some(hash) = body.get(field).and_then(|v| v.as_str()) {
                        self.formula_to_memento.insert(hash.to_string(), memento_cid.clone());
                    }
                }
                // Implication evidence: antecedentHash/consequentHash
                for field in &["antecedentHash", "consequentHash"] {
                    if let Some(hash) = body.get(field).and_then(|v| v.as_str()) {
                        self.formula_to_memento.insert(hash.to_string(), memento_cid.clone());
                    }
                }
            }
        }
        self.mementos.insert(memento_cid, envelope);
    }

    /// Sub-formula composition: walk the formula DAG and return all
    /// sub-formula CIDs that have mementos in the pool. If P is verified
    /// and we need to prove P ∧ Q, this returns P's CID so the solver
    /// can focus on Q.
    pub fn find_verified_subformulas(&self, formula: &Json) -> Vec<(String, &Json)> {
        let mut verified = Vec::new();
        let mut stack = vec![formula.clone()];
        let mut visited = std::collections::HashSet::new();

        while let Some(node) = stack.pop() {
            let cid = compute_formula_cid(&node);
            if !visited.insert(cid.clone()) {
                continue;
            }

            if let Some(memento) = self.verify_by_hash(&cid) {
                verified.push((cid, memento));
            }

            // Push children for recursive checking
            if let Some(obj) = node.as_object() {
                match obj.get("kind").and_then(|v| v.as_str()) {
                    Some("and") | Some("or") | Some("not") | Some("implies") => {
                        if let Some(ops) = obj.get("operands").and_then(|v| v.as_array()) {
                            for op in ops {
                                stack.push(op.clone());
                            }
                        }
                    }
                    Some("forall") | Some("exists") | Some("choice") => {
                        if let Some(body) = obj.get("body") {
                            stack.push(body.clone());
                        }
                    }
                    _ => {}
                }
            }
        }

        verified
    }
}

/// Result of an implication check.
#[derive(Debug, Clone)]
pub enum ImplicationResult {
    /// Direct implication memento found in pool.
    ProvenDirect { memento_cid: String },
    /// Transitive chain of implications found.
    ProvenTransitive { path: Vec<String> },
    /// Trivial: P → P.
    ProvenReflexive,
    /// No known implication path.
    Unknown,
}

impl ImplicationResult {
    pub fn is_proven(&self) -> bool {
        !matches!(self, Self::Unknown)
    }
}

/// Compute the CID for a formula JSON node by canonicalizing and hashing.
/// The hash IS the boundary: this function is the gate between the
/// formula domain and the hash domain.
pub fn compute_formula_cid(formula: &Json) -> String {
    use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value};

    fn json_to_value(j: &Json) -> std::sync::Arc<Value> {
        match j {
            Json::Null => Value::null(),
            Json::Bool(b) => Value::boolean(*b),
            Json::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Value::integer(i)
                } else if let Some(u) = n.as_u64() {
                    Value::integer(u as i64)
                } else if let Some(f) = n.as_f64() {
                    Value::integer(f as i64)
                } else {
                    Value::integer(0)
                }
            }
            Json::String(s) => Value::string(s.clone()),
            Json::Array(items) => {
                let v: Vec<_> = items.iter().map(json_to_value).collect();
                Value::array(v)
            }
            Json::Object(map) => {
                let entries: Vec<(String, _)> =
                    map.iter().map(|(k, v)| (k.clone(), json_to_value(v))).collect();
                std::sync::Arc::new(Value::Object(entries))
            }
        }
    }

    let value_tree = json_to_value(formula);
    let canonical = encode_jcs(&value_tree);
    blake3_512_of(canonical.as_bytes())
}

#[derive(Debug, Default, Clone)]
pub struct CallSite {
    pub bridge_ir_name: String,
    pub bridge_target_cid: String,
    pub bridge_source_layer: String,
    pub bridge_target_layer: String,
    /// Forward pin: the specific `.proof` bundle CID this bridge commits
    /// to as its consequent. `None` for legacy bridges that pre-date the
    /// `targetProofCid` field (kept loadable for back-compat; resolve_target
    /// emits a soft warning since `ConsequentBundlePinned` cannot be
    /// enforced when the field is absent). For bridges authored against
    /// the current grammar this MUST be `Some`; see
    /// `protocol/specs/2026-04-30-ir-formal-grammar.md`
    /// § "Bridge target pinning: the shim-poisoning vector".
    pub bridge_target_proof_cid: Option<String>,
    pub property_name: String,
    pub property_cid: String,
    pub arg_term: Option<Json>,
}

#[derive(Debug, Default, Clone)]
pub struct ResolvedProperty {
    pub cid: String,
    pub ir_formula: Option<Json>,
    pub ir_kit_version: String,
}

#[derive(Debug, Clone)]
pub struct Obligation {
    pub property_cid: String,
    pub ir_kit_version: String,
    pub ir_formula: Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObligationVerdict {
    Discharged,
    Unsatisfied,
    Undecidable,
    Disagreement,
}

impl ObligationVerdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Discharged => "discharged",
            Self::Unsatisfied => "unsatisfied",
            Self::Undecidable => "undecidable",
            Self::Disagreement => "disagreement",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReportRow {
    pub callsite: CallSite,
    pub status: String,
    pub reason: String,
}

#[derive(Debug, Default, Clone)]
pub struct Report {
    pub total_callsites: usize,
    pub discharged: usize,
    pub violations: usize,
    pub rows: Vec<ReportRow>,
    pub load_errors: Vec<LoadError>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_implication_memento(ant: &str, con: &str) -> Json {
        json!({
            "cid": format!("blake3-512:{}{}", ant, con),
            "evidence": {
                "kind": "implication",
                "body": {
                    "antecedentHash": ant,
                    "consequentHash": con,
                    "prover": "z3@4.12",
                    "proverRunMs": 42
                }
            }
        })
    }

    #[test]
    fn transitive_implication_chain_of_three() {
        let mut pool = MementoPool::default();

        // Insert P → Q and Q → R
        let p = "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let q = "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let r = "blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

        pool.insert("m1".to_string(), make_implication_memento(p, q));
        pool.insert("m2".to_string(), make_implication_memento(q, r));

        // Check P → R via transitivity
        let result = pool.can_implies(p, r);
        assert!(
            matches!(result, ImplicationResult::ProvenTransitive { .. }),
            "Expected transitive proof for P → R, got {:?}", result
        );
    }

    #[test]
    fn direct_implication_lookup() {
        let mut pool = MementoPool::default();

        let p = "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let q = "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

        pool.insert("m1".to_string(), make_implication_memento(p, q));

        let result = pool.can_implies(p, q);
        assert!(
            matches!(result, ImplicationResult::ProvenDirect { .. }),
            "Expected direct proof, got {:?}", result
        );
    }

    #[test]
    fn reflexive_implication_always_holds() {
        let pool = MementoPool::default();

        let p = "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

        let result = pool.can_implies(p, p);
        assert!(
            matches!(result, ImplicationResult::ProvenReflexive),
            "Expected reflexive proof, got {:?}", result
        );
    }

    #[test]
    fn unknown_imputation_returns_unknown() {
        let pool = MementoPool::default();

        let p = "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let q = "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

        let result = pool.can_implies(p, q);
        assert!(
            matches!(result, ImplicationResult::Unknown),
            "Expected unknown, got {:?}", result
        );
    }
}
