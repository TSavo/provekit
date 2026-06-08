//! `sugar diff <BEFORE> <AFTER>`: the behavior diff between two minted proof sets.
//!
//! Everything else in the suite mints proofs. `diff` is the verb that *reads*
//! two of them and reports what changed in terms of meaning, not text.
//!
//! Each proof set lifts to a `{contract-name -> CID}` table (`name_to_cid` in
//! the loaded `MementoPool`). The CID is the name-stripped, content-addressed
//! identity of the contract's pre/post: its *behavior*.
//!
//! The verdict is driven by the CID SET, not the name set, because names are
//! sugar. We invert each table to `CID -> {names}` and classify by behavior:
//!
//!   held      a CID present both sides under the same name(s)
//!   renamed   a CID present both sides, but its name(s) changed (pure rename)
//!   new       a CID only in AFTER  (genuinely new behavior -- additive)
//!   lost      a CID only in BEFORE (behavior actually gone -- breaking)
//!
//! A pure rename has zero new and zero lost behaviors, so it is `bump: none`,
//! exit 0. That is the whole point: rename a contract, churn its implementation,
//! reformat 2700 files -- as long as no behavior-CID appears or disappears, the
//! behavior delta is none and the gate stays green. Only a CID that vanishes
//! (lost) or appears (new) moves the needle.
//!
//! The exit code is the product: nonzero when a behavior is lost. That one fact
//! makes the same binary a CI gate, a pre-publish hook (refuse a dishonest
//! version bump), and an install-time hook (refuse a silent dependency mutation).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use clap::Args;
use sugar_verifier::load_all_proofs;

use crate::{EXIT_OK, EXIT_VERIFY_FAIL};

#[derive(Args, Debug)]
pub struct DiffArgs {
    /// Baseline project root: the "before" proof set.
    pub before: PathBuf,
    /// Comparison project root: the "after" proof set.
    pub after: PathBuf,
}

/// `name -> CID`, as loaded from a proof set.
type Table = BTreeMap<String, String>;
/// `CID -> {names}`: a behavior and every contract name that denotes it.
type ByCid = BTreeMap<String, BTreeSet<String>>;

fn invert(t: &Table) -> ByCid {
    let mut m: ByCid = BTreeMap::new();
    for (name, cid) in t {
        m.entry(cid.clone()).or_default().insert(name.clone());
    }
    m
}

/// The behavior delta between two proof sets, keyed by CID.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub new_behaviors: u32,
    pub lost_behaviors: u32,
    pub held: u32,
    pub renamed: u32,
    pub lines: Vec<String>,
}

impl Summary {
    /// A break is exactly "a behavior that existed no longer does." A rename is
    /// not a break: the behavior is still there under a new name.
    pub fn breaking(&self) -> bool {
        self.lost_behaviors > 0
    }

    /// The honest-semver bump the behavior delta implies.
    pub fn bump(&self) -> &'static str {
        if self.lost_behaviors > 0 {
            "MAJOR"
        } else if self.new_behaviors > 0 {
            "minor"
        } else {
            "none"
        }
    }
}

fn short(cid: &str) -> String {
    // strip the `blake3-512:` prefix if present, keep a recognizable head
    let hex = cid.rsplit(':').next().unwrap_or(cid);
    format!("{}…", &hex[..hex.len().min(12)])
}

fn names(set: &BTreeSet<String>) -> String {
    set.iter().cloned().collect::<Vec<_>>().join(", ")
}

/// Pure comparison: classify every behavior CID across both tables. This is the
/// whole feature; `run` is just IO around it.
pub fn summarize(before: &Table, after: &Table) -> Summary {
    let b = invert(before);
    let a = invert(after);
    let mut s = Summary::default();
    let cids: BTreeSet<&String> = b.keys().chain(a.keys()).collect();
    for cid in cids {
        match (b.get(cid), a.get(cid)) {
            (Some(bn), Some(an)) => {
                // behavior preserved on both sides
                s.held += bn.intersection(an).count() as u32;
                if bn != an {
                    s.renamed += 1;
                    let from: Vec<String> = bn.difference(an).cloned().collect();
                    let to: Vec<String> = an.difference(bn).cloned().collect();
                    s.lines.push(format!(
                        "  renamed    {} -> {}   (behavior {} held)",
                        from.join(", "),
                        to.join(", "),
                        short(cid)
                    ));
                }
            }
            (Some(bn), None) => {
                s.lost_behaviors += 1;
                s.lines
                    .push(format!("  lost       {}   ({})", names(bn), short(cid)));
            }
            (None, Some(an)) => {
                s.new_behaviors += 1;
                s.lines
                    .push(format!("  new        {}   ({})", names(an), short(cid)));
            }
            (None, None) => unreachable!("cid came from the union of both key sets"),
        }
    }
    s
}

pub fn run(args: DiffArgs) -> u8 {
    let before = load_all_proofs::run(&args.before);
    let after = load_all_proofs::run(&args.after);
    let s = summarize(&before.name_to_cid, &after.name_to_cid);

    for line in &s.lines {
        println!("{line}");
    }
    if !s.lines.is_empty() {
        println!();
    }
    println!(
        "behavior: {} new, {} lost, {} held, {} renamed",
        s.new_behaviors, s.lost_behaviors, s.held, s.renamed
    );
    println!("required bump: {}", s.bump());

    if s.breaking() {
        EXIT_VERIFY_FAIL
    } else {
        EXIT_OK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(pairs: &[(&str, &str)]) -> Table {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn identity_holds_all_behaviors_no_bump() {
        let a = table(&[("f", "cid1"), ("g", "cid2")]);
        let s = summarize(&a, &a);
        assert_eq!((s.held, s.new_behaviors, s.lost_behaviors, s.renamed), (2, 0, 0, 0));
        assert!(!s.breaking());
        assert_eq!(s.bump(), "none");
    }

    #[test]
    fn pure_rename_is_renamed_not_breaking() {
        // THE headline: same behavior CID, new contract name. Names are sugar,
        // so this is `renamed`, zero behavior delta, bump none, exit ok.
        let s = summarize(&table(&[("old_name", "cidA")]), &table(&[("new_name", "cidA")]));
        assert_eq!((s.renamed, s.new_behaviors, s.lost_behaviors, s.held), (1, 0, 0, 0));
        assert!(!s.breaking());
        assert_eq!(s.bump(), "none");
    }

    #[test]
    fn behavior_moved_under_stable_name_is_major() {
        // name held, CID replaced: old behavior lost, new behavior gained.
        let s = summarize(&table(&[("f", "cid1")]), &table(&[("f", "cid2")]));
        assert_eq!((s.lost_behaviors, s.new_behaviors), (1, 1));
        assert!(s.breaking());
        assert_eq!(s.bump(), "MAJOR");
    }

    #[test]
    fn added_only_is_minor() {
        let s = summarize(
            &table(&[("f", "cid1")]),
            &table(&[("f", "cid1"), ("g", "cid2")]),
        );
        assert_eq!((s.new_behaviors, s.lost_behaviors, s.held), (1, 0, 1));
        assert!(!s.breaking());
        assert_eq!(s.bump(), "minor");
    }

    #[test]
    fn lost_behavior_is_major_and_breaking() {
        let s = summarize(
            &table(&[("f", "cid1"), ("g", "cid2")]),
            &table(&[("f", "cid1")]),
        );
        assert_eq!((s.lost_behaviors, s.held), (1, 1));
        assert!(s.breaking());
        assert_eq!(s.bump(), "MAJOR");
    }
}
