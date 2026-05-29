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

use libprovekit::compose::{OpacityMementoLookup, PinInvariantMementoView};
use serde::Serialize;
use serde_json::Value as Json;

/// Return the kind discriminator of a memento, regardless of shape:
///
/// * v1.2 layered: `header.kind`
/// * v1.1 flat:    `evidence.kind`
pub fn memento_kind(envelope: &Json) -> Option<&str> {
    if envelope.get("envelope").is_some() {
        envelope
            .pointer("/header/kind")
            .or_else(|| envelope.pointer("/envelope/header/kind"))
            .and_then(|v| v.as_str())
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
        envelope
            .get("header")
            .or_else(|| envelope.pointer("/envelope/header"))
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
            .or_else(|| {
                envelope
                    .pointer("/envelope/header")
                    .and_then(|h| h.get(field))
            })
            .or_else(|| {
                envelope
                    .pointer("/envelope/metadata")
                    .and_then(|m| m.get(field))
            })
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

    /// AliasingMemento discharge index: (formal_a, formal_b) ->
    /// memento CID. Populated when an "aliasing-memento" kind memento is
    /// inserted. The key is the sorted pair of formal parameter names.
    pub aliasing_pair_to_memento: BTreeMap<(String, String), String>,

    /// Composite key "functionCid\x00target" -> memento CID. Populated
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
        // The proof of `P → Q` is the implication memento that links them, not
        // the presence of P or Q in `formula_to_memento`. (P, the antecedent,
        // is no longer indexed there at all -- it is an assumption, not a
        // fact.) Scan for an implication memento with these exact endpoints.
        // Shape-agnostic: under v1.2 these references live in the
        // metadata; under v1.1 they live in evidence.body.
        for envelope in self.mementos.values() {
            if memento_kind(envelope) == Some("implication") {
                let ant = memento_body_field(envelope, "antecedentHash").and_then(|v| v.as_str());
                let con = memento_body_field(envelope, "consequentHash").and_then(|v| v.as_str());
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
    pub fn implies_transitive(
        &self,
        antecedent_cid: &str,
        consequent_cid: &str,
    ) -> Option<Vec<String>> {
        if antecedent_cid == consequent_cid {
            return Some(vec![antecedent_cid.to_string()]);
        }

        // Build implication graph adjacency list on-the-fly.
        // Shape-agnostic per the body/header accessors.
        let mut graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for envelope in self.mementos.values() {
            if memento_kind(envelope) == Some("implication") {
                if let (Some(ant), Some(con)) = (
                    memento_body_field(envelope, "antecedentHash").and_then(|v| v.as_str()),
                    memento_body_field(envelope, "consequentHash").and_then(|v| v.as_str()),
                ) {
                    graph
                        .entry(ant.to_string())
                        .or_default()
                        .push(con.to_string());
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
                memento_cid: memento
                    .get("cid")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
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
        // Index ONLY the formula hashes that name an ESTABLISHED FACT into
        // `formula_to_memento` -- the map Tier 0 (`verify`) trusts as "this
        // formula is proven true". A precondition (`preHash`) and an
        // implication antecedent (`antecedentHash`) are OBLIGATIONS /
        // ASSUMPTIONS, not facts: indexing them let a callsite's consumer
        // precondition self-discharge merely because the callee DECLARES it
        // (the missing-edge hole; see `precondition_is_obligation_not_verified_fact`).
        // Established facts: a function's `postHash`/`invHash` (guarantees on
        // return / always). NOT `consequentHash`: an implication's consequent
        // `Q` holds only GIVEN its antecedent `P`, so indexing it would make
        // Tier 0 `verify(Q)` treat a conditional as unconditional -- the same
        // category error as `preHash`/`antecedentHash`. Proven implications
        // discharge via `verify_implication`/`can_implies`, which scan
        // implication mementos directly and don't need the consequent here.
        for field in &["postHash", "invHash"] {
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
        let is_contract = env_for_name.and_then(memento_kind) == Some("contract");
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
        let kind = self
            .mementos
            .get(&memento_cid)
            .and_then(|e| memento_kind(e))
            .map(str::to_string);
        match kind.as_deref() {
            Some("loop-invariant") => {
                // header.loopCid (v1.2) or evidence.body.loopCid (v1.1)
                if let Some(env) = self.mementos.get(&memento_cid) {
                    if let Some(loop_cid) =
                        memento_body_field(env, "loopCid").and_then(|v| v.as_str())
                    {
                        self.loop_cid_to_memento
                            .entry(loop_cid.to_string())
                            .or_insert(memento_cid.clone());
                    }
                }
            }
            Some("try-branch") => {
                // header.tryCid (v1.2) or evidence.body.tryCid (v1.1)
                if let Some(env) = self.mementos.get(&memento_cid) {
                    if let Some(try_cid) =
                        memento_body_field(env, "tryCid").and_then(|v| v.as_str())
                    {
                        self.try_cid_to_memento
                            .entry(try_cid.to_string())
                            .or_insert(memento_cid.clone());
                    }
                }
            }
            Some("closure-binding") => {
                // header.bodyFnCid (v1.2) or evidence.body.bodyFnCid (v1.1)
                if let Some(env) = self.mementos.get(&memento_cid) {
                    if let Some(body_fn_cid) =
                        memento_body_field(env, "bodyFnCid").and_then(|v| v.as_str())
                    {
                        self.body_fn_cid_to_memento
                            .entry(body_fn_cid.to_string())
                            .or_insert(memento_cid.clone());
                    }
                }
            }
            Some("aliasing-memento") => {
                // header.formal_a and header.formal_b (v1.2) or evidence.body.formal_a/formal_b (v1.1)
                // Index by the sorted (formal_a, formal_b) pair
                if let Some(env) = self.mementos.get(&memento_cid) {
                    if let (Some(formal_a), Some(formal_b)) = (
                        memento_body_field(env, "formal_a").and_then(|v| v.as_str()),
                        memento_body_field(env, "formal_b").and_then(|v| v.as_str()),
                    ) {
                        let mut pair = (formal_a.to_string(), formal_b.to_string());
                        // Sort the pair for canonical ordering
                        if pair.0 > pair.1 {
                            pair = (pair.1, pair.0);
                        }
                        self.aliasing_pair_to_memento
                            .entry(pair)
                            .or_insert(memento_cid.clone());
                    }
                }
            }
            Some("pin-invariant") => {
                // header.functionCid + header.pinnedTarget -> composite key
                if let Some(env) = self.mementos.get(&memento_cid) {
                    let function_cid =
                        memento_body_field(env, "functionCid").and_then(|v| v.as_str());
                    let target = memento_body_field(env, "pinnedTarget").and_then(|v| v.as_str());
                    if let (Some(fc), Some(t)) = (function_cid, target) {
                        let key = format!("{}\x00{}", fc, t);
                        self.pin_invariant_to_memento
                            .entry(key)
                            .or_insert(memento_cid.clone());
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
            self.mementos.entry(cid).or_insert(env);
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
        // site: keep the first, let the substrate use whichever it loaded
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
        for (k, v) in other.aliasing_pair_to_memento {
            self.aliasing_pair_to_memento.entry(k).or_insert(v);
        }
        for (k, v) in other.pin_invariant_to_memento {
            self.pin_invariant_to_memento.entry(k).or_insert(v);
        }
    }
}

/// `MementoPool` implements `OpacityMementoLookup` so that
/// `compose_function_contracts_checked` in `libprovekit` can query
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
    fn has_aliasing_memento(&self, formal_a: &str, formal_b: &str) -> bool {
        // Check if the pool has an aliasing memento for this pair of formals.
        // Canonicalize by sorting the pair.
        let mut pair = (formal_a.to_string(), formal_b.to_string());
        if pair.0 > pair.1 {
            pair = (pair.1, pair.0);
        }
        self.aliasing_pair_to_memento.contains_key(&pair)
    }
    fn lookup_pin_invariant(
        &self,
        function_cid: &str,
        target: &str,
    ) -> Option<PinInvariantMementoView> {
        let key = format!("{}\x00{}", function_cid, target);
        let memento_cid = self.pin_invariant_to_memento.get(&key)?;
        let memento = self.mementos.get(memento_cid)?;
        let invariant = memento_body_field(memento, "invariant")?
            .as_str()?
            .to_string();
        Some(PinInvariantMementoView {
            function_cid: function_cid.to_string(),
            pinned_target: target.to_string(),
            invariant,
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
                let entries: Vec<(String, _)> = map
                    .iter()
                    .map(|(k, v)| (k.clone(), json_to_value(v)))
                    .collect();
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
    /// The atomic predicate the matched call ctor sits directly inside, if
    /// the call was found as an argument of an atomic (e.g. the `=` in a
    /// harvested `assert_eq!(double(3), 6)` -> `=(double(3), 6)`). Captured
    /// so the body-discharge path can derive the postcondition `Q` (the
    /// atomic with the call replaced by `result`). `None` when the call was
    /// not directly under an atomic. Does not participate in any CID.
    pub containing_atomic: Option<Json>,
}

#[derive(Debug, Default, Clone)]
pub struct ResolvedProperty {
    pub cid: String,
    pub ir_formula: Option<Json>,
    pub ir_kit_version: String,
    /// True iff the resolved target contract is a body-derived op-contract
    /// (body-bearing), not a plain refinement target.
    ///
    /// The canonical marker is a non-empty `formals` array on the contract
    /// body: that is what `core::bind::bind_function_bridge` mints and what
    /// body-bearing test fixtures construct. If a future contract shape
    /// carries body markers under a different field, it MUST be recognized
    /// here (`resolve_target::run` is the single setter) so the honesty
    /// boundary stays complete.
    ///
    /// A body-bearing target whose obligation cannot be reduced and
    /// discharged MUST be refused, never vacuous-passed -- the "no
    /// precondition => vacuously true" shortcut is only legitimate for
    /// genuinely non-body-bearing claims. Both consumers enforce this before
    /// their vacuous-discharge branch: `cmd_verify::verify_one_claim` and
    /// `runner::work_one`.
    pub target_is_body_bearing: bool,
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
            "Expected transitive proof for P → R, got {:?}",
            result
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
            "Expected direct proof, got {:?}",
            result
        );
    }

    #[test]
    fn reflexive_implication_always_holds() {
        let pool = MementoPool::default();

        let p = "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

        let result = pool.can_implies(p, p);
        assert!(
            matches!(result, ImplicationResult::ProvenReflexive),
            "Expected reflexive proof, got {:?}",
            result
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
            "Expected unknown, got {:?}",
            result
        );
    }

    // ---- PinInvariantMemento round-trip (real pool) ----

    fn make_pin_invariant_memento(
        cid: &str,
        function_cid: &str,
        target: &str,
        invariant: &str,
    ) -> Json {
        json!({
            "cid": cid,
            "envelope": {
                "signer": "ed25519:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "declaredAt": "2026-05-05T00:00:00Z",
                "signature": "ed25519:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            },
            "header": {
                "schemaVersion": "1",
                "kind": "pin-invariant",
                "cid": cid,
                "functionCid": function_cid,
                "pinnedTarget": target
            },
            "metadata": {
                "invariant": invariant
            }
        })
    }

    #[test]
    fn pin_invariant_insert_lookup_roundtrip() {
        let mut pool = MementoPool::default();
        let fc = "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let m_cid = "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string();
        pool.insert(
            m_cid.clone(),
            make_pin_invariant_memento(&m_cid, fc, "pin", "0 <= state"),
        );
        let view = pool.lookup_pin_invariant(fc, "pin");
        assert!(view.is_some(), "expected Some after insert");
        let v = view.unwrap();
        assert_eq!(v.pinned_target, "pin");
        assert!(!v.invariant.is_empty());
        assert_eq!(v.function_cid, fc);
    }

    #[test]
    fn pin_invariant_cross_function_cid_mismatch() {
        let mut pool = MementoPool::default();
        let fc_a = "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let fc_b = "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let m_cid = "blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string();
        pool.insert(
            m_cid.clone(),
            make_pin_invariant_memento(&m_cid, fc_a, "pin", "0 <= state"),
        );
        // Same target "pin" but different function CID: should NOT match
        let view = pool.lookup_pin_invariant(fc_b, "pin");
        assert!(view.is_none(), "cross-function-CID lookup must return None");
    }

    #[test]
    fn pin_invariant_v11_flat_shape_roundtrip() {
        // v1.1 flat shape: no envelope wrapper, fields live in evidence.body.
        // This exercises the fallback path in memento_body_field that reads
        // from /evidence/body instead of /header and /metadata.
        let mut pool = MementoPool::default();
        let fc = "blake3-512:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
        let m_cid = "blake3-512:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
        let flat_memento = json!({
            "cid": m_cid,
            "evidence": {
                "kind": "pin-invariant",
                "body": {
                    "functionCid": fc,
                    "pinnedTarget": "pin",
                    "invariant": "state >= 0"
                }
            }
        });
        pool.insert(m_cid.to_string(), flat_memento);
        let view = pool.lookup_pin_invariant(fc, "pin");
        assert!(view.is_some(), "v1.1 flat memento must be found via lookup");
        let v = view.unwrap();
        assert_eq!(v.pinned_target, "pin");
        assert_eq!(v.invariant, "state >= 0");
        assert_eq!(v.function_cid, fc);
    }

    // ---- A precondition is an obligation, not a verified fact ----

    fn make_contract_memento(cid: &str, name: &str, pre: &Json, post: &Json) -> Json {
        // v1.1 flat shape: evidence.kind="contract", derived hashes in body.
        json!({
            "cid": cid,
            "evidence": {
                "kind": "contract",
                "body": {
                    "contractName": name,
                    "pre": pre,
                    "post": post,
                    "preHash": compute_formula_cid(pre),
                    "postHash": compute_formula_cid(post),
                }
            }
        })
    }

    #[test]
    fn precondition_is_obligation_not_verified_fact() {
        // The missing-edge hole: indexing a contract's preHash into the
        // "verified formulas" map (formula_to_memento) makes Tier 0
        // `pool.verify(consumer_pre)` self-discharge — a callsite's consumer
        // precondition is satisfied merely by the callee DECLARING it. A
        // precondition is an obligation to discharge, never an established
        // fact. Only the post (and inv) are guarantees.
        let mut pool = MementoPool::default();
        let pre = json!({
            "kind": "atomic", "pred": "ge",
            "args": [{"kind":"var","name":"encoding"}, {"kind":"const","value":0}]
        });
        let post = json!({
            "kind": "atomic", "pred": "eq",
            "args": [{"kind":"var","name":"result"}, {"kind":"var","name":"value"}]
        });
        let m_cid =
            "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string();
        pool.insert(
            m_cid.clone(),
            make_contract_memento(&m_cid, "content_address", &pre, &post),
        );

        // A postcondition IS an established fact (the function guarantees it).
        assert!(
            pool.verify(&post).is_some(),
            "a contract's post must remain a verified fact"
        );

        // A bare precondition must NOT count as verified — else any callsite's
        // consumer pre self-discharges at Tier 0 and the missing edge hides.
        assert!(
            pool.verify(&pre).is_none(),
            "a contract's bare pre must NOT be treated as a verified fact"
        );
    }
}
