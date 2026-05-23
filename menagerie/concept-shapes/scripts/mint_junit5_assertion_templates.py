#!/usr/bin/env python3
"""
mint_junit5_assertion_templates.py -- mint JUnit5 assertion-template mementos
mapping ProvekIt predicate concepts to native JUnit5 assertion calls.

Each entry is BIDIRECTIONAL by design (one table serves both directions):
  - HARVESTER: a JUnit assertion in test source matches harvest_pattern (or
    harvest_pattern_alt), identifying the predicate concept it discharges.
  - EMITTER: the predicate concept from the catalog looks up its template,
    substitutes formal args into emit_template, and emits the JUnit call.

Filename convention: <concept-name>->junit5:<assertion>.<cid>.json
(matches the realization filename convention with the junit5 framework prefix
in place of the target-language prefix.)

Seed batch (PR-5, issue #1401):
  - concept:option-is-some -> assertNotNull
  - concept:eq             -> assertEquals
  - concept:ne             -> assertNotEquals
  - concept:lt             -> assertTrue  ({a} < {b})
  - concept:gt             -> assertTrue  ({a} > {b})
  - concept:le             -> assertTrue  ({a} <= {b})
  - concept:ge             -> assertTrue  ({a} >= {b})

Skipped (predicate concept does not yet exist as a hub; follow-up PRs):
  - concept:option-is-none, concept:list-empty, concept:list-nonempty
  - concept:throws / concept:fallible-err (concept:throw exists but models
    the throw statement, not the predicate "body throws X"; mapping these
    would make the harvester misidentify any throw statement as an assertion)
  - concept:bool-true, concept:bool-false

This script is idempotent: running it twice produces byte-identical files.
"""
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

BASE = Path(__file__).resolve().parents[1]
CATALOG = BASE / "catalog"
TEMPLATES_DIR = CATALOG / "assertion-templates"
CID_FILE = BASE / "cids.tsv"
BINARY = Path("/Users/tsavo/provekit/implementations/rust/target/debug/compute_fixture_cid")
if not BINARY.exists():
    sys.exit("compute_fixture_cid binary not found; build with: cargo build -p provekit-canonicalizer --bin compute_fixture_cid")

DEFERRED_RECEIPT = "deferred:pending-pk-1401"
UNSIGNED_SIG = {
    "alg": "ed25519",
    "key_id": "UNSIGNED_DEV_ONLY",
    "sig_b64": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
}


def compute_cid(memento):
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
        json.dump(memento, f, ensure_ascii=True)
        f.write("\n")
        tmp = f.name
    try:
        result = subprocess.run([str(BINARY), tmp], capture_output=True, text=True)
        if result.returncode != 0:
            raise SystemExit(f"compute_fixture_cid failed: {result.stderr}")
        return result.stdout.strip()
    finally:
        os.unlink(tmp)


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(value, f, indent=2, ensure_ascii=True)
        f.write("\n")


def catalog_entry(memento):
    cid = compute_cid(memento)
    return {"memento": memento, "cid": cid, "signature": UNSIGNED_SIG}, cid


def ctor(name, args=None):
    return {"kind": "ctor", "name": name, "args": args or []}


def make_template(predicate_concept, assertion, formals, formal_sorts,
                  emit_template, harvest_pattern, harvest_pattern_alt, loss_record):
    fn_name = f"{predicate_concept}->junit5:{assertion}"
    return {
        "kind": "assertion-template",
        "fn_name": fn_name,
        "predicate_concept": predicate_concept,
        "target": {"framework": "junit5", "assertion": assertion},
        "formals": formals,
        "formal_sorts": formal_sorts,
        "emit_template": emit_template,
        "harvest_pattern": harvest_pattern,
        "harvest_pattern_alt": harvest_pattern_alt,
        "loss_record": loss_record,
        "discharge_receipt": DEFERRED_RECEIPT,
        "effects": [],
    }


# (predicate_concept, assertion, formals, formal_sorts, emit, harvest, alts, loss)
SPECS = [
    (
        "concept:option-is-some", "assertNotNull",
        ["x"], [ctor("OptionOfA")],
        "assertNotNull({x})",
        "assertNotNull({x})",
        ["assertTrue({x} != null)", "assertFalse({x} == null)"],
        {
            "structural_divergence": "JUnit encodes option-is-some as null-check; nullable reference IS the option encoding in java. Lossy with respect to Option<Unit> sentinel distinctions (None vs Some(Unit) collapse to null vs non-null)."
        },
    ),
    (
        "concept:eq", "assertEquals",
        ["a", "b"], [ctor("A"), ctor("A")],
        "assertEquals({a}, {b})",
        "assertEquals({a}, {b})",
        [],
        {
            "structural_divergence": "JUnit assertEquals dispatches on runtime type: uses Objects.equals for refs, primitive == for boxed primitives unboxed. concept:eq is sort-polymorphic structural equality."
        },
    ),
    (
        "concept:ne", "assertNotEquals",
        ["a", "b"], [ctor("A"), ctor("A")],
        "assertNotEquals({a}, {b})",
        "assertNotEquals({a}, {b})",
        [],
        {
            "structural_divergence": "Same as concept:eq inverted. JUnit assertNotEquals delegates to the same equals contract."
        },
    ),
    (
        "concept:lt", "assertTrue",
        ["a", "b"], [ctor("Comparable"), ctor("Comparable")],
        "assertTrue({a} < {b})",
        "assertTrue({a} < {b})",
        [],
        {
            "structural_divergence": "JUnit has no native assertLessThan; encoded as assertTrue over the < operator. Harvester must inspect the inline operator (<) to disambiguate from gt/le/ge which share the assertTrue assertion name."
        },
    ),
    (
        "concept:gt", "assertTrue",
        ["a", "b"], [ctor("Comparable"), ctor("Comparable")],
        "assertTrue({a} > {b})",
        "assertTrue({a} > {b})",
        [],
        {
            "structural_divergence": "JUnit has no native assertGreaterThan; encoded as assertTrue over the > operator. Harvester must inspect the inline operator (>) to disambiguate."
        },
    ),
    (
        "concept:le", "assertTrue",
        ["a", "b"], [ctor("Comparable"), ctor("Comparable")],
        "assertTrue({a} <= {b})",
        "assertTrue({a} <= {b})",
        [],
        {
            "structural_divergence": "JUnit has no native assertLessOrEqual; encoded as assertTrue over the <= operator. Harvester must inspect the inline operator (<=) to disambiguate."
        },
    ),
    (
        "concept:ge", "assertTrue",
        ["a", "b"], [ctor("Comparable"), ctor("Comparable")],
        "assertTrue({a} >= {b})",
        "assertTrue({a} >= {b})",
        [],
        {
            "structural_divergence": "JUnit has no native assertGreaterOrEqual; encoded as assertTrue over the >= operator. Harvester must inspect the inline operator (>=) to disambiguate."
        },
    ),
]

# Predicate concepts in the original seed list that were intentionally skipped
# because no concept hub exists yet. Documented for follow-up PRs.
SKIPPED = [
    ("concept:option-is-none",  "assertNull",       "no concept:option-is-none hub yet"),
    ("concept:list-empty",      "assertTrue",       "no concept:list-empty hub yet"),
    ("concept:list-nonempty",   "assertFalse",      "no concept:list-nonempty hub yet"),
    ("concept:throws",          "assertThrows",     "no concept:throws / concept:fallible-err hub; concept:throw exists but models the throw statement, not the predicate"),
    ("concept:bool-true",       "assertTrue",       "no concept:bool-true hub yet"),
    ("concept:bool-false",      "assertFalse",      "no concept:bool-false hub yet"),
]


def mint_all():
    TEMPLATES_DIR.mkdir(parents=True, exist_ok=True)

    cid_rows = []

    for spec in SPECS:
        (predicate_concept, assertion, formals, formal_sorts,
         emit_template, harvest_pattern, harvest_pattern_alt, loss_record) = spec

        memento = make_template(
            predicate_concept, assertion, formals, formal_sorts,
            emit_template, harvest_pattern, harvest_pattern_alt, loss_record,
        )
        entry, cid = catalog_entry(memento)
        fn_name = memento["fn_name"]
        path = TEMPLATES_DIR / f"{fn_name}.{cid}.json"
        write_json(path, entry)
        # Use repo-relative path in cids.tsv so the row is worktree-independent
        # (matches scripts/normalize_cids_paths.py contract).
        rel_path = f"menagerie/concept-shapes/catalog/assertion-templates/{fn_name}.{cid}.json"
        print(f"  {fn_name}: {cid[:40]}...")
        cid_rows.append({
            "kind": "assertion-template",
            "name": fn_name,
            "cid": cid,
            "path": rel_path,
        })

    # Append to cids.tsv (idempotent: skip rows already present by (kind, name))
    existing_lines = []
    if CID_FILE.exists():
        existing_lines = CID_FILE.read_text(encoding="utf-8").splitlines()
    if not existing_lines:
        existing_lines = ["kind\tname\tcid\tpath"]
    seen = set()
    for line in existing_lines[1:]:
        parts = line.split("\t")
        if len(parts) >= 2:
            seen.add((parts[0], parts[1]))
    for row in cid_rows:
        key = (row["kind"], row["name"])
        if key not in seen:
            existing_lines.append(f"{row['kind']}\t{row['name']}\t{row['cid']}\t{row['path']}")
            seen.add(key)
    CID_FILE.write_text("\n".join(existing_lines) + "\n", encoding="utf-8")

    print(f"\n[DONE] Minted {len(SPECS)} JUnit5 assertion-template mementos.")
    if SKIPPED:
        print(f"[SKIPPED] {len(SKIPPED)} entries (predicate concept hub not yet minted):")
        for pred, asn, reason in SKIPPED:
            print(f"  - {pred} -> junit5:{asn}: {reason}")


if __name__ == "__main__":
    mint_all()
