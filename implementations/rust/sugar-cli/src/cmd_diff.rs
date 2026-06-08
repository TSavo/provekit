//! `sugar diff <BEFORE> <AFTER>`: the behavior diff between two minted proof sets.
//!
//! Everything else in the suite mints proofs. `diff` is the verb that *reads*
//! two of them and reports what changed in terms of meaning, not text.
//!
//! Each proof set lifts to a `{contract-name -> CID}` table (`name_to_cid` in
//! the loaded `MementoPool`). The CID is the name-stripped, content-addressed
//! identity of the contract's pre/post: its *behavior*. So we do set arithmetic
//! over the two tables and classify every contract into the trichotomy:
//!
//!   unchanged  same name, same CID   (implementation churned, behavior held)
//!   changed    same name, new CID    (behavior moved: the loud case)
//!   added      name only in AFTER    (new surface, additive)
//!   removed    name only in BEFORE   (surface dropped, breaking)
//!
//! The exit code is the product: nonzero when behavior moved or surface dropped.
//! That one fact makes `diff` a CI gate, a pre-publish hook, and an install-time
//! hook with the same binary. A behavior-preserving refactor under a stable
//! contract name prints `behavior delta: none` and exits 0.
//!
//! (MVP keys by contract name, so it catches behavior-change under a stable
//! name. A pure contract *rename* shows as removed+added of the same CID; a
//! later increment can add the CID-set view that collapses that to `unchanged`.)

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

/// The behavior delta between two `{contract-name -> CID}` tables.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub unchanged: u32,
    pub changed: u32,
    pub added: u32,
    pub removed: u32,
    pub deltas: Vec<String>,
}

impl Summary {
    /// A break is exactly "a published contract's behavior moved or vanished."
    pub fn breaking(&self) -> bool {
        self.changed > 0 || self.removed > 0
    }

    /// The honest-semver bump the delta implies.
    pub fn bump(&self) -> &'static str {
        if self.breaking() {
            "MAJOR"
        } else if self.added > 0 {
            "minor"
        } else {
            "none"
        }
    }
}

/// Pure comparison: classify every contract name across both tables. This is the
/// whole feature; `run` is just IO around it.
pub fn summarize(before: &BTreeMap<String, String>, after: &BTreeMap<String, String>) -> Summary {
    let mut s = Summary::default();
    let names: BTreeSet<&String> = before.keys().chain(after.keys()).collect();
    for name in names {
        match (before.get(name), after.get(name)) {
            (Some(x), Some(y)) if x == y => s.unchanged += 1,
            (Some(x), Some(y)) => {
                s.changed += 1;
                s.deltas
                    .push(format!("  changed    {name}\n             {x}\n          -> {y}"));
            }
            (None, Some(_)) => {
                s.added += 1;
                s.deltas.push(format!("  added      {name}"));
            }
            (Some(_), None) => {
                s.removed += 1;
                s.deltas.push(format!("  removed    {name}"));
            }
            (None, None) => unreachable!("name came from the union of both key sets"),
        }
    }
    s
}

pub fn run(args: DiffArgs) -> u8 {
    let before = load_all_proofs::run(&args.before);
    let after = load_all_proofs::run(&args.after);
    let s = summarize(&before.name_to_cid, &after.name_to_cid);

    for d in &s.deltas {
        println!("{d}");
    }
    if !s.deltas.is_empty() {
        println!();
    }
    println!(
        "behavior delta: {} changed, {} removed, {} added; {} unchanged",
        s.changed, s.removed, s.added, s.unchanged
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

    fn table(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn identity_is_all_unchanged_no_bump() {
        let a = table(&[("f", "cid1"), ("g", "cid2")]);
        let s = summarize(&a, &a);
        assert_eq!((s.unchanged, s.changed, s.added, s.removed), (2, 0, 0, 0));
        assert!(!s.breaking());
        assert_eq!(s.bump(), "none");
    }

    #[test]
    fn same_name_new_cid_is_major_and_breaking() {
        // behavior moved under a stable contract name: the loud case.
        let s = summarize(&table(&[("f", "cid1")]), &table(&[("f", "cid2")]));
        assert_eq!((s.changed, s.unchanged), (1, 0));
        assert!(s.breaking());
        assert_eq!(s.bump(), "MAJOR");
    }

    #[test]
    fn added_only_is_minor_not_breaking() {
        let s = summarize(
            &table(&[("f", "cid1")]),
            &table(&[("f", "cid1"), ("g", "cid2")]),
        );
        assert_eq!((s.added, s.unchanged), (1, 1));
        assert!(!s.breaking());
        assert_eq!(s.bump(), "minor");
    }

    #[test]
    fn removed_contract_is_major_and_breaking() {
        let s = summarize(
            &table(&[("f", "cid1"), ("g", "cid2")]),
            &table(&[("f", "cid1")]),
        );
        assert_eq!((s.removed, s.unchanged), (1, 1));
        assert!(s.breaking());
        assert_eq!(s.bump(), "MAJOR");
    }
}
