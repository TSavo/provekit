// coretests_sweep: measure the delta to stdlib-0.
//
// Walks a corpus of Rust test files (coretests/tests/**), runs the assertion
// lifter over each, and produces a ledger that classifies EVERY assertion
// macro invocation into exactly one of three bins:
//
//   discharged  -- lifted to a FOL atom (one invariant operand per assertion)
//   refused     -- the lifter emitted a named warning (loudly-bounded-lossy)
//   unaccounted -- seen in source but neither lifted nor warned (a SILENT DROP)
//
// 100% on stdlib == unaccounted == 0, with every refusal carrying an honest
// reason. This binary computes that number and the reason histogram (the
// remaining roadmap). It does NOT decide whether a refusal is honest vs a
// missing reduction -- that is an architect judgement made from the histogram.
//
// Usage: coretests_sweep <corpus-dir> [--json <out.json>]

use std::collections::BTreeMap;

use sugar_lift_rust_tests::lift_file;
use syn::visit::{self, Visit};

const ASSERT_MACROS: &[&str] = &[
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
];

/// Counts assertion-macro invocations independently of the lifter, so we can
/// reconcile against the lifter's own accounting and surface silent drops.
#[derive(Default)]
struct AssertCounter {
    total: usize,
}

impl<'ast> Visit<'ast> for AssertCounter {
    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        if let Some(seg) = m.path.segments.last() {
            let name = seg.ident.to_string();
            if ASSERT_MACROS.contains(&name.as_str()) {
                self.total += 1;
            }
        }
        visit::visit_macro(self, m);
    }
}

/// Normalize a per-assertion refusal reason into a bucket key so the histogram
/// groups by failure SHAPE, not by the specific value/name that triggered it.
/// Backtick-quoted spans (the concrete got-value or symbol) are erased.
fn bucket(reason: &str) -> String {
    // Drop backtick-quoted specifics: `b"abc"`, `Foo::bar`, etc.
    let mut cleaned = String::new();
    let mut in_tick = false;
    for c in reason.chars() {
        if c == '`' {
            in_tick = !in_tick;
            continue;
        }
        if !in_tick {
            cleaned.push(c);
        }
    }
    // Drop a trailing "got ..." / "skipped: ..." specific tail.
    let head = cleaned
        .split(", got")
        .next()
        .unwrap_or(&cleaned)
        .split("; skipped:")
        .next()
        .unwrap_or(&cleaned)
        .trim()
        .to_lowercase();
    let head = head.trim_end_matches(|c: char| c == ':' || c.is_whitespace());
    let head = head.trim();
    if head.is_empty() {
        reason.trim().to_lowercase()
    } else {
        // Cap length so near-identical long reasons still merge.
        head.chars().take(72).collect()
    }
}

#[derive(Default)]
struct Totals {
    files: usize,
    parse_ok: usize,
    parse_fail: usize,
    assert_macros: usize,
    test_fns_seen: usize,
    test_fns_lifted: usize,
    discharged: usize,
    refused: usize,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: coretests_sweep <corpus-dir> [--json <out.json>]");
        std::process::exit(2);
    }
    let corpus = &args[1];
    let json_out = args
        .iter()
        .position(|a| a == "--json")
        .and_then(|i| args.get(i + 1))
        .cloned();

    let mut totals = Totals::default();
    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    // Per-file rows: (path, asserts, atoms, warnings, unaccounted, parse_ok)
    let mut rows: Vec<(String, usize, usize, usize, i64, bool)> = Vec::new();

    for entry in walkdir::WalkDir::new(corpus)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        totals.files += 1;
        let rel = path
            .strip_prefix(corpus)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => {
                totals.parse_fail += 1;
                rows.push((rel, 0, 0, 0, 0, false));
                continue;
            }
        };
        let file = match syn::parse_file(&src) {
            Ok(f) => f,
            Err(_) => {
                totals.parse_fail += 1;
                rows.push((rel, 0, 0, 0, 0, false));
                continue;
            }
        };
        totals.parse_ok += 1;

        let mut counter = AssertCounter::default();
        counter.visit_file(&file);

        let out = lift_file(&file, &rel);
        let discharged = out.assertions_lifted;
        let refused = out.assertions_refused;

        totals.assert_macros += counter.total;
        totals.test_fns_seen += out.seen;
        totals.test_fns_lifted += out.lifted;
        totals.discharged += discharged;
        totals.refused += refused;

        for reason in &out.skip_reasons {
            *reasons.entry(bucket(reason)).or_insert(0) += 1;
        }

        // Silent drop: a real assert macro the collector never reached (nested
        // in control flow) -- neither lifted nor refused with a reason.
        let unaccounted = counter.total as i64 - discharged as i64 - refused as i64;
        rows.push((rel, counter.total, discharged, refused, unaccounted, true));
    }

    // Headline reconciliation at macro granularity.
    let unaccounted =
        totals.assert_macros as i64 - totals.discharged as i64 - totals.refused as i64;
    let pct = |n: usize| {
        if totals.assert_macros == 0 {
            0.0
        } else {
            100.0 * n as f64 / totals.assert_macros as f64
        }
    };

    println!("==== coretests sweep: delta to stdlib-0 ====");
    println!("corpus: {}", corpus);
    println!(
        "files: {} (parse_ok {}, parse_fail {})",
        totals.files, totals.parse_ok, totals.parse_fail
    );
    println!("assertion macros seen: {}", totals.assert_macros);
    println!(
        "  discharged (lifted to FOL):  {:>6}  ({:.1}%)",
        totals.discharged,
        pct(totals.discharged)
    );
    println!(
        "  refused (named reason):      {:>6}  ({:.1}%)",
        totals.refused,
        pct(totals.refused)
    );
    println!(
        "  unaccounted (SILENT DROP):   {:>6}  ({:.1}%)   <-- delta target = 0",
        unaccounted,
        pct(unaccounted.max(0) as usize)
    );
    println!(
        "test fns: seen {} / lifted {}",
        totals.test_fns_seen, totals.test_fns_lifted
    );
    println!();
    println!("---- refusal reason histogram (the roadmap) ----");
    let mut reason_vec: Vec<(&String, &usize)> = reasons.iter().collect();
    reason_vec.sort_by(|a, b| b.1.cmp(a.1));
    for (reason, count) in &reason_vec {
        println!("  {:>6}  {}", count, reason);
    }
    println!();
    println!("---- top files by unaccounted (silent drops) ----");
    let mut by_unacc: Vec<&(String, usize, usize, usize, i64, bool)> = rows.iter().collect();
    by_unacc.sort_by(|a, b| b.4.cmp(&a.4));
    for (rel, asserts, discharged, refused, unacc, ok) in by_unacc.iter().take(30) {
        if *unacc <= 0 {
            break;
        }
        println!(
            "  {:>5} silent  ({} asserts, {} discharged, {} refused){}  {}",
            unacc,
            asserts,
            discharged,
            refused,
            if *ok { "" } else { " [parse_fail]" },
            rel
        );
    }

    if let Some(out_path) = json_out {
        let mut obj = serde_json::Map::new();
        obj.insert("corpus".into(), corpus.clone().into());
        obj.insert("files".into(), totals.files.into());
        obj.insert("parse_ok".into(), totals.parse_ok.into());
        obj.insert("parse_fail".into(), totals.parse_fail.into());
        obj.insert("assert_macros".into(), totals.assert_macros.into());
        obj.insert("discharged".into(), totals.discharged.into());
        obj.insert("refused".into(), totals.refused.into());
        obj.insert("unaccounted".into(), unaccounted.into());
        let reason_obj: serde_json::Map<String, serde_json::Value> = reasons
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::from(*v)))
            .collect();
        obj.insert("reasons".into(), serde_json::Value::Object(reason_obj));
        let file_arr: Vec<serde_json::Value> = rows
            .iter()
            .map(|(rel, asserts, discharged, refused, unacc, ok)| {
                let mut m = serde_json::Map::new();
                m.insert("file".into(), rel.clone().into());
                m.insert("asserts".into(), (*asserts).into());
                m.insert("discharged".into(), (*discharged).into());
                m.insert("refused".into(), (*refused).into());
                m.insert("unaccounted".into(), (*unacc).into());
                m.insert("parse_ok".into(), (*ok).into());
                serde_json::Value::Object(m)
            })
            .collect();
        obj.insert("per_file".into(), serde_json::Value::Array(file_arr));
        let json = serde_json::Value::Object(obj);
        if let Err(e) = std::fs::write(&out_path, serde_json::to_string_pretty(&json).unwrap()) {
            eprintln!("failed to write {}: {}", out_path, e);
        } else {
            println!("\nwrote ledger json: {}", out_path);
        }
    }
}
