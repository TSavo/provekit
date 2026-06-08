//! `sugar diff <BEFORE> <AFTER>`: the behavior diff between two minted proof sets.
//!
//! Everything else in the suite mints proofs. `diff` is the verb that *reads*
//! two of them and reports what changed in terms of meaning, not text.
//!
//! Two modes, same comparison:
//!   default     BEFORE and AFTER are project roots holding minted proofs.
//!   --git       BEFORE and AFTER are git revisions; the project's proofs are
//!               extracted from each revision's tree and diffed. This is the
//!               behavioral-VCS hat: "when did this last change what it does."
//!
//! Each proof set lifts to a `{contract-name -> CID}` table (`name_to_cid`). The
//! CID is the name-stripped, content-addressed identity of the contract's
//! pre/post: its *behavior*. The verdict is driven by the CID SET, not the name
//! set, because names are sugar. We invert each table to `CID -> {names}`:
//!
//!   held      a CID present both sides under the same name(s)
//!   renamed   a CID present both sides, name(s) changed (a pure rename)
//!   new       a CID only in AFTER  (genuinely new behavior, additive)
//!   lost      a CID only in BEFORE (behavior actually gone, breaking)
//!
//! Exit nonzero iff a behavior is lost. A pure rename, an implementation rewrite,
//! a reformat of the world: as long as no behavior-CID appears or disappears,
//! the delta is none and the gate stays green. That one exit code makes the same
//! binary a CI gate, a pre-publish hook, and an install-time supply-chain hook.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use clap::Args;
use sugar_verifier::{load_all_proofs, MementoPool};

use crate::{EXIT_OK, EXIT_USER_ERROR, EXIT_VERIFY_FAIL};

#[derive(Args, Debug)]
pub struct DiffArgs {
    /// BEFORE: a project root, or a git revision when --git is set.
    pub before: String,
    /// AFTER: a project root, or a git revision when --git is set.
    pub after: String,
    /// Treat BEFORE and AFTER as git revisions and diff a project's proofs
    /// across history ("when did this last change what it does").
    #[arg(long)]
    pub git: bool,
    /// In --git mode, the project subdirectory within each revision's tree.
    #[arg(long, default_value = ".")]
    pub path: String,
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
    let hex = cid.rsplit(':').next().unwrap_or(cid);
    format!("{}…", &hex[..hex.len().min(12)])
}

fn names(set: &BTreeSet<String>) -> String {
    set.iter().cloned().collect::<Vec<_>>().join(", ")
}

/// Pure comparison: classify every behavior CID across both tables. This is the
/// whole feature; everything else is IO around it.
pub fn summarize(before: &Table, after: &Table) -> Summary {
    let b = invert(before);
    let a = invert(after);
    let mut s = Summary::default();
    let cids: BTreeSet<&String> = b.keys().chain(a.keys()).collect();
    for cid in cids {
        match (b.get(cid), a.get(cid)) {
            (Some(bn), Some(an)) => {
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

fn git_toplevel() -> Result<String, String> {
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        return Err("not in a git repository (--git must run from inside one)".into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// Extract `rev:path` from `repo` into a temp dir via `git archive | tar`, load
/// its proofs, and clean up. No worktree state, no checkout of the live tree.
fn load_git(repo: &str, rev: &str, path: &str, label: &str) -> Result<MementoPool, String> {
    let tmp = std::env::temp_dir().join(format!("sugar-diff-{label}-{}", sanitize(rev)));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| format!("mkdir {}: {e}", tmp.display()))?;

    let treeish = if path == "." || path.is_empty() {
        rev.to_string()
    } else {
        format!("{rev}:{path}")
    };
    let archive = Command::new("git")
        .args(["-C", repo, "archive", "--format=tar", &treeish])
        .output()
        .map_err(|e| format!("git archive: {e}"))?;
    if !archive.status.success() {
        return Err(format!(
            "git archive {treeish}: {}",
            String::from_utf8_lossy(&archive.stderr).trim()
        ));
    }
    let mut tar = Command::new("tar")
        .args(["-x", "-C", &tmp.to_string_lossy()])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("tar: {e}"))?;
    tar.stdin
        .take()
        .expect("tar stdin")
        .write_all(&archive.stdout)
        .map_err(|e| format!("tar stdin: {e}"))?;
    if !tar.wait().map_err(|e| format!("tar wait: {e}"))?.success() {
        return Err(format!("tar extract failed for {treeish}"));
    }

    let pool = load_all_proofs::run(&tmp);
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(pool)
}

pub fn run(args: DiffArgs) -> u8 {
    let (before, after) = if args.git {
        let repo = match git_toplevel() {
            Ok(r) => r,
            Err(e) => {
                eprintln!("error: {e}");
                return EXIT_USER_ERROR;
            }
        };
        let pair = load_git(&repo, &args.before, &args.path, "before")
            .and_then(|b| load_git(&repo, &args.after, &args.path, "after").map(|a| (b, a)));
        match pair {
            Ok(p) => p,
            Err(e) => {
                eprintln!("error: {e}");
                return EXIT_USER_ERROR;
            }
        }
    } else {
        (
            load_all_proofs::run(Path::new(&args.before)),
            load_all_proofs::run(Path::new(&args.after)),
        )
    };

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
        let s = summarize(&table(&[("old_name", "cidA")]), &table(&[("new_name", "cidA")]));
        assert_eq!((s.renamed, s.new_behaviors, s.lost_behaviors, s.held), (1, 0, 0, 0));
        assert!(!s.breaking());
        assert_eq!(s.bump(), "none");
    }

    #[test]
    fn behavior_moved_under_stable_name_is_major() {
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

    // --- git mode: build a throwaway repo from the real committed example
    // proofs, commit two different proof sets, and diff across the two refs. ---

    fn cp_r(src: &Path, dst: &Path) {
        assert!(Command::new("cp")
            .arg("-r")
            .arg(src)
            .arg(dst)
            .status()
            .expect("cp -r")
            .success());
    }

    #[test]
    fn git_diff_across_two_commits_of_real_proofs() {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|p| p.join("examples/numpy-showcase").is_dir())
            .expect("repo root with examples/");
        let set_a = repo_root.join("examples/numpy-showcase");
        let set_b = repo_root.join("examples/numpy-consumer-demo");

        let tmp = std::env::temp_dir().join(format!("sugar-diff-git-it-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let git = |a: &[&str]| {
            Command::new("git")
                .args(["-C", &tmp.to_string_lossy()])
                .args(a)
                .output()
                .unwrap()
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@example.com"]);
        git(&["config", "user.name", "t"]);

        // commit 1: proj = numpy-showcase's proofs
        cp_r(&set_a, &tmp.join("proj"));
        git(&["add", "-Af"]);
        git(&["commit", "-qm", "c1"]);

        // commit 2: proj = numpy-consumer-demo's proofs (a different contract set)
        std::fs::remove_dir_all(tmp.join("proj")).unwrap();
        cp_r(&set_b, &tmp.join("proj"));
        git(&["add", "-Af"]);
        git(&["commit", "-qm", "c2"]);

        let repo = tmp.to_string_lossy().to_string();
        let before = load_git(&repo, "HEAD~1", "proj", "test_before").expect("load before");
        let after = load_git(&repo, "HEAD", "proj", "test_after").expect("load after");
        let s = summarize(&before.name_to_cid, &after.name_to_cid);
        let _ = std::fs::remove_dir_all(&tmp);

        // the two real proof sets denote different behaviors, so the cross-ref
        // diff must show behaviors both appearing and disappearing.
        assert!(
            s.new_behaviors > 0 && s.lost_behaviors > 0,
            "expected a real behavior delta across the two committed proof sets, got {s:?}"
        );
        assert!(s.breaking());
    }
}
