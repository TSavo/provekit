#!/usr/bin/env python3
"""
mint_assert_java.py -- mint concept:assert -> java realization (N edge).

Pattern mirrors mint_result_java.py (PR #694).
This is N-edge-only: the concept:assert abstraction is already on main (PR #685).

Mints:
  (A) RealizationDesugaringMemento: concept:assert -> java:assert-statement  (the N edge)

The Java realization of concept:assert uses the built-in assert statement:
  assert pred : "message";

Java assertions are disabled by default at runtime. They must be enabled
with -ea (enableassertions) flag. This is a key domain_narrowing constraint.

All CIDs are BLAKE3-512 via compute_fixture_cid.
discharge_receipt is null (PR1 form, N-edge-only).

Loss record (5-dimension):
  structural_divergence: assert is a statement, not a method call; no custom handler
  domain_narrowing: Java assertions disabled by default; -ea required at runtime
  ub_introduction: none; AssertionError is a structured throwable
  effect_divergence: throws AssertionError (catchable) vs abort semantics
  value_divergence: none; predicate maps directly to assert condition
"""
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

BASE = Path(__file__).resolve().parents[1]
CATALOG_REAL = BASE / "catalog"
REAL_DIR = CATALOG_REAL / "realizations"
CID_FILE = BASE / "cids.tsv"

ROOT = BASE.parents[1]
RUST_DIR = ROOT / "implementations" / "rust"

BINARY_CANDIDATES = [
    RUST_DIR / "target" / "debug" / "compute_fixture_cid",
    Path("/Users/tsavo/provekit/implementations/rust/target/debug/compute_fixture_cid"),
]

BINARY = None
for candidate in BINARY_CANDIDATES:
    if candidate.exists():
        BINARY = candidate
        break

if BINARY is None:
    sys.exit(
        "compute_fixture_cid binary not found; "
        "run: cargo build --manifest-path implementations/rust/Cargo.toml "
        "-p provekit-canonicalizer"
    )

UNSIGNED_SIG = {
    "alg": "ed25519",
    "key_id": "UNSIGNED_DEV_ONLY",
    "sig_b64": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
}


# ---------------------------------------------------------------------------
# CID utilities
# ---------------------------------------------------------------------------

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


# ---------------------------------------------------------------------------
# IR formula helpers
# ---------------------------------------------------------------------------

def var(name):
    return {"kind": "var", "name": name}


def op(name, args):
    return {"kind": "op", "name": name, "args": args}


def ctor(name):
    return {"kind": "ctor", "name": name, "args": []}


# ---------------------------------------------------------------------------
# (A) RealizationDesugaringMemento: concept:assert -> java:assert-statement
# ---------------------------------------------------------------------------

def build_realization_assert_java():
    """
    N edge: concept:assert -> java:assert-statement.

    Java realization: the built-in assert statement.
      assert pred : "assertion failed";

    Java assert statements are a native language feature (since Java 1.4)
    but are disabled by default. They must be enabled with -ea at JVM startup.
    In production environments, assertions are often left disabled.

    Loss record (5-dimension):

    structural_divergence:
      assert is a statement syntactic form, not a method call; it cannot be
      stored in a variable or called as a first-class value; the assert keyword
      takes a boolean expression directly; no custom handler hook is available
      (short of a SecurityManager or custom ClassLoader); syntax is
      "assert expr : detail" not a function call.

    domain_narrowing:
      Java assertions are disabled by default at JVM startup; -ea (or
      -enableassertions) is required to activate them; in production
      deployments, assertions are typically not enabled; the realization
      domain is narrowed to JVM invocations with -ea flag set; programs
      running without -ea silently skip the held(p) contract check.

    ub_introduction:
      none: Java has no undefined behavior; the assert statement throws
      AssertionError (extends Error, not RuntimeException) which is a
      structured throwable; no memory corruption or undefined state.

    effect_divergence:
      Java assert throws AssertionError (a throwable, catchable by
      "catch (AssertionError e)") vs the concept:assert held(p) contract
      which implies program termination on failure; a caller could catch
      AssertionError and continue execution, violating the postcondition;
      effect diverges when AssertionError is caught.

    value_divergence:
      none: the predicate slot maps directly to the assert boolean expression;
      no value-representation gap between concept:assert(pred) and
      "assert pred : msg".
    """
    return {
        "kind": "equation",
        "fn_name": "concept:assert->java:assert-statement",
        "formals": [],
        "formal_sorts": [],
        "post": {
            "lhs": op("concept:assert", []),
            "rhs": op(
                "java:assert-statement",
                [
                    {
                        "kind": "const",
                        "value": "assert pred : \"assertion failed\"",
                        "sort": ctor("AssertStmt"),
                    }
                ],
            ),
        },
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": "java",
        "loss_record": {
            "structural_divergence": (
                "assert is a statement syntactic form, not a method call; "
                "cannot be stored in a variable or passed as first-class value; "
                "takes a boolean expression directly; no custom handler hook; "
                "syntax is 'assert expr : detail' not a function call"
            ),
            "domain_narrowing": (
                "Java assertions disabled by default at JVM startup; "
                "-ea (enableassertions) flag required to activate; "
                "in production, assertions are typically not enabled; "
                "realization domain narrowed to JVM invocations with -ea; "
                "programs running without -ea silently skip the held(p) check"
            ),
            "ub_introduction": (
                "none: Java has no undefined behavior; assert throws AssertionError "
                "(extends Error) which is a structured throwable; "
                "no memory corruption or undefined state introduced"
            ),
            "effect_divergence": (
                "Java assert throws AssertionError (catchable) vs concept:assert "
                "held(p) postcondition implying program termination on failure; "
                "a caller catching AssertionError can continue execution, "
                "violating the postcondition; effect diverges when caught"
            ),
            "value_divergence": (
                "none: predicate slot maps directly to assert boolean expression; "
                "no value-representation gap"
            ),
        },
        "discharge_receipt": None,
        "effects": [],
    }


# ---------------------------------------------------------------------------
# Main mint flow
# ---------------------------------------------------------------------------

def append_cids_tsv(rows):
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
    for row in rows:
        key = (row["kind"], row["name"])
        if key not in seen:
            existing_lines.append(f"{row['kind']}\t{row['name']}\t{row['cid']}\t{row['path']}")
            seen.add(key)
    CID_FILE.write_text("\n".join(existing_lines) + "\n", encoding="utf-8")


def mint_all():
    REAL_DIR.mkdir(parents=True, exist_ok=True)

    cid_rows = []

    print("[N] Minting concept:assert->java:assert-statement realization (N edge)...")
    real_memento = build_realization_assert_java()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:assert->java:assert-statement.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:assert->java:assert-statement: {real_cid[:40]}...")
    cid_rows.append(
        {
            "kind": "realization",
            "name": "concept:assert->java:assert-statement",
            "cid": real_cid,
            "path": str(real_path),
        }
    )

    print("\n[STABILITY] Re-minting realization for byte-stability check...")
    check_cid = compute_cid(real_memento)
    if check_cid != real_cid:
        print(f"  UNSTABLE: {check_cid} != {real_cid}")
        raise SystemExit("ABORTING: CID instability detected.")
    print(f"  STABLE: realization: ok")

    append_cids_tsv(cid_rows)

    print(f"\n[DONE] Minted CID:")
    print(f"  realization (N edge) CID: {real_cid}")

    return real_cid


if __name__ == "__main__":
    mint_all()
