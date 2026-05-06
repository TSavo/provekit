// SPDX-License-Identifier: Apache-2.0
//
// Pipeline types. Mirrors implementations/cpp/.../verifier/types.hpp.
//
// Shape compatibility: a memento envelope is either the v1.1 flat
// shape (top-level `evidence`/`bindingHash`/`producerSignature`/...) or
// the v1.2 layered shape (top-level `envelope`/`header`/`metadata`).
// The accessors `memento_kind` and `memento_body` paper over the cut so
// the rest of the verifier doesn't have to branch.

use std::collections::BTreeMap;

use provekit_walk::contract::{OpacityMementoLookup, PinInvariantMementoView};
use serde_json::Value as Json;
use serde::Serialize;

/// Return the kind discriminator of a memento, regardless of shape:
///
/// * v1.2 layered: `header.kind`
/// * v1.1 flat:    `evidence.kind`
pub fn memento_kind(envelope: &Json) -> Option<&str> {
    if envelope.get("envelope").is_some() {
        envelope.pointer("/header/kind").and_then(|v| v.as_str())
    } else {
        envelope.pointer("/evidence/kind").and_then(|v| v.as_str())
    }
}

/// Return the substrate-relevant inner object of a memento (the
/// container of kind-specific fields), regardless of shape:
///
/// * v1.2 layered: `header`
/// * v1.1 flat:    `evidence.body`
///
/// Note: for v1.1, formula references like `preHash`/`antecedentHash`
/// live under `evidence.body`; under v1.2 they live in `metadata`. Use
/// `memento_body_field` for those lookups.
pub fn memento_body(envelope: &Json) -> Option<&Json> {
    if envelope.get("envelope").is_some() {
        envelope.get("header")
    } else {
        envelope.pointer("/evidence/body")
    }
}

/// Look up a body-tier field that the verifier needs (formula-hash
/// references and similar) regardless of shape:
///
/// * v1.2 layered: prefer `header.<field>` (substrate-load-bearing
///   bridge/contract/implication kind-specific fields), then fall back
///   to `metadata.<field>` (per-formula derived hashes like preHash).
/// * v1.1 flat:    `evidence.body.<field>` (legacy flat).
///
/// This single helper covers both the substrate references the
/// verifier indexes by (antecedentHash / consequentHash on
/// implications, sourceSymbol on bridges) and the convenience hashes
/// (preHash / postHash / invHash) that ride in metadata under v1.2.
pub fn memento_body_field<'a>(envelope: &'a Json, field: &str) -> Option<&'a Json> {
    if envelope.get("envelope").is_some() {
        envelope
            .pointer("/header")
            .and_then(|h| h.get(field))
            .or_else(|| envelope.pointer("/metadata").and_then(|m| m.get(field)))
    } else {
        envelope
            .pointer("/evidence/body")
            .and_then(|b| b.get(field))
    }
}

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
    /// Contract CID -> contract name (indexed during load)
    pub cid_to_name: BTreeMap<String, String>,
    /// Contract name -> CID (reverse index)
    pub name_to_cid: BTreeMap<String, String>,

    // ---- Opacity discharge indexes (issue #384 B.5) ----
    //
    // These maps are indexed during `insert()` when a memento of the
    // corresponding discharge kind is loaded. The substrate's
    // `compose_function_contracts_checked` queries these via the
    // `OpacityMementoLookup` impl below.

    /// loopCid (from header.loopCid of a LoopInvariantMemento) ->
    /// memento CID. Populated when a "loop-invariant" kind memento is
    /// inserted. Spec: protocol/specs/2026-05-05-loop-invariant-memento.md
    pub loop_cid_to_memento: BTreeMap<String, String>,

    /// tryCid (from header.tryCid of a TryBranchMemento) -> memento CID.
    /// Populated when a "try-branch" kind memento is inserted.
    /// Spec: protocol/specs/2026-05-05-try-branch-memento.md
    pub try_cid_to_memento: BTreeMap<String, String>,

    /// bodyFnCid (from header.bodyFnCid of a ClosureBindingMemento) ->
    /// memento CID. Populated when a "closure-binding" kind memento is
    /// inserted. Spec: protocol/specs/2026-05-05-closure-binding-memento.md
    pub body_fn_cid_to_memento: BTreeMap<String, String>,

    /// Composite key "functionCid:target" -> memento CID. Populated
    /// when a "pin-invariant" kind memento is inserted. The composite
    /// key ensures the memento is anchored to both the function contract
    /// and the pinned parameter name.
    /// Spec: protocol/specs/2026-05-05-pin-invariant-memento.md
    pub pin_invariant_to_memento: BTreeMap<String, String>,
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

        // Scan for implication mementos that link these two.
        // Shape-agnostic: under v1.2 these references live in the
        // metadata; under v1.1 they live in evidence.body.
        for (_, envelope) in &self.mementos {
            if memento_kind(envelope) == Some("implication") {
                let ant = memento_body_field(envelope, "antecedentHash")
                    .and_then(|v| v.as_str());
                let con = memento_body_field(envelope, "consequentHash")
                    .and_then(|v| v.as_str());
                if ant == Some(antecedent_cid) && con == Some(consequent_cid) {
                    return Some(envelope);
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

        // Build implication graph adjacency list on-the-fly.
        // Shape-agnostic per the body/header accessors.
        let mut graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (_, envelope) in &self.mementos {
            if memento_kind(envelope) == Some("implication") {
                if let (Some(ant), Some(con)) = (
                    memento_body_field(envelope, "antecedentHash").and_then(|v| v.as_str()),
                    memento_body_field(envelope, "consequentHash").and_then(|v| v.as_str()),
                ) {
                    graph.entry(ant.to_string()).or_default().push(con.to_string());
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
        // Index by the formula hashes referenced in the body. Layout
        // depends on shape; `memento_body_field` papers over the cut.
        // Contract: preHash/postHash/invHash (in metadata under v1.2,
        //           in evidence.body under v1.1).
        // Implication: antecedentHash/consequentHash (in header under
        //           v1.2, in evidence.body under v1.1).
        for field in &[
            "preHash",
            "postHash",
            "invHash",
            "antecedentHash",
            "consequentHash",
        ] {
            if let Some(hash) = memento_body_field(&envelope, field).and_then(|v| v.as_str()) {
                self.formula_to_memento
                    .insert(hash.to_string(), memento_cid.clone());
            }
        }
        self.mementos.insert(memento_cid.clone(), envelope);

        // Index by contract name for cross-kit resolution.
        // Gate on memento kind: only contract-shaped mementos carry a
        // contractName/name that's a stable cross-kit identifier. Other
        // kinds (implication, etc.) sometimes have a header.name field
        // but it's not a contract identity, so indexing them would
        // mis-resolve call edges.
        let env_for_name = self.mementos.get(&memento_cid);
        let is_contract = env_for_name
            .and_then(memento_kind)
            == Some("contract");
        if is_contract {
            let name = env_for_name
                .and_then(|env| {
                    env.pointer("/header/contractName")
                        .or_else(|| env.pointer("/header/name"))
                        .or_else(|| env.pointer("/evidence/body/contractName"))
                        .or_else(|| env.pointer("/evidence/body/name"))
                })
                .and_then(|v| v.as_str());

            if let Some(n) = name {
                let n = n.to_string();
                // Detect collisions: same contract name, different CIDs.
                // Across merged projects this can happen when two
                // independent kits emit the same contract name. Keep the
                // first insertion (winner-keeps-it) and surface the
                // collision in load_errors so the verifier surfaces it.
                if let Some(existing) = self.name_to_cid.get(&n) {
                    if existing != &memento_cid {
                        self.load_errors.push(LoadError {
                            proof_path: memento_cid.clone(),
                            reason: format!(
                                "duplicate contract name `{n}` resolves to two CIDs: {existing} (kept) and {memento_cid} (dropped)"
                            ),
                        });
                    }
                } else {
                    self.cid_to_name.insert(memento_cid.clone(), n.clone());
                    self.name_to_cid.insert(n, memento_cid.clone());
                }
            }
        }

        // ---- Opacity discharge indexing (issue #384 B.5) ----
        // Index discharge mementos by their opacity-site CID fields so that
        // OpacityMementoLookup queries are O(log n) BTreeMap lookups rather
        // than a full pool scan.
        //
        // Both v1.1 flat (evidence.body.*) and v1.2 layered (header.*) shapes
        // are covered by memento_body_field / memento_kind.
        let kind = self.mementos.get(&memento_cid).and_then(|e| memento_kind(e)).map(str::to_string);
        match kind.as_deref() {
            Some("loop-invariant") => {
                // header.loopCid (v1.2) or evidence.body.loopCid (v1.1)
                if let Some(env) = self.mementos.get(&memento_cid) {
                    if let Some(loop_cid) = memento_body_field(env, "loopCid").and_then(|v| v.as_str()) {
                        self.loop_cid_to_memento.entry(loop_cid.to_string()).or_insert(memento_cid.clone());
                    }
                }
            }
            Some("try-branch") => {
                // header.tryCid (v1.2) or evidence.body.tryCid (v1.1)
                if let Some(env) = self.mementos.get(&memento_cid) {
                    if let Some(try_cid) = memento_body_field(env, "tryCid").and_then(|v| v.as_str()) {
                        self.try_cid_to_memento.entry(try_cid.to_string()).or_insert(memento_cid.clone());
                    }
                }
            }
            Some("closure-binding") => {
                // header.bodyFnCid (v1.2) or evidence.body.bodyFnCid (v1.1)
                if let Some(env) = self.mementos.get(&memento_cid) {
                    if let Some(body_fn_cid) = memento_body_field(env, "bodyFnCid").and_then(|v| v.as_str()) {
                        self.body_fn_cid_to_memento.entry(body_fn_cid.to_string()).or_insert(memento_cid.clone());
                    }
                }
            }
            Some("pin-invariant") => {
                // header.functionCid + header.pinnedTarget -> composite key
                if let Some(env) = self.mementos.get(&memento_cid) {
                    let function_cid = memento_body_field(env, "functionCid").and_then(|v| v.as_str());
                    let target = memento_body_field(env, "pinnedTarget").and_then(|v| v.as_str());
                    if let (Some(fc), Some(t)) = (function_cid, target) {
                        let key = format!("{}\x00{}", fc, t);
                        self.pin_invariant_to_memento.entry(key).or_insert(memento_cid.clone());
                    }
                }
            }
            _ => {}
        }
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

    /// Merge another pool into this one.
    ///
    /// Collision policy: for keys that already exist in `self`, the
    /// existing value wins (insert-only-if-absent). Cross-project merges
    /// must not silently overwrite earlier-loaded resolutions; surface
    /// collisions via `load_errors` so the verifier reports them.
    pub fn merge(&mut self, other: Self) {
        for (cid, env) in other.mementos {
            if !self.mementos.contains_key(&cid) {
                self.mementos.insert(cid, env);
            }
        }
        for (k, v) in other.formula_to_memento {
            if let Some(existing) = self.formula_to_memento.get(&k) {
                if existing != &v {
                    self.load_errors.push(LoadError {
                        proof_path: v.clone(),
                        reason: format!(
                            "merge collision for formula `{k}`: kept `{existing}`, dropped `{v}`"
                        ),
                    });
                }
            } else {
                self.formula_to_memento.insert(k, v);
            }
        }
        for (k, v) in other.bridges_by_symbol {
            self.bridges_by_symbol.entry(k).or_insert(v);
        }
        for (k, vs) in other.bundle_members {
            self.bundle_members.entry(k).or_default().extend(vs);
        }
        self.load_errors.extend(other.load_errors);
        for (k, v) in other.cid_to_name {
            self.cid_to_name.entry(k).or_insert(v);
        }
        for (k, v) in other.name_to_cid {
            if let Some(existing) = self.name_to_cid.get(&k) {
                if existing != &v {
                    self.load_errors.push(LoadError {
                        proof_path: v.clone(),
                        reason: format!(
                            "merge collision for contract name `{k}`: kept `{existing}`, dropped `{v}`"
                        ),
                    });
                }
            } else {
                self.name_to_cid.insert(k, v);
            }
        }
        // Opacity discharge indexes: first-insertion wins (same policy as
        // other single-valued indexes). Collisions on these keys mean two
        // proofs supply different discharge mementos for the same opacity
        // site — keep the first, let the substrate use whichever it loaded
        // first.
        for (k, v) in other.loop_cid_to_memento {
            self.loop_cid_to_memento.entry(k).or_insert(v);
        }
        for (k, v) in other.try_cid_to_memento {
            self.try_cid_to_memento.entry(k).or_insert(v);
        }
        for (k, v) in other.body_fn_cid_to_memento {
            self.body_fn_cid_to_memento.entry(k).or_insert(v);
        }
    }
}

/// `MementoPool` implements `OpacityMementoLookup` so that
/// `compose_function_contracts_checked` in `provekit-walk` can query
/// whether a discharge memento is present for a given opacity site CID.
///
/// Each lookup is an O(log n) BTreeMap probe against the three
/// opacity-discharge indexes populated by `MementoPool::insert()`.
impl OpacityMementoLookup for MementoPool {
    fn has_loop_invariant(&self, loop_cid: &str) -> bool {
        self.loop_cid_to_memento.contains_key(loop_cid)
    }
    fn has_try_branch(&self, try_cid: &str) -> bool {
        self.try_cid_to_memento.contains_key(try_cid)
    }
    fn has_closure_binding(&self, body_fn_cid: &str) -> bool {
        self.body_fn_cid_to_memento.contains_key(body_fn_cid)
    }
    fn has_drop_contract(&self, _type_name: &str) -> bool {
        // No drop-contract discharge memento kind is specified yet
        // (follow-up to issue #384). Return false so the substrate
        // refuses composition rather than silently assuming the drop
        // is effect-free. Wire this to a real index once the
        // drop-contract memento spec lands under protocol/specs/.
        false
    }
    fn lookup_pin_invariant(&self, function_cid: &str, target: &str) -> Option<PinInvariantMementoView> {
        let key = format!("{}\x00{}", function_cid, target);
        let memento_cid = self.pin_invariant_to_memento.get(&key)?;
        let memento = self.mementos.get(memento_cid)?;
        let header = memento.get("header")?;
        let metadata = memento.get("metadata")?;
        Some(PinInvariantMementoView {
            function_cid: function_cid.to_string(),
            pinned_target: target.to_string(),
            invariant: metadata.get("invariant")?.as_str()?.to_string(),
        })
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedCallEdge {
    pub source_contract_cid: String,
    pub target_contract_cid: String,
    pub file: String,
}

#[derive(Debug, Default, Clone)]
pub struct Report {
    pub total_callsites: usize,
    pub discharged: usize,
    pub violations: usize,
    pub rows: Vec<ReportRow>,
    pub load_errors: Vec<LoadError>,
    pub call_edges: Vec<ResolvedCallEdge>,
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
