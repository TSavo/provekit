#!/usr/bin/env python3
"""bin-classify.py — sort sugar's OWN unproven residual into bin-1 / bin-2.

The construction-semantics axiom (docs + memory): every undischarged line is in
exactly one of two bins.

  bin-1  constructed from written literals, but the walker doesn't speak that
         constructor YET. This DRAINS — each is one slice of lifter work. The
         single tracking number the goal watches: drive bin-1 -> 0.
  bin-2  never constructed BY THE SOURCE: the value crosses the IO membrane
         (clock, dice, network, allocator, user, mutated state) or the assertion
         quantifies over RUNTIME data (opaque collection contents), or it is
         procedural meta-test scaffolding (a test OF the tooling, not a value
         claim). NAMED, never proved.

This script makes that sort STRUCTURAL and RECOMPUTABLE over the Rust assertion-
lift residual (the coretests_sweep refusal reasons). It is deliberately
CONSERVATIVE: a reason that does not match a justified bin-2 rule falls to bin-1
(presumed drainable) and is listed, so nothing is hidden in bin-2 by default.
A beam that can't miss dark: an UNCLASSIFIED reason is loud, not silent.

Run:  python3 scripts/bin-classify.py
(expects /tmp/sweep-*.json from coretests_sweep, or pass --build to produce them)
"""
from __future__ import annotations
import glob, json, os, re, subprocess, sys, collections

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CRATES = ["libsugar", "sugar-ir-compiler-smt-lib", "sugar-cli", "sugar-verifier", "sugar-walk"]

# ── The bin-2 rule table. Each (pattern, reason) says WHY the residual is the
#    membrane, not a missing constructor. Order: first match wins. Anything that
#    matches NONE of these is bin-1 (presumed drainable) — the honest default.
BIN2_RULES = [
    (re.compile(r"ambiguous temporal identity|temporally stable|mutable container"),
     "mutated receiver / mutable state — not allocated-at-formation (allocation axiom)"),
    (re.compile(r"helper body is not a static assertion reduction"
                r"|reachable only via|reachable only when the method"
                r"|non-#\[test\] item|in impl method|is not structural"
                r"|assert_no_forbidden|assert_panic_locus|assert_single_panic_locus"
                r"|assert_kit_declaration|assert_modes_match|assert_malformed"
                r"|assert_manifest_declared|assert_mapping_absent|assert_no_fn_name"),
     "meta-test scaffolding — a test OF the tooling, not a value construction"),
    # Provenance-named (PROVEN bin-2): for-loops AND iterator quantifiers
    # (.all/.any) now STATE "over an OPAQUE collection" (for_iter_domain) -> runtime
    # data, not constructed from source literals. A "LITERAL" provenance matches NO
    # bin-2 rule -> it falls to bin-1 (drainable), which is correct.
    # ...and a literal-domain loop whose BODY asserts over opaque runtime data is
    # likewise bin-2 (the iterated values are literal, but the asserted values are
    # runtime), now stated as "over OPAQUE runtime data".
    (re.compile(r"over an OPAQUE collection|over OPAQUE runtime data"),
     "loop / iterator quantifier over opaque runtime data — not source-constructed"),
    # Other control-flow contexts (match/if/while/unenumerated) do not yet carry
    # provenance -> still PRESUMED, owing the same check.
    (re.compile(r"under match context|under if context|under while context"
                r"|unenumerated statement position"),
     "control-flow-bound assertion (conditional/while/unenumerated) over runtime iteration (bin-2-presumed)"),
    # Remaining bare-closure refusals (.map/.find/etc.) with no provenance yet.
    (re.compile(r"\|\s*\w+\s*\|"),  # a closure `|x| ...` in the refused term
     "iterator-closure predicate over an opaque collection — vacuous without teeth (bin-2-presumed)"),
]

def classify(reason: str):
    for pat, why in BIN2_RULES:
        if pat.search(reason):
            # "presumed" bin-2 (opaque-collection quantifiers) is held DISTINCT
            # from "proven" bin-2 (mutation / meta-scaffolding): the former still
            # owes a per-row collection-provenance check before it is truly bin-2.
            bucket = "bin-2-presumed" if "presumed" in why else "bin-2-proven"
            return (bucket, why)
    return ("bin-1", "no bin-2 rule matched — presumed a missing constructor (drainable)")

def build_sweeps():
    subprocess.run(
        ["cargo", "build", "--manifest-path", os.path.join(REPO, "implementations/rust/Cargo.toml"),
         "-p", "sugar-lift-rust-tests", "--bin", "coretests_sweep"],
        check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    sweep = os.path.join(REPO, "implementations/rust/target/debug/coretests_sweep")
    for c in CRATES:
        subprocess.run([sweep, os.path.join(REPO, f"implementations/rust/{c}/src"),
                        "--json", f"/tmp/sweep-{c}.json"],
                       check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

def main():
    if "--build" in sys.argv or not glob.glob("/tmp/sweep-*.json"):
        build_sweeps()
    bins = collections.Counter()
    why_counts = collections.Counter()
    bin1_samples = collections.Counter()
    total_refused = 0
    for f in sorted(glob.glob("/tmp/sweep-*.json")):
        d = json.load(open(f))
        for reason in d.get("all_reasons", []):
            total_refused += 1
            b, why = classify(reason)
            bins[b] += 1
            why_counts[(b, why)] += 1
            if b == "bin-1":
                # collapse to a shape key for the burndown worklist
                key = re.sub(r"`[^`]*`", "`…`", reason)[:70]
                bin1_samples[key] += 1
    print("=" * 70)
    print(" bin-1 / bin-2 classification of sugar's Rust assertion-lift residual")
    print("=" * 70)
    print(f" total named-refused: {total_refused}")
    print(f"   bin-1 (constructible, DRAINS):        {bins['bin-1']:4d}   <-- the tracking number")
    print(f"   bin-2 PROVEN (membrane, named):       {bins['bin-2-proven']:4d}")
    print(f"   bin-2 PRESUMED (opaque-coll, pending): {bins['bin-2-presumed']:4d}   owes a provenance check")
    print()
    print(" bin-2 breakdown (why it is the membrane, not a missing constructor):")
    for (b, why), n in sorted(why_counts.items(), key=lambda x: -x[1]):
        if b.startswith("bin-2"):
            print(f"   {n:4d}  [{b}] {why}")
    print()
    print(" bin-1 worklist (presumed-drainable shapes — drive these to 0):")
    if not bin1_samples:
        print("   (none — bin-1 = 0 on this axis)")
    for shape, n in bin1_samples.most_common(20):
        print(f"   {n:4d}  {shape}")
    print()
    print(" NOTE: bin-2-presumed rows (control-flow / closure over a collection) are")
    print(" presumed bin-2 because the sweep corpus is over OPAQUE runtime collections;")
    print(" a ∀ over a LITERAL collection would be bin-1 (unrollable). Making the lifter")
    print(" emit collection-provenance in the refusal is the next rigor step.")

if __name__ == "__main__":
    main()
