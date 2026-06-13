// SPDX-License-Identifier: Apache-2.0
//
// Slice 1 of the superposition report (goal: the content-addressed universe
// DAG). A `Universe` is NOT a cloned world. It is a chain of `(fact,
// [mFact1, mFact2, ...])` links: a spine of DETERMINED facts (holding in every
// world, stored once, referenced by CID) interleaved with FORK groups
// (mutually-exclusive alternatives, exactly one per world).
//
// The whole world-space — 2^M for M binary forks — lives as a chain whose
// on-disk size is LINEAR in `N + Sigma(fork sizes)`, never the 2^M product.
//
//   getFixedpoint()  =  the chain of asserted (consistent) facts so far.
//   .fork(alts)      =  append a fork link, SHARING the entire prefix by Arc.
//                       No copy: `Arc::clone(self)` only bumps a refcount.
//
// Every `fact` / fork member is a CID — in practice the `contract_cid` of a
// `ContractDecl` already living in the proof envelope's members map, so forks
// share the base for free. This module is the data model + the two-level CID
// (per-world walkable proof / per-superposition order-invariant handle) + the
// factored world reconstruction. The consistency CHECK that decides
// assert-vs-fork (z3) is a later slice; here `assert`/`fork` are pure builders.

use std::sync::Arc;

use sugar_canonicalizer::{encode_jcs, Value};

use crate::canonical::{cid_of_value, jcs_bytes_of_value};

/// One link of the chain: either a determined fact (the spine, every world) or
/// a fork group (mutually-exclusive members, exactly one per world).
///
/// The user's `(fact, [mFact1, mFact2, ...])` shorthand is recovered by pairing
/// each `Fork` with the `Determined` link(s) preceding it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    /// A warranted fact that holds in EVERY world. Carries its CID.
    Determined(String),
    /// A mutual-exclusion group: exactly one member holds per world.
    /// `len == 0` => the output survives in NO world (a contradiction with no
    /// restoration) => the superposition is Undecidable.
    /// `len == 1` => a collapsed fork (effectively determined).
    /// `len >= 2` => a genuine fork.
    Fork(Vec<String>),
}

impl Statement {
    /// Canonical value of the statement for the commit-object node CID.
    fn to_value(&self) -> Arc<Value> {
        match self {
            Statement::Determined(c) => {
                Value::object(vec![("determined".to_string(), Value::string(c.clone()))])
            }
            Statement::Fork(alts) => Value::object(vec![(
                "fork".to_string(),
                Value::array(alts.iter().cloned().map(Value::string).collect()),
            )]),
        }
    }
}

/// The witness that licenses a node's place in the chain. The commit-object node
/// CID commits to (base, statement, witness), so the SAME statement with a
/// DIFFERENT witness is a DIFFERENT node — the proof of WHY it joined is part of
/// its identity, recomputable by a third party.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsistencyWitness {
    /// Asserted consistent without a check yet (slice-3 default / genesis spine).
    Asserted,
    /// Walked: SAT against the accumulated universe conjoined with the vendor's
    /// pins. The statement joined the universe.
    Sat,
    /// Walked: UNSAT — carries the minimal unsat core (sorted CIDs of the
    /// conflicting statements) that forced the fork.
    Unsat { core: Vec<String> },
}

impl ConsistencyWitness {
    fn to_value(&self) -> Arc<Value> {
        match self {
            ConsistencyWitness::Asserted => {
                Value::object(vec![("witness".to_string(), Value::string("asserted"))])
            }
            ConsistencyWitness::Sat => {
                Value::object(vec![("witness".to_string(), Value::string("sat"))])
            }
            ConsistencyWitness::Unsat { core } => {
                let mut c = core.clone();
                c.sort();
                Value::object(vec![
                    ("core".to_string(), Value::array(c.into_iter().map(Value::string).collect())),
                    ("witness".to_string(), Value::string("unsat")),
                ])
            }
        }
    }
}

/// An immutable, structurally-shared chain. `.fork()`/`.assert()` return a new
/// head linking onto `self` by Arc — the prefix is never copied.
#[derive(Debug, Clone)]
pub enum Universe {
    Empty,
    Link {
        stmt: Statement,
        /// Why this node joined the chain — part of the commit-object identity.
        witness: ConsistencyWitness,
        prev: Arc<Universe>,
    },
}

/// Strength = the count of worlds that survive walking against ALL known vendor
/// assertions. Check, not model: each world is kept iff it is SAT against the
/// vendor's own pins; the survivor count is the grade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strength {
    /// Exactly one surviving reading: the output is pinned. (count == 1)
    Strong,
    /// More than one consistent-yet-mutually-exclusive reading. (count > 1)
    /// The LOGIC is sound — every survivor is internally consistent against the
    /// vendor; what is unpinned is WHICH. Bugs here live in ordering, not logic.
    Weak,
    /// No surviving reading: the code contradicts its own sworn assertions.
    /// (count == 0)
    Undecidable,
}

impl Strength {
    /// Stable machine tag for the content-addressed report.
    pub fn tag(&self) -> &'static str {
        match self {
            Strength::Strong => "strong",
            Strength::Weak => "weak",
            Strength::Undecidable => "undecidable",
        }
    }

    /// The report's verdict line for this grade — what the vendor reads.
    pub fn verdict(&self) -> &'static str {
        match self {
            Strength::Strong => "Only one reading made sense.",
            Strength::Weak => {
                "Multiple consistent but mutually-exclusive readings — \
                 bugs live in ordering, not logic. Collapse it: pin it, or get \
                 the side effect off the critical path."
            }
            Strength::Undecidable => {
                "No consistent world — the code contradicts its own assertions. \
                 Get your act together."
            }
        }
    }
}

impl Universe {
    /// The empty consistent closure — the fixedpoint before any fact.
    pub fn empty() -> Arc<Universe> {
        Arc::new(Universe::Empty)
    }

    /// Extend the fixedpoint with a determined fact (holds in every world).
    /// Shares the entire prior chain by Arc; no copy. Witness defaults to
    /// `Asserted` (no check yet); the engine uses `assert_with`.
    pub fn assert(self: &Arc<Self>, fact: impl Into<String>) -> Arc<Universe> {
        self.assert_with(fact, ConsistencyWitness::Asserted)
    }

    /// Determined-fact extension carrying the witness that licensed it (the
    /// engine's SAT certificate). Shares the prior chain by Arc; no copy.
    pub fn assert_with(
        self: &Arc<Self>,
        fact: impl Into<String>,
        witness: ConsistencyWitness,
    ) -> Arc<Universe> {
        Arc::new(Universe::Link {
            stmt: Statement::Determined(fact.into()),
            witness,
            prev: Arc::clone(self),
        })
    }

    /// Append a fork: a mutual-exclusion group of alternatives (CIDs), exactly
    /// one per world. Shares the entire prior chain by Arc; no copy.
    pub fn fork(self: &Arc<Self>, alternatives: Vec<String>) -> Arc<Universe> {
        self.fork_with(alternatives, ConsistencyWitness::Asserted)
    }

    /// Fork carrying the witness that forced it (the minimal unsat core). Shares
    /// the prior chain by Arc; no copy.
    pub fn fork_with(
        self: &Arc<Self>,
        alternatives: Vec<String>,
        witness: ConsistencyWitness,
    ) -> Arc<Universe> {
        Arc::new(Universe::Link {
            stmt: Statement::Fork(alternatives),
            witness,
            prev: Arc::clone(self),
        })
    }

    /// The consistent closure. In the git model `getFixedpoint().fork()` is the
    /// canonical move. Slice 3: with no engine yet, an all-asserted chain IS its
    /// own fixedpoint (every assert was consistent by construction), so this
    /// returns self by Arc; slice 4's engine performs real saturation.
    pub fn fixedpoint(self: &Arc<Self>) -> Arc<Universe> {
        Arc::clone(self)
    }

    /// The commit-object CID of THIS node: a content hash over (base node CID,
    /// statement, witness) — exactly git's (parent, delta) shape. Genesis
    /// (`Empty`) has a fixed CID; shared ancestry yields identical base CIDs, so
    /// the chain dedups by hash. Distinct from `superposition_cid` (the whole-set
    /// order-invariant handle): this identifies one node and its lineage.
    pub fn node_cid(&self) -> String {
        match self {
            Universe::Empty => cid_of_value(&Value::object(vec![(
                "kind".to_string(),
                Value::string("universe-genesis"),
            )])),
            Universe::Link {
                stmt,
                witness,
                prev,
            } => {
                let node = Value::object(vec![
                    ("base".to_string(), Value::string(prev.node_cid())),
                    ("kind".to_string(), Value::string("universe-node")),
                    ("statement".to_string(), stmt.to_value()),
                    ("witness".to_string(), witness.to_value()),
                ]);
                cid_of_value(&node)
            }
        }
    }

    /// Statements in forward (oldest-first) order. Borrows tied to `self`.
    pub fn forward_statements(&self) -> Vec<&Statement> {
        let mut out = Vec::new();
        let mut cur = self;
        loop {
            match cur {
                Universe::Empty => break,
                Universe::Link { stmt, prev, .. } => {
                    out.push(stmt);
                    cur = prev;
                }
            }
        }
        out.reverse();
        out
    }

    /// The N determined facts — the warranted core, true in every world.
    pub fn determined_facts(&self) -> Vec<&str> {
        self.forward_statements()
            .into_iter()
            .filter_map(|s| match s {
                Statement::Determined(c) => Some(c.as_str()),
                Statement::Fork(_) => None,
            })
            .collect()
    }

    /// The M fork groups — the mutual-exclusion groups (the Dr-Who loops).
    pub fn fork_groups(&self) -> Vec<&[String]> {
        self.forward_statements()
            .into_iter()
            .filter_map(|s| match s {
                Statement::Fork(alts) => Some(alts.as_slice()),
                Statement::Determined(_) => None,
            })
            .collect()
    }

    /// Number of distinct worlds = product of fork-group sizes. FACTORED: read
    /// off the chain in linear time, never by enumerating the product.
    /// Saturates at u128::MAX (the count is a strength signal, not an index).
    pub fn world_count(&self) -> u128 {
        let mut acc: u128 = 1;
        for s in self.forward_statements() {
            if let Statement::Fork(alts) = s {
                acc = acc.saturating_mul(alts.len() as u128);
            }
        }
        acc
    }

    /// Strength grade from the live universe-count.
    pub fn strength(&self) -> Strength {
        match self.world_count() {
            0 => Strength::Undecidable,
            1 => Strength::Strong,
            _ => Strength::Weak,
        }
    }

    pub fn is_collapsed(&self) -> bool {
        self.world_count() == 1
    }

    /// The per-world CID: walk the chain picking one alternative per fork to get
    /// a linear list of fact CIDs (a "walkable proof"), then CID that list.
    /// `index` in `0..world_count`; returns None if out of range (incl. count 0).
    /// O(chain length) — reconstructs world `index` WITHOUT materializing any
    /// other world.
    pub fn world_cid(&self, index: u128) -> Option<String> {
        let total = self.world_count();
        if total == 0 || index >= total {
            return None;
        }
        let stmts = self.forward_statements();
        // Mixed-radix decode of `index` across the fork groups (forward order,
        // first fork = least significant). Bijective over 0..total.
        let fork_radices: Vec<u128> = stmts
            .iter()
            .filter_map(|s| match s {
                Statement::Fork(alts) => Some(alts.len() as u128),
                Statement::Determined(_) => None,
            })
            .collect();
        let mut digits: Vec<usize> = Vec::with_capacity(fork_radices.len());
        let mut rem = index;
        for r in &fork_radices {
            digits.push((rem % r) as usize);
            rem /= r;
        }
        // Walk forward, emitting the determined facts and the chosen alternatives.
        let mut chosen: Vec<Arc<Value>> = Vec::new();
        let mut fi = 0usize;
        for s in &stmts {
            match s {
                Statement::Determined(c) => chosen.push(Value::string(c.clone())),
                Statement::Fork(alts) => {
                    chosen.push(Value::string(alts[digits[fi]].clone()));
                    fi += 1;
                }
            }
        }
        let node = Value::object(vec![
            ("facts".to_string(), Value::array(chosen)),
            ("kind".to_string(), Value::string("world")),
        ]);
        Some(cid_of_value(&node))
    }

    /// The order-invariant federation handle. Identity is the multiset of
    /// determined facts + the set of fork groups (each a sorted member set) —
    /// order is out of the membrane, so the chain is canonicalized by sorting.
    pub fn canonical_value(&self) -> Arc<Value> {
        let stmts = self.forward_statements();
        let determined: Vec<String> = stmts
            .iter()
            .filter_map(|s| match s {
                Statement::Determined(c) => Some(c.clone()),
                Statement::Fork(_) => None,
            })
            .collect();
        let forks: Vec<Vec<String>> = stmts
            .iter()
            .filter_map(|s| match s {
                Statement::Fork(alts) => Some(alts.clone()),
                Statement::Determined(_) => None,
            })
            .collect();
        canonical_value_of(&determined, &forks)
    }

    /// CID of the whole superposition (order-invariant handle).
    pub fn superposition_cid(&self) -> String {
        cid_of_value(&self.canonical_value())
    }

    /// The bytes a `.proof` envelope carries as this superposition's member:
    /// the JCS-canonical bytes of the order-invariant node. A third party CIDs
    /// the member by `blake3_512_of(bytes)`, which equals `superposition_cid`.
    pub fn member_bytes(&self) -> Vec<u8> {
        jcs_bytes_of_value(&self.canonical_value())
    }
}

/// Build the order-invariant canonical node from a determined-fact list and a
/// list of fork groups. Shared by `Universe::canonical_value` and the parse-back
/// `SuperpositionView`, so the on-disk node and the recovered structure CID to
/// the same handle. Idempotent: sorts everything, so re-canonicalizing a parsed
/// view reproduces the byte-identical node.
fn canonical_value_of(determined: &[String], forks: &[Vec<String>]) -> Arc<Value> {
    let mut det = determined.to_vec();
    det.sort();
    let mut fk: Vec<Arc<Value>> = forks
        .iter()
        .map(|members| {
            let mut m = members.clone();
            m.sort();
            Value::array(m.into_iter().map(Value::string).collect())
        })
        .collect();
    fk.sort_by(|a, b| encode_jcs(a).cmp(&encode_jcs(b)));
    Value::object(vec![
        (
            "determined".to_string(),
            Value::array(det.into_iter().map(Value::string).collect()),
        ),
        ("forks".to_string(), Value::array(fk)),
        ("kind".to_string(), Value::string("superposition")),
    ])
}

/// The order-free structure a third party recovers from a superposition member's
/// on-disk bytes: the N determined facts + the M fork groups. This IS the report
/// content (minus per-world walking), recomputable from the `.proof` alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuperpositionView {
    pub determined: Vec<String>,
    pub forks: Vec<Vec<String>>,
}

impl SuperpositionView {
    /// Parse a superposition member's JCS bytes back into its structure. Order
    /// is not recovered (it is out of the membrane and never was in the CID);
    /// the canonical multiset/sets are.
    pub fn from_member_bytes(bytes: &[u8]) -> Result<Self, String> {
        let json: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|e| format!("not JSON: {e}"))?;
        let obj = json.as_object().ok_or("member is not an object")?;
        match obj.get("kind").and_then(|k| k.as_str()) {
            Some("superposition") => {}
            other => return Err(format!("not a superposition node: kind={other:?}")),
        }
        let determined = string_array(obj.get("determined"), "determined")?;
        let forks_json = obj
            .get("forks")
            .and_then(|f| f.as_array())
            .ok_or("missing forks array")?;
        let mut forks = Vec::with_capacity(forks_json.len());
        for (i, group) in forks_json.iter().enumerate() {
            forks.push(string_array(Some(group), &format!("forks[{i}]"))?);
        }
        Ok(SuperpositionView { determined, forks })
    }

    /// Re-derive the superposition CID from the recovered structure. Equals the
    /// originating `Universe::superposition_cid` (idempotent canonicalization).
    pub fn cid(&self) -> String {
        cid_of_value(&canonical_value_of(&self.determined, &self.forks))
    }

    /// Number of worlds = product of fork-group sizes (saturating). FACTORED.
    pub fn world_count(&self) -> u128 {
        self.forks
            .iter()
            .fold(1u128, |acc, g| acc.saturating_mul(g.len() as u128))
    }

    pub fn strength(&self) -> Strength {
        match self.world_count() {
            0 => Strength::Undecidable,
            1 => Strength::Strong,
            _ => Strength::Weak,
        }
    }
}

/// Where a lifted statement lives in the world-space. This is the slice-2
/// vocabulary: the lift "tags each emitted statement with the world(s) it
/// survives in, so the accumulator can fork" — no flat decl list.
///
/// At lift time NOTHING has been walked against a pin, so a fresh warrant is a
/// `Candidate`: an unchecked demand that provisionally claims the base (all
/// worlds) until a check keeps it (-> `Determined`) or forks it (-> `ForkMember`).
/// A Candidate is NOT a verdict — `Accumulator::strength` withholds a grade
/// until the engine has walked (no fake green).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldMembership {
    /// Lifted, not yet walked against any vendor pin. Claims the base; no verdict.
    Candidate,
    /// Walked and SAT in every world: part of the determined spine.
    Determined,
    /// Walked into a fork: alternative `member` of mutual-exclusion group `group`.
    ForkMember { group: u32, member: u32 },
}

impl WorldMembership {
    pub fn to_json(self) -> serde_json::Value {
        match self {
            WorldMembership::Candidate => serde_json::json!({"membership": "candidate"}),
            WorldMembership::Determined => serde_json::json!({"membership": "determined"}),
            WorldMembership::ForkMember { group, member } => {
                serde_json::json!({"membership": "fork", "group": group, "member": member})
            }
        }
    }

    pub fn from_json(v: &serde_json::Value) -> Result<Self, String> {
        match v.get("membership").and_then(|m| m.as_str()) {
            Some("candidate") => Ok(WorldMembership::Candidate),
            Some("determined") => Ok(WorldMembership::Determined),
            Some("fork") => {
                let group = v
                    .get("group")
                    .and_then(|g| g.as_u64())
                    .ok_or("fork membership missing group")? as u32;
                let member = v
                    .get("member")
                    .and_then(|m| m.as_u64())
                    .ok_or("fork membership missing member")? as u32;
                Ok(WorldMembership::ForkMember { group, member })
            }
            other => Err(format!("unknown membership: {other:?}")),
        }
    }
}

/// A lifted statement tagged with its world-membership: the wire unit that
/// replaces a bare decl in the flat list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaggedStatement {
    /// CID of the lifted fact (in practice a `contract_cid`).
    pub cid: String,
    pub membership: WorldMembership,
}

/// Accumulates tagged statements into the factored `Universe`. This is "the
/// accumulator" of slice 2: statements arrive tagged so it can fork them into
/// the chain. The incremental check-sat engine (slice 4) drives the tags;
/// here we only fold tagged statements into a chain and honestly withhold a
/// strength grade until `mark_walked()`.
#[derive(Debug, Default, Clone)]
pub struct Accumulator {
    walked: bool,
    statements: Vec<TaggedStatement>,
}

impl Accumulator {
    pub fn new() -> Self {
        Accumulator::default()
    }

    pub fn push(&mut self, cid: impl Into<String>, membership: WorldMembership) {
        self.statements.push(TaggedStatement {
            cid: cid.into(),
            membership,
        });
    }

    /// The engine calls this once it has actually walked the candidates against
    /// the vendor's pins. Until then `strength()` returns None — an unwalked
    /// accumulator has reached no verdict.
    pub fn mark_walked(&mut self) {
        self.walked = true;
    }

    pub fn is_walked(&self) -> bool {
        self.walked
    }

    pub fn statements(&self) -> &[TaggedStatement] {
        &self.statements
    }

    /// Fold the tagged statements into the factored chain. Candidates and
    /// Determined statements become spine facts; ForkMembers group by `group`
    /// (ordered by `member`) into forks. Order is out of the membrane, so the
    /// spine is sorted for determinism; the superposition CID is unaffected.
    pub fn universe(&self) -> Arc<Universe> {
        use std::collections::BTreeMap;
        let mut spine: Vec<String> = self
            .statements
            .iter()
            .filter_map(|s| match s.membership {
                WorldMembership::Candidate | WorldMembership::Determined => Some(s.cid.clone()),
                WorldMembership::ForkMember { .. } => None,
            })
            .collect();
        spine.sort();
        let mut groups: BTreeMap<u32, Vec<(u32, String)>> = BTreeMap::new();
        for s in &self.statements {
            if let WorldMembership::ForkMember { group, member } = s.membership {
                groups.entry(group).or_default().push((member, s.cid.clone()));
            }
        }
        let mut u = Universe::empty();
        for cid in spine {
            u = u.assert(cid);
        }
        for (_group, mut members) in groups {
            members.sort_by_key(|(m, _)| *m);
            let alts: Vec<String> = members.into_iter().map(|(_, c)| c).collect();
            u = u.fork(alts);
        }
        u
    }

    /// The strength grade — ONLY after walking. Pre-walk we have made no verdict
    /// (every statement is an unchecked Candidate); returning a grade here would
    /// be a fake green. None means "not yet walked".
    pub fn strength(&self) -> Option<Strength> {
        if self.walked {
            Some(self.universe().strength())
        } else {
            None
        }
    }
}

fn string_array(v: Option<&serde_json::Value>, field: &str) -> Result<Vec<String>, String> {
    let arr = v
        .and_then(|x| x.as_array())
        .ok_or_else(|| format!("{field} must be an array"))?;
    arr.iter()
        .map(|e| {
            e.as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| format!("{field} entries must be strings"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cid(tag: &str) -> String {
        // A stand-in fact CID. Real facts are contract_cids; slice 1 only needs
        // distinct opaque strings.
        format!("blake3-512:{tag}")
    }

    #[test]
    fn determined_only_is_strong_single_world() {
        let u = Universe::empty()
            .assert(cid("a"))
            .assert(cid("b"))
            .assert(cid("c"));
        assert_eq!(u.world_count(), 1);
        assert_eq!(u.strength(), Strength::Strong);
        assert!(u.is_collapsed());
        assert_eq!(u.determined_facts(), vec!["blake3-512:a", "blake3-512:b", "blake3-512:c"]);
        assert!(u.fork_groups().is_empty());
        // The single world has a CID; index 1 is out of range.
        assert!(u.world_cid(0).is_some());
        assert!(u.world_cid(1).is_none());
    }

    #[test]
    fn one_binary_fork_two_worlds() {
        let u = Universe::empty()
            .assert(cid("a"))
            .fork(vec![cid("x"), cid("y")]);
        assert_eq!(u.world_count(), 2);
        assert_eq!(u.strength(), Strength::Weak);
        let w0 = u.world_cid(0).unwrap();
        let w1 = u.world_cid(1).unwrap();
        assert_ne!(w0, w1, "the two worlds must have distinct CIDs");
        assert!(u.world_cid(2).is_none());
        assert_eq!(u.fork_groups().len(), 1);
        assert_eq!(u.fork_groups()[0].len(), 2);
    }

    #[test]
    fn strength_verdicts_match_the_three_readings() {
        assert!(Strength::Strong.verdict().contains("one reading"));
        assert!(Strength::Weak.verdict().contains("ordering, not logic"));
        assert!(Strength::Undecidable.verdict().contains("contradicts its own"));
    }

    #[test]
    fn empty_fork_is_undecidable() {
        let u = Universe::empty().assert(cid("a")).fork(vec![]);
        assert_eq!(u.world_count(), 0);
        assert_eq!(u.strength(), Strength::Undecidable);
        assert!(u.world_cid(0).is_none());
    }

    #[test]
    fn superposition_cid_is_order_invariant() {
        // Same facts + same forks, asserted in different link orders, must yield
        // the same superposition CID (order is out of the membrane). Per-world
        // CIDs may differ; the federation handle must not.
        let a = Universe::empty()
            .assert(cid("a"))
            .assert(cid("b"))
            .fork(vec![cid("x"), cid("y")]);
        let b = Universe::empty()
            .fork(vec![cid("y"), cid("x")]) // members reversed
            .assert(cid("b"))
            .assert(cid("a")); // facts reordered
        assert_eq!(
            a.superposition_cid(),
            b.superposition_cid(),
            "superposition CID must be order-invariant"
        );
    }

    #[test]
    fn distinct_forks_distinct_superposition_cid() {
        let a = Universe::empty().assert(cid("a")).fork(vec![cid("x"), cid("y")]);
        let b = Universe::empty().assert(cid("a")).fork(vec![cid("x"), cid("z")]);
        assert_ne!(a.superposition_cid(), b.superposition_cid());
    }

    #[test]
    fn fork_shares_base_no_clone() {
        // .fork() must share the entire prefix by Arc, not copy it.
        let base = Universe::empty().assert(cid("a")).assert(cid("b"));
        let w1 = base.fork(vec![cid("x"), cid("y")]);
        let w2 = base.fork(vec![cid("p"), cid("q")]);
        // Both forks' `prev` must be the SAME allocation as `base`.
        let prev1 = match &*w1 {
            Universe::Link { prev, .. } => Arc::as_ptr(prev),
            _ => panic!(),
        };
        let prev2 = match &*w2 {
            Universe::Link { prev, .. } => Arc::as_ptr(prev),
            _ => panic!(),
        };
        assert_eq!(prev1, Arc::as_ptr(&base), "fork must share the base by Arc");
        assert_eq!(prev2, Arc::as_ptr(&base), "fork must share the base by Arc");
        assert_eq!(prev1, prev2, "both forks share one base allocation");
    }

    #[test]
    fn factored_not_enumerated() {
        // 30 binary forks => 2^30 worlds, but the chain stays LINEAR on disk and
        // any single world reconstructs in O(chain length) — no 2^30 blowup.
        let mut u = Universe::empty().assert(cid("root"));
        for i in 0..30 {
            u = u.fork(vec![cid(&format!("l{i}")), cid(&format!("r{i}"))]);
        }
        assert_eq!(u.world_count(), 1u128 << 30);
        assert_eq!(u.strength(), Strength::Weak);

        // On-disk (canonical) size is linear, not exponential: 30 forks * 2
        // members + 1 determined fact. Bytes stay tiny.
        let bytes = encode_jcs(&u.canonical_value());
        assert!(
            bytes.len() < 4096,
            "factored form must be linear-sized, got {} bytes",
            bytes.len()
        );

        // Reconstruct a deep world by index without materializing the rest.
        let w = u.world_cid(123_456_789).unwrap();
        let w2 = u.world_cid(123_456_790).unwrap();
        assert_ne!(w, w2);
        // Top of range valid, one past is not.
        assert!(u.world_cid((1u128 << 30) - 1).is_some());
        assert!(u.world_cid(1u128 << 30).is_none());
    }

    #[test]
    fn member_cid_recomputes_from_bytes_alone() {
        // Third-party recomputability: the member's CID is blake3_512 of its
        // on-disk bytes, with no other input. Equals superposition_cid.
        let u = Universe::empty()
            .assert(cid("a"))
            .fork(vec![cid("x"), cid("y")]);
        let bytes = u.member_bytes();
        assert_eq!(
            sugar_canonicalizer::blake3_512_of(&bytes),
            u.superposition_cid(),
            "a third party must recompute the member CID from bytes alone"
        );
    }

    #[test]
    fn view_round_trips_structure_and_cid() {
        let u = Universe::empty()
            .assert(cid("a"))
            .assert(cid("b"))
            .fork(vec![cid("x"), cid("y")])
            .fork(vec![cid("p"), cid("q"), cid("r")]);
        let view = SuperpositionView::from_member_bytes(&u.member_bytes()).unwrap();
        // Structure recovered (canonical/sorted form).
        assert_eq!(view.determined, vec!["blake3-512:a", "blake3-512:b"]);
        assert_eq!(view.forks.len(), 2);
        assert_eq!(view.world_count(), 6);
        assert_eq!(view.strength(), Strength::Weak);
        // The recovered structure re-derives the SAME federation handle.
        assert_eq!(view.cid(), u.superposition_cid());
    }

    #[test]
    fn proof_envelope_carries_the_fork_node() {
        use sugar_proof_envelope::{build_proof_envelope, ProofEnvelopeInput};
        use std::collections::BTreeMap;

        let u = Universe::empty()
            .assert(cid("a"))
            .fork(vec![cid("x"), cid("y")]);
        let sup_cid = u.superposition_cid();

        // The .proof admits the fork node as a content-addressed member.
        let mut members: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        members.insert(sup_cid.clone(), u.member_bytes());

        let input = ProofEnvelopeInput {
            name: "superposition-slice1".to_string(),
            version: "1".to_string(),
            binary_cid: None,
            metadata: None,
            members,
            signer_cid: "ed25519:AA".to_string(),
            signer_seed: [0x42; 32],
            declared_at: "2026-06-13T00:00:00.000Z".to_string(),
        };
        let out = build_proof_envelope(&input);
        assert!(out.cid.starts_with("blake3-512:"));
        // The catalog body carries the member keyed by its CID; a verifier pulls
        // the bytes and recomputes the same superposition CID.
        let recovered = SuperpositionView::from_member_bytes(&input.members[&sup_cid]).unwrap();
        assert_eq!(recovered.cid(), sup_cid);
    }

    #[test]
    fn node_cid_is_a_commit_object_over_base_statement_witness() {
        // Deterministic, and grows with the chain (each node a distinct commit).
        let g = Universe::empty();
        let genesis = g.node_cid();
        assert_eq!(genesis, Universe::empty().node_cid(), "genesis CID is fixed");

        let a = g.assert(cid("a"));
        let ab = a.assert(cid("b"));
        assert_ne!(a.node_cid(), genesis);
        assert_ne!(ab.node_cid(), a.node_cid());
        // Recompute from scratch — third party gets the same node CID.
        let ab2 = Universe::empty().assert(cid("a")).assert(cid("b"));
        assert_eq!(ab.node_cid(), ab2.node_cid());
    }

    #[test]
    fn same_statement_different_witness_is_a_different_node() {
        // The witness is part of identity: a fact that joined by SAT is a
        // different commit-object than the same fact merely Asserted.
        let base = Universe::empty().assert(cid("a"));
        let by_assert = base.assert_with(cid("b"), ConsistencyWitness::Asserted);
        let by_sat = base.assert_with(cid("b"), ConsistencyWitness::Sat);
        assert_ne!(by_assert.node_cid(), by_sat.node_cid());
        // But the whole-set handle ignores the witness (it is a relation on the
        // facts, not on why they joined).
        assert_eq!(by_assert.superposition_cid(), by_sat.superposition_cid());
    }

    #[test]
    fn shared_ancestry_dedups_by_base_cid() {
        // git-model: two forks off one fixedpoint share the SAME base node CID.
        let base = Universe::empty().assert(cid("a")).assert(cid("b"));
        let fp = base.fixedpoint();
        let w1 = fp.fork(vec![cid("x"), cid("y")]);
        let w2 = fp.fork(vec![cid("p"), cid("q")]);
        let base_of = |u: &Arc<Universe>| match &**u {
            Universe::Link { prev, .. } => prev.node_cid(),
            _ => panic!(),
        };
        assert_eq!(
            base_of(&w1),
            base_of(&w2),
            "shared ancestry must dedup to one base CID"
        );
        assert_eq!(base_of(&w1), base.node_cid());
    }

    #[test]
    fn fixedpoint_is_identity_in_slice3() {
        let base = Universe::empty().assert(cid("a"));
        assert!(Arc::ptr_eq(&base, &base.fixedpoint()));
    }

    #[test]
    fn unsat_witness_core_is_in_the_node_cid() {
        let base = Universe::empty().assert(cid("a"));
        let f1 = base.fork_with(
            vec![cid("x"), cid("y")],
            ConsistencyWitness::Unsat { core: vec![cid("a"), cid("x")] },
        );
        let f2 = base.fork_with(
            vec![cid("x"), cid("y")],
            ConsistencyWitness::Unsat { core: vec![cid("a"), cid("z")] },
        );
        // Different core => different node (the WHY is recorded in identity).
        assert_ne!(f1.node_cid(), f2.node_cid());
        // Core order does not matter (sorted in the witness value).
        let f1b = base.fork_with(
            vec![cid("x"), cid("y")],
            ConsistencyWitness::Unsat { core: vec![cid("x"), cid("a")] },
        );
        assert_eq!(f1.node_cid(), f1b.node_cid());
    }

    #[test]
    fn world_membership_json_round_trips() {
        for m in [
            WorldMembership::Candidate,
            WorldMembership::Determined,
            WorldMembership::ForkMember { group: 2, member: 1 },
        ] {
            let j = m.to_json();
            assert_eq!(WorldMembership::from_json(&j).unwrap(), m);
        }
    }

    #[test]
    fn accumulator_withholds_strength_until_walked() {
        // No fake green: an unwalked accumulator has reached NO verdict, even
        // though its candidates would structurally form a single world.
        let mut acc = Accumulator::new();
        acc.push(cid("a"), WorldMembership::Candidate);
        acc.push(cid("b"), WorldMembership::Candidate);
        assert!(!acc.is_walked());
        assert_eq!(acc.strength(), None, "pre-walk must withhold a grade");
        acc.mark_walked();
        assert_eq!(acc.strength(), Some(Strength::Strong));
    }

    #[test]
    fn accumulator_folds_tags_into_factored_chain() {
        // Two determined facts + a fork group of two members + a fork group of
        // three => 6 worlds, two fork groups, two determined facts.
        let mut acc = Accumulator::new();
        acc.push(cid("a"), WorldMembership::Determined);
        acc.push(cid("b"), WorldMembership::Determined);
        acc.push(cid("x"), WorldMembership::ForkMember { group: 0, member: 0 });
        acc.push(cid("y"), WorldMembership::ForkMember { group: 0, member: 1 });
        acc.push(cid("p"), WorldMembership::ForkMember { group: 1, member: 0 });
        acc.push(cid("q"), WorldMembership::ForkMember { group: 1, member: 1 });
        acc.push(cid("r"), WorldMembership::ForkMember { group: 1, member: 2 });
        let u = acc.universe();
        assert_eq!(u.determined_facts().len(), 2);
        assert_eq!(u.fork_groups().len(), 2);
        assert_eq!(u.world_count(), 6);
        acc.mark_walked();
        assert_eq!(acc.strength(), Some(Strength::Weak));
    }

    #[test]
    fn accumulator_orders_fork_members_by_index() {
        // Members pushed out of order must land in member-index order in the fork.
        let mut acc = Accumulator::new();
        acc.push(cid("second"), WorldMembership::ForkMember { group: 0, member: 1 });
        acc.push(cid("first"), WorldMembership::ForkMember { group: 0, member: 0 });
        let u = acc.universe();
        let group: Vec<String> = u.fork_groups()[0].to_vec();
        assert_eq!(
            group,
            vec!["blake3-512:first".to_string(), "blake3-512:second".to_string()]
        );
    }

    #[test]
    fn mixed_radix_world_selection_is_hand_checkable() {
        // Two forks: [x,y] (radix 2) then [p,q,r] (radix 3) => 6 worlds.
        // index = first_digit + 2 * second_digit  (first fork least significant).
        let u = Universe::empty()
            .fork(vec![cid("x"), cid("y")])
            .fork(vec![cid("p"), cid("q"), cid("r")]);
        assert_eq!(u.world_count(), 6);
        // World 0 = (x, p); World 1 = (y, p); World 2 = (x, q); World 3 = (y, q).
        // Verify by reconstructing the expected linear list directly.
        let expect = |members: &[&str]| {
            let node = Value::object(vec![
                (
                    "facts".to_string(),
                    Value::array(members.iter().map(|m| Value::string(m.to_string())).collect()),
                ),
                ("kind".to_string(), Value::string("world")),
            ]);
            cid_of_value(&node)
        };
        assert_eq!(u.world_cid(0).unwrap(), expect(&["blake3-512:x", "blake3-512:p"]));
        assert_eq!(u.world_cid(1).unwrap(), expect(&["blake3-512:y", "blake3-512:p"]));
        assert_eq!(u.world_cid(2).unwrap(), expect(&["blake3-512:x", "blake3-512:q"]));
        assert_eq!(u.world_cid(3).unwrap(), expect(&["blake3-512:y", "blake3-512:q"]));
        assert_eq!(u.world_cid(5).unwrap(), expect(&["blake3-512:y", "blake3-512:r"]));
    }
}
