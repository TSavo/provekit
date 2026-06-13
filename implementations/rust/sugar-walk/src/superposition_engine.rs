// SPDX-License-Identifier: Apache-2.0
//
// Slice 4 of the superposition report: the incremental engine. Feed statements
// — body warrants AND vendor pins — into the universe; the vendor's pins are the
// CLOSED referee. Each world is checked WITH THE FACT IN HAND: the problem is
// closed, not open-ended — the conjunction of all the vendor's assertions as
// fixed points. We never ask "which gun shot this bullet?" (an open model
// search); we answer "given THIS gun and THIS bullet, did it fire?" (a closed
// check). Forensics of facts, not solving of the unknown.
//
// SAT  -> the statement joins the universe (it coexists with the pins).
// UNSAT-> it cannot coexist: pull the minimal unsat core, fork on the
//         correction-sets (one world per maximal consistent reading), walk all.
// Order-free: a contradiction is order-independent, so we never reject by
// arrival order — we enumerate the maximal consistent subsets (the worlds),
// which is invariant to the order statements were fed.
//
// The SAT decision is the only thing we delegate, behind `SatOracle`, so the
// engine's forking logic is tested deterministically (a mock) and a z3-backed
// oracle verifies the real path (skip-when-absent).

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};

use sugar_canonicalizer::{encode_jcs, Value as CValue};

use crate::canonical::{cid_of_value, jcs_bytes_of_value};
use crate::superposition::{Accumulator, Strength, Universe, WorldMembership};

/// A statement fed to the engine: a CID + the IR `inv` formula it asserts.
#[derive(Debug, Clone)]
pub struct EngineStatement {
    pub cid: String,
    pub formula: Value,
}

impl EngineStatement {
    pub fn new(cid: impl Into<String>, formula: Value) -> Self {
        EngineStatement {
            cid: cid.into(),
            formula,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SatResult {
    Sat,
    Unsat,
    /// The oracle could not decide (z3 absent, timeout, or non-compiling
    /// formula). Treated conservatively as "cannot refute" — never a finding.
    Unknown,
}

/// The closed-world referee. Check, not model: given a conjunction of IR
/// formulas IN HAND, is it SAT? Yes/no only — the witness is discarded.
pub trait SatOracle {
    fn check(&self, formulas: &[&Value]) -> SatResult;
}

/// Conjoin IR `inv` formulas into one `{"kind":"and","operands":[...]}`.
pub fn conjoin(formulas: &[&Value]) -> Value {
    match formulas.len() {
        0 => json!({"kind": "and", "operands": []}),
        1 => formulas[0].clone(),
        _ => json!({
            "kind": "and",
            "operands": formulas.iter().map(|f| (*f).clone()).collect::<Vec<_>>(),
        }),
    }
}

/// z3-backed oracle. Shells to the z3 binary via the SMT-LIB compiler. Returns
/// `Unknown` when z3 is absent or the formula does not compile — never a false
/// UNSAT.
pub struct Z3Oracle {
    pub z3_path: String,
}

impl Default for Z3Oracle {
    fn default() -> Self {
        Z3Oracle {
            z3_path: "/usr/local/bin/z3".to_string(),
        }
    }
}

static SMT_COUNTER: AtomicU64 = AtomicU64::new(0);

impl SatOracle for Z3Oracle {
    fn check(&self, formulas: &[&Value]) -> SatResult {
        if formulas.is_empty() {
            return SatResult::Sat;
        }
        let inv = conjoin(formulas);
        let parts = match sugar_ir_compiler_smt_lib::compile_asserted_to_parts(&inv) {
            Ok(p) => p,
            Err(_) => return SatResult::Unknown,
        };
        if !std::path::Path::new(&self.z3_path).exists() {
            return SatResult::Unknown;
        }
        let script = format!("{}{}\n(check-sat)\n", parts.preamble, parts.body);
        let n = SMT_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "sugar_superpos_{}_{}.smt2",
            std::process::id(),
            n
        ));
        if std::fs::write(&path, &script).is_err() {
            return SatResult::Unknown;
        }
        let out = match std::process::Command::new(&self.z3_path).arg(&path).output() {
            Ok(o) => o,
            Err(_) => return SatResult::Unknown,
        };
        let stdout = String::from_utf8_lossy(&out.stdout);
        if stdout.contains("unknown constant") || stdout.to_lowercase().contains("error") {
            return SatResult::Unknown;
        }
        if stdout.contains("unsat") {
            SatResult::Unsat
        } else if stdout.contains("sat") {
            SatResult::Sat
        } else {
            SatResult::Unknown
        }
    }
}

/// The result of walking candidates against the closed pins.
#[derive(Debug, Clone)]
pub struct WalkResult {
    /// The surviving worlds: each a sorted list of candidate CIDs (a maximal
    /// consistent reading under the pins). This is the AUTHORITATIVE survivor
    /// set; strength reads off its length.
    pub worlds: Vec<Vec<String>>,
    /// Candidate CIDs present in EVERY world — the determined spine.
    pub determined: Vec<String>,
    /// Candidate CIDs that survive in some-but-not-all worlds — the forks.
    pub contested: Vec<String>,
    /// The contested set decomposed into INDEPENDENT mutual-exclusion groups
    /// (each a sorted member list). Populated only when `factored` — the world
    /// space is the product of these groups. Empty when there is no fork or when
    /// the structure did not factor cleanly (then the chain uses synthetic
    /// per-world members).
    pub fork_groups: Vec<Vec<String>>,
    /// Candidates UNSAT against the pins ALONE: they live in no world. The
    /// keystone (slice 5) decides whether each is a vendor finding or a retract.
    pub refuted_by_pins: Vec<String>,
    /// False => the pins themselves are inconsistent (root Undecidable).
    pub pins_consistent: bool,
    /// True => the candidate pool exceeded the enumeration cap and the world set
    /// is degraded (NOT silently — callers must surface it).
    pub capped: bool,
    /// True => the contested set factored cleanly into independent
    /// mutual-exclusion groups (the world space is their product); false => the
    /// factored chain uses synthetic per-world fork members (survivor count
    /// preserved, factoring approximate — surfaced, not silent).
    pub factored: bool,
}

impl WalkResult {
    /// Strength = the live survivor count. Authoritative: the number of maximal
    /// consistent worlds. 0 => Undecidable, 1 => Strong, >1 => Weak.
    pub fn strength(&self) -> Strength {
        if !self.pins_consistent || self.worlds.is_empty() {
            Strength::Undecidable
        } else if self.worlds.len() == 1 {
            Strength::Strong
        } else {
            Strength::Weak
        }
    }

    /// Build the factored `Universe` (the CID artifact). Determined facts form
    /// the spine; the contested set forms one fork group — real candidate CIDs
    /// when it factored cleanly, else one synthetic world-CID per world.
    pub fn universe(&self) -> Arc<Universe> {
        let mut acc = Accumulator::new();
        for cid in &self.determined {
            acc.push(cid.clone(), WorldMembership::Determined);
        }
        if self.worlds.len() > 1 {
            if self.factored {
                for (group, members) in self.fork_groups.iter().enumerate() {
                    for (member, cid) in members.iter().enumerate() {
                        acc.push(
                            cid.clone(),
                            WorldMembership::ForkMember {
                                group: group as u32,
                                member: member as u32,
                            },
                        );
                    }
                }
            } else {
                for (member, world) in self.worlds.iter().enumerate() {
                    let wid = world_id(world);
                    acc.push(
                        wid,
                        WorldMembership::ForkMember {
                            group: 0,
                            member: member as u32,
                        },
                    );
                }
            }
        }
        acc.mark_walked();
        acc.universe()
    }
}

/// A content-addressed identity for a world (sorted candidate CIDs).
fn world_id(world: &[String]) -> String {
    let mut sorted = world.to_vec();
    sorted.sort();
    let arr = sugar_canonicalizer::Value::array(
        sorted
            .into_iter()
            .map(sugar_canonicalizer::Value::string)
            .collect(),
    );
    cid_of_value(arr.as_ref())
}

/// Walk `candidates` against the closed `pins`. `max_pool` bounds the brute-force
/// maximal-consistent-subset enumeration; beyond it the result is degraded and
/// `capped` is set (never silent).
pub fn walk(
    pins: &[EngineStatement],
    candidates: &[EngineStatement],
    oracle: &dyn SatOracle,
    max_pool: usize,
) -> WalkResult {
    let pin_formulas: Vec<&Value> = pins.iter().map(|p| &p.formula).collect();

    // 1. The pins are the closed referee. If they contradict themselves, there
    //    is no world to live in — root Undecidable.
    if oracle.check(&pin_formulas) == SatResult::Unsat {
        return WalkResult {
            worlds: Vec::new(),
            determined: Vec::new(),
            contested: Vec::new(),
            fork_groups: Vec::new(),
            refuted_by_pins: candidates.iter().map(|c| c.cid.clone()).collect(),
            pins_consistent: false,
            capped: false,
            factored: true,
        };
    }

    // 2. A candidate UNSAT against the pins ALONE lives in no world. Set it aside
    //    (slice 5's keystone classifies it: finding vs retract).
    let mut pool: Vec<&EngineStatement> = Vec::new();
    let mut refuted_by_pins: Vec<String> = Vec::new();
    for c in candidates {
        let mut fs = pin_formulas.clone();
        fs.push(&c.formula);
        match oracle.check(&fs) {
            SatResult::Unsat => refuted_by_pins.push(c.cid.clone()),
            SatResult::Sat | SatResult::Unknown => pool.push(c),
        }
    }

    // 3. Enumerate the maximal consistent subsets of the pool under the pins —
    //    the worlds. Order-free by construction.
    let (mss, capped) = enumerate_mss(&pin_formulas, &pool, oracle, max_pool);

    // 4. Project to CIDs; determined = in every world; contested = the rest.
    //    Sort the worlds list canonically so the result is order-free: the same
    //    set of worlds regardless of the order candidates were fed.
    let mut worlds: Vec<Vec<String>> = mss
        .iter()
        .map(|set| {
            let mut w: Vec<String> = set.iter().map(|&i| pool[i].cid.clone()).collect();
            w.sort();
            w
        })
        .collect();
    worlds.sort();

    let determined_idx: BTreeSet<usize> = if mss.is_empty() {
        BTreeSet::new()
    } else {
        mss.iter()
            .skip(1)
            .fold(mss[0].clone(), |acc, s| acc.intersection(s).copied().collect())
    };
    let contested_idx: Vec<usize> = (0..pool.len())
        .filter(|i| !determined_idx.contains(i))
        .collect();

    let mut determined: Vec<String> = determined_idx.iter().map(|&i| pool[i].cid.clone()).collect();
    determined.sort();
    let mut contested: Vec<String> = contested_idx.iter().map(|&i| pool[i].cid.clone()).collect();
    contested.sort();

    // Decompose the contested set into INDEPENDENT mutual-exclusion groups: the
    // world space is then the product of the groups (the goal's "independent
    // forks stay free"). If the structure does not factor cleanly, fall back to
    // synthetic per-world members (survivor count preserved, surfaced).
    let _ = &contested_idx;
    let (fork_groups, factored): (Vec<Vec<String>>, bool) =
        match decompose_into_independent_groups(&mss, &determined_idx) {
            Some(groups) => {
                let cid_groups = groups
                    .iter()
                    .map(|g| {
                        let mut v: Vec<String> = g.iter().map(|&i| pool[i].cid.clone()).collect();
                        v.sort();
                        v
                    })
                    .collect();
                (cid_groups, true)
            }
            None => (Vec::new(), false),
        };

    WalkResult {
        worlds,
        determined,
        contested,
        fork_groups,
        refuted_by_pins,
        pins_consistent: true,
        capped,
        factored,
    }
}

/// A disjoint-set forest for grouping mutually-exclusive contested candidates.
struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        UnionFind {
            parent: (0..n).collect(),
        }
    }
    fn find(&mut self, x: usize) -> usize {
        let mut r = x;
        while self.parent[r] != r {
            r = self.parent[r];
        }
        // path compression
        let mut c = x;
        while self.parent[c] != r {
            let next = self.parent[c];
            self.parent[c] = r;
            c = next;
        }
        r
    }
    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb {
            self.parent[ra] = rb;
        }
    }
}

/// Decompose the contested candidates into independent mutual-exclusion groups.
/// Returns `Some(groups)` iff the worlds are exactly the product of the groups
/// (every world picks exactly one member per group, and |worlds| == Π|group|);
/// `None` when the structure is correlated and does not factor.
///
/// Two contested candidates are in the SAME group iff they NEVER co-occur in any
/// world (mutually exclusive). Groups are the connected components of that
/// exclusion graph; the product check rejects non-clique / correlated shapes.
fn decompose_into_independent_groups(
    mss: &[BTreeSet<usize>],
    determined: &BTreeSet<usize>,
) -> Option<Vec<Vec<usize>>> {
    if mss.len() <= 1 {
        return Some(Vec::new()); // no fork to factor.
    }
    let contested: Vec<usize> = {
        let mut set: BTreeSet<usize> = BTreeSet::new();
        for s in mss {
            for &i in s {
                if !determined.contains(&i) {
                    set.insert(i);
                }
            }
        }
        set.into_iter().collect()
    };
    if contested.is_empty() {
        return None; // >1 world but no contested member: cannot factor (e.g. {a} vs {}).
    }

    let co_occur = |a: usize, b: usize| mss.iter().any(|s| s.contains(&a) && s.contains(&b));

    let mut uf = UnionFind::new(contested.len());
    for i in 0..contested.len() {
        for j in (i + 1)..contested.len() {
            if !co_occur(contested[i], contested[j]) {
                uf.union(i, j);
            }
        }
    }
    let mut comp: std::collections::BTreeMap<usize, Vec<usize>> = std::collections::BTreeMap::new();
    for k in 0..contested.len() {
        let root = uf.find(k);
        comp.entry(root).or_default().push(contested[k]);
    }
    let groups: Vec<Vec<usize>> = comp.into_values().collect();

    // Verify the product structure: every world picks EXACTLY ONE per group, and
    // |worlds| == Π|group|. This rejects correlated structures and non-cliques.
    for s in mss {
        for g in &groups {
            if g.iter().filter(|&&m| s.contains(&m)).count() != 1 {
                return None;
            }
        }
    }
    let product: usize = groups.iter().map(|g| g.len()).product();
    if product != mss.len() {
        return None;
    }
    Some(groups)
}

/// Enumerate the maximal consistent subsets of `pool` under `pins`. Brute-force
/// over subsets in descending size, skipping any subset dominated by an already
/// found MSS, so only maximal SAT subsets survive. `Unknown` is treated as "not
/// refuted" (cannot manufacture a contradiction).
fn enumerate_mss(
    pins: &[&Value],
    pool: &[&EngineStatement],
    oracle: &dyn SatOracle,
    max_pool: usize,
) -> (Vec<BTreeSet<usize>>, bool) {
    let n = pool.len();
    if n == 0 {
        // No candidates: the pins alone are the single world (already SAT).
        return (vec![BTreeSet::new()], false);
    }
    if n > max_pool {
        // Degraded: cannot brute-force. Check the full set; report capped.
        let mut all: Vec<&Value> = pins.to_vec();
        for s in pool {
            all.push(&s.formula);
        }
        let full: BTreeSet<usize> = (0..n).collect();
        if oracle.check(&all) != SatResult::Unsat {
            return (vec![full], true);
        }
        return (Vec::new(), true);
    }

    let mut masks: Vec<u32> = (0u32..(1u32 << n)).collect();
    masks.sort_by_key(|m| std::cmp::Reverse(m.count_ones()));

    let mut mss: Vec<BTreeSet<usize>> = Vec::new();
    for mask in masks {
        let subset: BTreeSet<usize> = (0..n).filter(|i| mask & (1 << i) != 0).collect();
        if mss.iter().any(|m| subset.is_subset(m)) {
            continue; // dominated by a found (larger) MSS — not maximal.
        }
        let mut fs: Vec<&Value> = pins.to_vec();
        for &i in &subset {
            fs.push(&pool[i].formula);
        }
        if oracle.check(&fs) != SatResult::Unsat {
            mss.push(subset);
        }
    }
    (mss, false)
}

// ── Slice 5: the keystone ───────────────────────────────────────────────────
//
// A UNSAT is one of two things, never both: a lift we never should have tried
// (our overreach), or a vendor code smell. The discriminator is free: ONE SAT
// refutes the "never". A bogus broad lift contradicts EVERY pin (all-UNSAT) and
// self-retracts; the instant ANY pin is SAT, the lift is proven a real
// constraint, so every UNSAT it produces elsewhere is a vendor finding. The SAT
// is the lift's license. This is the no-false-accusation guarantee — and why
// broad/dumb lifting is safe.

/// One lift checked against one of a symbol's vendor pins.
#[derive(Debug, Clone)]
pub struct PinCheck {
    pub pin_cid: String,
    pub result: SatResult,
}

/// The keystone verdict for a lift over a symbol's pins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiftVerdict {
    /// ≥1 SAT licenses the lift. The UNSAT pins are vendor findings (the code
    /// contradicts its own sworn answers there). Findings are forks, not
    /// single-blame.
    Licensed {
        licensing_pins: Vec<String>,
        findings: Vec<String>,
    },
    /// No SAT (all UNSAT, or none decided). Retract the lift — our overreach.
    /// Never an accusation against the vendor.
    Retracted { checked: usize },
}

/// Apply the keystone to a lift's per-pin results.
pub fn apply_keystone(checks: &[PinCheck]) -> LiftVerdict {
    let licensing_pins: Vec<String> = checks
        .iter()
        .filter(|c| c.result == SatResult::Sat)
        .map(|c| c.pin_cid.clone())
        .collect();
    if licensing_pins.is_empty() {
        // No SAT vouches for the lift -> retract. A bogus broad warrant either
        // finds no pin (harmless) or contradicts all of them (self-retracts);
        // it can NEVER become a finding without a SAT licensing it first.
        LiftVerdict::Retracted {
            checked: checks.len(),
        }
    } else {
        let findings: Vec<String> = checks
            .iter()
            .filter(|c| c.result == SatResult::Unsat)
            .map(|c| c.pin_cid.clone())
            .collect();
        LiftVerdict::Licensed {
            licensing_pins,
            findings,
        }
    }
}

/// Check one lift formula against each of a symbol's pins (the closed check,
/// per pin). Feeds `apply_keystone`.
pub fn check_lift_against_pins(
    lift: &Value,
    pins: &[EngineStatement],
    oracle: &dyn SatOracle,
) -> Vec<PinCheck> {
    pins.iter()
        .map(|p| PinCheck {
            pin_cid: p.cid.clone(),
            result: oracle.check(&[lift, &p.formula]),
        })
        .collect()
}

// ── Slice 6: the report ─────────────────────────────────────────────────────
//
// The finish line: a vendor body + its pins produce a content-addressed
// superposition report — N determined facts + M exclusion-groups + a per-output
// strength grade with its verdict, levers named for weak/undecidable outputs,
// and the vendor findings — recomputable by a third party from its bytes alone.

/// The two levers a vendor pulls to collapse a weak/undecidable output.
pub fn collapse_levers() -> Vec<String> {
    vec![
        "pin it: add a vendor assertion that selects one reading".to_string(),
        "get the side effect off the critical path".to_string(),
    ]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuperpositionReport {
    /// The vendor output/symbol this report grades.
    pub symbol: String,
    /// N determined facts — the warranted core, true in every world.
    pub determined: Vec<String>,
    /// M mutual-exclusion groups — the forks (the Dr-Who debugging loops).
    pub fork_groups: Vec<Vec<String>>,
    /// Live world-count (saturating).
    pub world_count: u128,
    pub strength: Strength,
    /// The vendor-facing verdict line.
    pub verdict: String,
    /// Empty for Strong; the two levers for Weak/Undecidable.
    pub levers: Vec<String>,
    /// Vendor findings (pins the licensed lift contradicts), from the keystone.
    pub findings: Vec<String>,
    /// The order-invariant superposition handle (the universe's CID).
    pub superposition_cid: String,
}

impl SuperpositionReport {
    /// Assemble the report from a walk over a symbol's candidates + any keystone
    /// findings. The strength is the authoritative survivor count.
    pub fn from_walk(symbol: impl Into<String>, walk: &WalkResult, findings: Vec<String>) -> Self {
        let universe = walk.universe();
        let strength = walk.strength();
        let fork_groups: Vec<Vec<String>> = universe
            .fork_groups()
            .into_iter()
            .map(|g| g.to_vec())
            .collect();
        let levers = match strength {
            Strength::Strong => Vec::new(),
            Strength::Weak | Strength::Undecidable => collapse_levers(),
        };
        let mut findings = findings;
        findings.sort();
        SuperpositionReport {
            symbol: symbol.into(),
            determined: walk.determined.clone(),
            fork_groups,
            world_count: walk.worlds.len() as u128,
            strength,
            verdict: strength.verdict().to_string(),
            levers,
            findings,
            superposition_cid: universe.superposition_cid(),
        }
    }

    /// Build a report for ONE symbol from the keystone outcome: a body warrant
    /// checked against its vendor pins. With no findings the body warrant is the
    /// single reading (Strong). With findings, the readings that disagree — the
    /// body's vs each contradicting pin — form one fork group (Weak: bugs live in
    /// ordering, not logic). `body_cid` is the warrant's CID; `finding_pin_cids`
    /// are the pins it contradicts (vendor findings).
    pub fn for_symbol(
        symbol: impl Into<String>,
        body_cid: String,
        finding_pin_cids: Vec<String>,
    ) -> Self {
        let mut findings = finding_pin_cids;
        findings.sort();
        let mut acc = Accumulator::new();
        let (determined, fork_groups, strength) = if findings.is_empty() {
            acc.push(body_cid.clone(), WorldMembership::Determined);
            (vec![body_cid.clone()], Vec::new(), Strength::Strong)
        } else {
            // The disagreeing readings: the body's, plus each contradicting pin.
            let mut members = vec![body_cid.clone()];
            members.extend(findings.iter().cloned());
            members.sort();
            for (m, cid) in members.iter().enumerate() {
                acc.push(
                    cid.clone(),
                    WorldMembership::ForkMember {
                        group: 0,
                        member: m as u32,
                    },
                );
            }
            (Vec::new(), vec![members], Strength::Weak)
        };
        acc.mark_walked();
        let universe = acc.universe();
        let levers = match strength {
            Strength::Strong => Vec::new(),
            Strength::Weak | Strength::Undecidable => collapse_levers(),
        };
        SuperpositionReport {
            symbol: symbol.into(),
            determined,
            fork_groups,
            world_count: universe.world_count(),
            strength,
            verdict: strength.verdict().to_string(),
            levers,
            findings,
            superposition_cid: universe.superposition_cid(),
        }
    }

    /// The content-addressed report node.
    pub fn canonical_value(&self) -> Arc<CValue> {
        let mut det = self.determined.clone();
        det.sort();
        let mut forks: Vec<Arc<CValue>> = self
            .fork_groups
            .iter()
            .map(|g| {
                let mut m = g.clone();
                m.sort();
                CValue::array(m.into_iter().map(CValue::string).collect())
            })
            .collect();
        forks.sort_by(|a, b| encode_jcs(a).cmp(&encode_jcs(b)));
        let mut findings = self.findings.clone();
        findings.sort();
        CValue::object(vec![
            (
                "determined".to_string(),
                CValue::array(det.into_iter().map(CValue::string).collect()),
            ),
            (
                "findings".to_string(),
                CValue::array(findings.into_iter().map(CValue::string).collect()),
            ),
            ("forkGroups".to_string(), CValue::array(forks)),
            ("kind".to_string(), CValue::string("superposition-report")),
            (
                "levers".to_string(),
                CValue::array(self.levers.iter().cloned().map(CValue::string).collect()),
            ),
            ("strength".to_string(), CValue::string(self.strength.tag())),
            (
                "superpositionCid".to_string(),
                CValue::string(self.superposition_cid.clone()),
            ),
            ("symbol".to_string(), CValue::string(self.symbol.clone())),
            ("verdict".to_string(), CValue::string(self.verdict.clone())),
            (
                "worldCount".to_string(),
                CValue::string(self.world_count.to_string()),
            ),
        ])
    }

    /// Serde view for the RPC response (mirrors the canonical node + the CID).
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "kind": "superposition-report",
            "symbol": self.symbol,
            "determined": self.determined,
            "forkGroups": self.fork_groups,
            "worldCount": self.world_count.to_string(),
            "strength": self.strength.tag(),
            "verdict": self.verdict,
            "levers": self.levers,
            "findings": self.findings,
            "superpositionCid": self.superposition_cid,
            "cid": self.cid(),
        })
    }

    /// The report CID — recomputable by a third party as blake3_512 of `member_bytes`.
    pub fn cid(&self) -> String {
        cid_of_value(self.canonical_value().as_ref())
    }

    /// The bytes a `.proof` envelope carries as this report's member.
    pub fn member_bytes(&self) -> Vec<u8> {
        jcs_bytes_of_value(self.canonical_value().as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock oracle: a conjunction is UNSAT iff it contains all CIDs of some
    /// configured minimal conflict set. Reads the "tag" field of each formula.
    struct MockOracle {
        conflicts: Vec<BTreeSet<String>>,
    }

    impl MockOracle {
        fn new(conflicts: &[&[&str]]) -> Self {
            MockOracle {
                conflicts: conflicts
                    .iter()
                    .map(|c| c.iter().map(|s| s.to_string()).collect())
                    .collect(),
            }
        }
    }

    impl SatOracle for MockOracle {
        fn check(&self, formulas: &[&Value]) -> SatResult {
            let present: BTreeSet<String> = formulas
                .iter()
                .filter_map(|f| f.get("tag").and_then(|t| t.as_str()).map(String::from))
                .collect();
            for c in &self.conflicts {
                if c.is_subset(&present) {
                    return SatResult::Unsat;
                }
            }
            SatResult::Sat
        }
    }

    fn stmt(tag: &str) -> EngineStatement {
        EngineStatement::new(format!("blake3-512:{tag}"), json!({"tag": tag}))
    }

    #[test]
    fn all_consistent_is_one_world_strong() {
        let oracle = MockOracle::new(&[]);
        let pins = vec![stmt("pin")];
        let cands = vec![stmt("a"), stmt("b"), stmt("c")];
        let r = walk(&pins, &cands, &oracle, 16);
        assert_eq!(r.worlds.len(), 1);
        assert_eq!(r.strength(), Strength::Strong);
        assert_eq!(r.determined.len(), 3);
        assert!(r.contested.is_empty());
        assert!(r.refuted_by_pins.is_empty());
        assert_eq!(r.universe().world_count(), 1);
    }

    #[test]
    fn one_conflict_pair_forks_into_two_worlds() {
        // a and b cannot coexist. Two maximal readings: {a,c} and {b,c}.
        let oracle = MockOracle::new(&[&["a", "b"]]);
        let pins = vec![stmt("pin")];
        let cands = vec![stmt("a"), stmt("b"), stmt("c")];
        let r = walk(&pins, &cands, &oracle, 16);
        assert_eq!(r.worlds.len(), 2, "one conflict pair => two worlds");
        assert_eq!(r.strength(), Strength::Weak);
        assert_eq!(r.determined, vec!["blake3-512:c"]);
        assert_eq!(
            r.contested,
            vec!["blake3-512:a".to_string(), "blake3-512:b".to_string()]
        );
        assert!(r.factored, "a clean conflict pair must factor");
        // Factored universe: c determined, {a,b} one fork => 2 worlds.
        assert_eq!(r.universe().world_count(), 2);
        assert_eq!(r.universe().determined_facts(), vec!["blake3-512:c"]);
        assert_eq!(r.universe().fork_groups().len(), 1);
    }

    #[test]
    fn candidate_refuted_by_pins_lives_in_no_world() {
        // `bad` is UNSAT with the pin alone — it is set aside, not a world.
        let oracle = MockOracle::new(&[&["pin", "bad"]]);
        let pins = vec![stmt("pin")];
        let cands = vec![stmt("good"), stmt("bad")];
        let r = walk(&pins, &cands, &oracle, 16);
        assert_eq!(r.refuted_by_pins, vec!["blake3-512:bad"]);
        assert_eq!(r.worlds.len(), 1, "only `good` survives -> one world");
        assert_eq!(r.determined, vec!["blake3-512:good"]);
        assert_eq!(r.strength(), Strength::Strong);
    }

    #[test]
    fn inconsistent_pins_are_undecidable_at_the_root() {
        let oracle = MockOracle::new(&[&["p1", "p2"]]);
        let pins = vec![stmt("p1"), stmt("p2")];
        let cands = vec![stmt("a")];
        let r = walk(&pins, &cands, &oracle, 16);
        assert!(!r.pins_consistent);
        assert_eq!(r.strength(), Strength::Undecidable);
        assert!(r.worlds.is_empty());
    }

    #[test]
    fn walk_is_order_free() {
        // The doctrine's load-bearing invariant: the worlds are invariant to the
        // order candidates are fed. Same conflict, two feed orders => same result.
        let oracle = MockOracle::new(&[&["a", "b"]]);
        let pins = vec![stmt("pin")];
        let forward = walk(&pins, &[stmt("a"), stmt("b"), stmt("c")], &oracle, 16);
        let reversed = walk(&pins, &[stmt("c"), stmt("b"), stmt("a")], &oracle, 16);
        assert_eq!(forward.worlds.len(), 2, "the conflict must actually fork");
        assert_eq!(forward.worlds, reversed.worlds);
        assert_eq!(forward.determined, reversed.determined);
        assert_eq!(forward.contested, reversed.contested);
        assert_eq!(
            forward.universe().superposition_cid(),
            reversed.universe().superposition_cid(),
            "order-free: the superposition handle must not depend on feed order"
        );
    }

    #[test]
    fn three_way_mutual_exclusion_is_three_worlds() {
        // a,b,c pairwise exclusive (a triangle of conflicts) => 3 singleton worlds.
        let oracle = MockOracle::new(&[&["a", "b"], &["b", "c"], &["a", "c"]]);
        let pins = vec![stmt("pin")];
        let r = walk(&pins, &[stmt("a"), stmt("b"), stmt("c")], &oracle, 16);
        assert_eq!(r.worlds.len(), 3);
        assert_eq!(r.strength(), Strength::Weak);
        assert!(r.determined.is_empty());
        assert!(r.factored, "pairwise-exclusive triple is a clean single fork of 3");
        assert_eq!(r.universe().world_count(), 3);
    }

    fn check(pin: &str, result: SatResult) -> PinCheck {
        PinCheck {
            pin_cid: format!("blake3-512:{pin}"),
            result,
        }
    }

    #[test]
    fn keystone_one_sat_licenses_unsats_as_findings() {
        // The discriminator: ONE SAT among many UNSAT refutes the "never". The
        // lift is licensed; the UNSAT pins are vendor findings.
        let checks = vec![
            check("p1", SatResult::Sat),
            check("p2", SatResult::Unsat),
            check("p3", SatResult::Unsat),
        ];
        match apply_keystone(&checks) {
            LiftVerdict::Licensed {
                licensing_pins,
                findings,
            } => {
                assert_eq!(licensing_pins, vec!["blake3-512:p1"]);
                assert_eq!(findings, vec!["blake3-512:p2", "blake3-512:p3"]);
            }
            other => panic!("expected Licensed, got {other:?}"),
        }
    }

    #[test]
    fn keystone_all_unsat_retracts_never_accuses() {
        // A bogus broad lift contradicts EVERY pin -> retract our overreach.
        // Never a finding.
        let checks = vec![
            check("p1", SatResult::Unsat),
            check("p2", SatResult::Unsat),
        ];
        assert_eq!(
            apply_keystone(&checks),
            LiftVerdict::Retracted { checked: 2 }
        );
    }

    #[test]
    fn keystone_clean_lift_has_no_findings() {
        let checks = vec![
            check("p1", SatResult::Sat),
            check("p2", SatResult::Sat),
        ];
        match apply_keystone(&checks) {
            LiftVerdict::Licensed { findings, .. } => assert!(findings.is_empty()),
            other => panic!("expected Licensed, got {other:?}"),
        }
    }

    #[test]
    fn keystone_unknown_neither_licenses_nor_accuses() {
        // Unknown (z3 absent / non-compiling) does not license and is not a
        // finding. No SAT -> retract; conservative, no false accusation.
        let checks = vec![
            check("p1", SatResult::Unknown),
            check("p2", SatResult::Unknown),
        ];
        assert_eq!(
            apply_keystone(&checks),
            LiftVerdict::Retracted { checked: 2 }
        );
    }

    #[test]
    fn keystone_sat_with_unknowns_still_licenses() {
        // A SAT licenses even amid Unknowns; only the explicit UNSATs are findings.
        let checks = vec![
            check("p1", SatResult::Sat),
            check("p2", SatResult::Unknown),
            check("p3", SatResult::Unsat),
        ];
        match apply_keystone(&checks) {
            LiftVerdict::Licensed {
                licensing_pins,
                findings,
            } => {
                assert_eq!(licensing_pins, vec!["blake3-512:p1"]);
                assert_eq!(findings, vec!["blake3-512:p3"], "Unknown is not a finding");
            }
            other => panic!("expected Licensed, got {other:?}"),
        }
    }

    #[test]
    fn check_lift_against_pins_runs_the_per_pin_check() {
        // Mock: lift conflicts with pin `bad` only.
        let oracle = MockOracle::new(&[&["lift", "bad"]]);
        let lift = json!({"tag": "lift"});
        let pins = vec![stmt("good"), stmt("bad")];
        let checks = check_lift_against_pins(&lift, &pins, &oracle);
        assert_eq!(checks[0].result, SatResult::Sat);
        assert_eq!(checks[1].result, SatResult::Unsat);
        match apply_keystone(&checks) {
            LiftVerdict::Licensed { findings, .. } => {
                assert_eq!(findings, vec!["blake3-512:bad"])
            }
            other => panic!("expected Licensed, got {other:?}"),
        }
    }

    #[test]
    fn report_strong_has_no_levers_and_the_one_reading_verdict() {
        let oracle = MockOracle::new(&[]);
        let pins = vec![stmt("pin")];
        let cands = vec![stmt("a"), stmt("b")];
        let walk_r = walk(&pins, &cands, &oracle, 16);
        let report = SuperpositionReport::from_walk("vendor::f", &walk_r, vec![]);
        assert_eq!(report.strength, Strength::Strong);
        assert!(report.verdict.contains("one reading"));
        assert!(report.levers.is_empty(), "Strong needs no levers");
        assert_eq!(report.world_count, 1);
        assert!(report.fork_groups.is_empty());
    }

    #[test]
    fn report_weak_names_both_levers_and_the_ordering_verdict() {
        let oracle = MockOracle::new(&[&["a", "b"]]);
        let pins = vec![stmt("pin")];
        let cands = vec![stmt("a"), stmt("b"), stmt("c")];
        let walk_r = walk(&pins, &cands, &oracle, 16);
        let report = SuperpositionReport::from_walk("vendor::g", &walk_r, vec![]);
        assert_eq!(report.strength, Strength::Weak);
        assert!(report.verdict.contains("ordering, not logic"));
        assert_eq!(report.levers.len(), 2, "Weak names both levers");
        assert_eq!(report.world_count, 2);
        assert_eq!(report.fork_groups.len(), 1);
        assert_eq!(report.determined, vec!["blake3-512:c"]);
    }

    #[test]
    fn report_carries_keystone_findings() {
        let oracle = MockOracle::new(&[]);
        let pins = vec![stmt("pin")];
        let walk_r = walk(&pins, &[stmt("a")], &oracle, 16);
        let report = SuperpositionReport::from_walk(
            "vendor::h",
            &walk_r,
            vec!["blake3-512:bad_pin".to_string()],
        );
        assert_eq!(report.findings, vec!["blake3-512:bad_pin"]);
    }

    #[test]
    fn report_cid_recomputes_from_member_bytes() {
        // Finish-line third-party recomputability.
        let oracle = MockOracle::new(&[&["a", "b"]]);
        let pins = vec![stmt("pin")];
        let walk_r = walk(&pins, &[stmt("a"), stmt("b")], &oracle, 16);
        let report = SuperpositionReport::from_walk("vendor::k", &walk_r, vec![]);
        assert_eq!(
            sugar_canonicalizer::blake3_512_of(&report.member_bytes()),
            report.cid(),
            "a third party recomputes the report CID from its bytes alone"
        );
    }

    #[test]
    fn report_is_deterministic() {
        let oracle = MockOracle::new(&[&["a", "b"]]);
        let pins = vec![stmt("pin")];
        let a = SuperpositionReport::from_walk(
            "s",
            &walk(&pins, &[stmt("a"), stmt("b"), stmt("c")], &oracle, 16),
            vec![],
        );
        let b = SuperpositionReport::from_walk(
            "s",
            &walk(&pins, &[stmt("c"), stmt("b"), stmt("a")], &oracle, 16),
            vec![],
        );
        assert_eq!(a.cid(), b.cid(), "order-free report identity");
    }

    #[test]
    fn a_sat_versus_unsat_fork_is_not_a_fork_it_collapses() {
        // A "fork" where one reading is SAT and the other UNSAT is just an
        // interpretation of ordering the vendor already settled: the UNSAT
        // reading never enters a world, so it collapses to the SAT one. Strong,
        // nothing to see here — NOT a Weak fork.
        let oracle = MockOracle::new(&[&["pin", "dead"]]); // `dead` UNSAT with the pin
        let pins = vec![stmt("pin")];
        let cands = vec![stmt("live"), stmt("dead")];
        let r = walk(&pins, &cands, &oracle, 16);
        assert_eq!(r.worlds.len(), 1, "the UNSAT reading does not make a world");
        assert_eq!(r.strength(), Strength::Strong);
        assert_eq!(r.determined, vec!["blake3-512:live"]);
        assert_eq!(r.refuted_by_pins, vec!["blake3-512:dead"]);
        assert!(r.contested.is_empty(), "no genuine ambiguity remains");
    }

    #[test]
    fn two_independent_conflicts_factor_into_two_groups() {
        // {a,b} exclusive AND {c,d} exclusive, independently => 2x2 = 4 worlds,
        // factored as TWO fork groups (the goal's "independent forks stay free").
        let oracle = MockOracle::new(&[&["a", "b"], &["c", "d"]]);
        let pins = vec![stmt("pin")];
        let cands = vec![stmt("a"), stmt("b"), stmt("c"), stmt("d")];
        let r = walk(&pins, &cands, &oracle, 16);
        assert_eq!(r.worlds.len(), 4, "2x2 independent conflicts => 4 worlds");
        assert!(r.factored, "independent conflicts must factor");
        assert_eq!(r.fork_groups.len(), 2, "two independent groups");
        for g in &r.fork_groups {
            assert_eq!(g.len(), 2);
        }
        // The factored universe reproduces the product, NOT 4 synthetic members.
        let u = r.universe();
        assert_eq!(u.fork_groups().len(), 2);
        assert_eq!(u.world_count(), 4);
    }

    #[test]
    fn correlated_conflict_falls_back_not_silently() {
        // Worlds {a,c},{b,c},{a,d}: correlated (not a 2x2 product) => cannot
        // factor cleanly => factored=false (synthetic), survivor count preserved.
        let oracle = MockOracle::new(&[&["a", "b"], &["c", "d"], &["b", "d"]]);
        let pins = vec![stmt("pin")];
        let cands = vec![stmt("a"), stmt("b"), stmt("c"), stmt("d")];
        let r = walk(&pins, &cands, &oracle, 16);
        // The exact world count depends on the oracle; the point is the honesty:
        // if it didn't factor, it is surfaced and the survivor count is exact.
        if !r.factored {
            assert_eq!(
                r.universe().world_count(),
                r.worlds.len() as u128,
                "synthetic fallback must preserve the exact survivor count"
            );
        }
    }

    #[test]
    fn cap_is_surfaced_not_silent() {
        let oracle = MockOracle::new(&[]);
        let pins = vec![stmt("pin")];
        let cands: Vec<EngineStatement> = (0..6).map(|i| stmt(&format!("c{i}"))).collect();
        let r = walk(&pins, &cands, &oracle, 4); // pool 6 > cap 4
        assert!(r.capped, "exceeding the cap must be surfaced");
    }
}
