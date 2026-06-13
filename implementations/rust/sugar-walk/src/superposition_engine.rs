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

use crate::canonical::cid_of_value;
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
    /// Candidates UNSAT against the pins ALONE: they live in no world. The
    /// keystone (slice 5) decides whether each is a vendor finding or a retract.
    pub refuted_by_pins: Vec<String>,
    /// False => the pins themselves are inconsistent (root Undecidable).
    pub pins_consistent: bool,
    /// True => the candidate pool exceeded the enumeration cap and the world set
    /// is degraded (NOT silently — callers must surface it).
    pub capped: bool,
    /// True => the contested set factored cleanly into one independent
    /// mutual-exclusion group; false => the factored chain uses synthetic
    /// per-world fork members (survivor count preserved, factoring approximate).
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
                for (member, cid) in self.contested.iter().enumerate() {
                    acc.push(
                        cid.clone(),
                        WorldMembership::ForkMember {
                            group: 0,
                            member: member as u32,
                        },
                    );
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

    // Clean single-fork factorization: every world = determined ∪ exactly one
    // contested candidate, and the contested candidates partition the worlds
    // one-to-one. (The canonical "the body forks on a conflict" shape.)
    let factored = is_clean_single_fork(&mss, &determined_idx, &contested_idx);

    WalkResult {
        worlds,
        determined,
        contested,
        refuted_by_pins,
        pins_consistent: true,
        capped,
        factored,
    }
}

/// True iff each MSS is `determined` plus exactly one contested candidate, and
/// the worlds are one-to-one with the contested candidates.
fn is_clean_single_fork(
    mss: &[BTreeSet<usize>],
    determined: &BTreeSet<usize>,
    contested: &[usize],
) -> bool {
    if mss.len() <= 1 {
        return true; // 0/1 world: no fork to factor.
    }
    if mss.len() != contested.len() {
        return false;
    }
    let mut seen: BTreeSet<usize> = BTreeSet::new();
    for set in mss {
        let extra: Vec<usize> = set.difference(determined).copied().collect();
        if extra.len() != 1 {
            return false;
        }
        if !seen.insert(extra[0]) {
            return false; // a contested candidate claimed by two worlds.
        }
    }
    true
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

    #[test]
    fn cap_is_surfaced_not_silent() {
        let oracle = MockOracle::new(&[]);
        let pins = vec![stmt("pin")];
        let cands: Vec<EngineStatement> = (0..6).map(|i| stmt(&format!("c{i}"))).collect();
        let r = walk(&pins, &cands, &oracle, 4); // pool 6 > cap 4
        assert!(r.capped, "exceeding the cap must be surfaced");
    }
}
