#!/usr/bin/env python3
"""
mint_result_python.py -- mint concept:result<T,E> + python realization (N edge only).

This script mints the Python realization of concept:result<T,E>.
concept:result abstraction is already on main; we are minting only the N edge.

Pattern mirrors PR #672 (concept:pair-c) and PR #641 (concept:option-c).

Mints:
  (A) RealizationDesugaringMemento: concept:result<T,E> -> python  (the N edge)
  (B) MorphismDischargeReceipt for the realization attempt

The Python realization encodes concept:result<T,E> as a dataclass-based
tagged union with two discriminator cases (Ok, Err), following Python's
idiomatic sum type patterns. This is the closest match to algebraic
sum semantics in Python without external dependencies.

All CIDs are BLAKE3-512 via compute_fixture_cid.
discharge_receipt is null (PR1 form, deferred to integration phase).

Loss-record: 5-dimensional per spec (structural_divergence, domain_narrowing,
ub_introduction, effect_divergence, value_divergence).
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
RECEIPT_DIR = CATALOG_REAL / "receipts"
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

def ctor(name):
    return {"kind": "ctor", "name": name, "args": []}


def var(name):
    return {"kind": "var", "name": name}


def op(name, args):
    return {"kind": "op", "name": name, "args": args}


# ---------------------------------------------------------------------------
# (A) RealizationDesugaringMemento: concept:result<T,E> -> python  (N edge)
# ---------------------------------------------------------------------------

def build_realization_result_python():
    """
    N edge: concept:result<T,E> -> python (dataclass-based tagged union).

    The Python realization encodes concept:result<T,E> as a dataclass with
    two discriminator-based cases:

      @dataclass
      class Result(Generic[T, E]):
          is_ok: bool
          value: Optional[T] = None
          error: Optional[E] = None

    Or equivalently using typing.Union:

      Result[T, E] = Union[Ok[T], Err[E]]

    For simplicity and clarity, we adopt the dataclass-with-bool-discriminator
    idiom, which is idiomatic Python and closest to algebraic semantics.
    This allows straightforward serde and the is_ok() / unwrap() / unwrap_err()
    method family.

    Loss record (concrete, 5-dimensional):

    1. structural_divergence:
      The abstraction is a single ADT term with two disjoint arms (Ok, Err).
      Python has no native sum type; the realization uses a dataclass with
      a boolean discriminator field (is_ok) and optional value/error fields.
      The encoding is indirect: a struct with three fields, not two named
      constructor arms. Pattern matching becomes if res.is_ok / else /
      manual exhaustiveness checking. Instantiation requires explicit
      Ok(v) or Err(e) factory calls (not language primitives).

    2. domain_narrowing:
      Python cannot statically enforce that exactly one of (is_ok, value, error)
      is active. The realization narrows the domain to well-formed programs where:
      - is_ok=True implies value is not None and error is None
      - is_ok=False implies error is not None and value is None
      Programs that violate this invariant (e.g., is_ok=True with value=None)
      are in the narrowed-out domain.

    3. ub_introduction:
      Unlike the C realization, Python does not have undefined behavior from
      reading uninitialized fields. However, type correctness is violated if
      the invariant is broken: mypy will flag accessing res.value when
      is_ok=False as type-incompatible (if using Union-based typing).
      This is a type error, not UB, but represents a domain violation.
      We record this as "no classical UB; type-system violation instead".

    4. effect_divergence:
      None significant. Python dataclass construction is side-effect-free.
      No resource acquisition or disposal patterns differ from the abstraction.

    5. value_divergence:
      None. The slot structure (value, error) maps directly to Python fields.
      No value-representation gap.
    """
    return {
        "kind": "equation",
        "fn_name": "concept:result->python:dataclass-tagged-union",
        "formals": ["value", "error"],
        "formal_sorts": [
            ctor("T"),
            ctor("E"),
        ],
        "post": {
            "lhs": op("concept:result", [var("value"), var("error")]),
            "rhs": op(
                "python:dataclass-tagged-union",
                [
                    op("python:dataclass-union", [
                        {"kind": "const", "value": "is_ok", "sort": ctor("DiscriminatorField")},
                        var("value"),
                        var("error"),
                    ]),
                ],
            ),
        },
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": "python",
        "loss_record": {
            "structural_divergence": (
                "dataclass with bool discriminator replaces native sum type: "
                "three fields (is_ok, value, error) with optional types "
                "replace two named constructor arms; instantiation requires "
                "explicit Ok(v) or Err(e) factory calls; pattern matching "
                "becomes if res.is_ok / else with manual exhaustiveness"
            ),
            "domain_narrowing": (
                "Python does not statically enforce the invariant that exactly "
                "one of (value, error) is active given is_ok; realization domain "
                "is narrowed to programs where the programmer maintains the "
                "invariant at every construction and access site"
            ),
            "ub_introduction": (
                "No classical undefined behavior; however, type checkers "
                "(mypy) will flag accessing res.value when is_ok=False as "
                "type-incompatible; this is a type-system violation rather "
                "than memory UB, but represents a domain violation"
            ),
            "effect_divergence": (
                "empty: dataclass construction is side-effect-free; "
                "no resource patterns differ from the abstraction"
            ),
            "value_divergence": (
                "empty: the slot structure (value, error) maps directly "
                "to Python fields; no value-representation gap"
            ),
        },
        "discharge_receipt": None,
        "effects": [],
    }


# ---------------------------------------------------------------------------
# (B) MorphismDischargeReceipt for the realization attempt
# ---------------------------------------------------------------------------

def build_discharge_receipt_result_python(real_cid):
    """
    MorphismDischargeReceipt: records the discharge attempt for the
    concept:result -> python realization.

    Receipt structure:
    - morphism: the realization CID
    - status: "pending" (PR1 form; null discharge_receipt on memento)
    - notes: brief justification
    """
    return {
        "kind": "morphism-discharge-attempt",
        "morphism_cid": real_cid,
        "attempt_status": "pending",
        "attempt_date": "2026-05-12",
        "notes": (
            "Python realization of concept:result via dataclass-based tagged union. "
            "Loss record fully characterized (5-dimensional). Discharge deferred to "
            "integration phase once Python lifter infrastructure is in place."
        ),
    }


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def append_cid_row(kind, name, cid, path):
    """Append a single row to cids.tsv, avoiding duplicates."""
    existing_lines = []
    seen = set()

    if CID_FILE.exists():
        for line in CID_FILE.read_text(encoding="utf-8").strip().split("\n"):
            if not line.strip():
                continue
            parts = line.split("\t")
            if len(parts) >= 2:
                seen.add((parts[0], parts[1]))
            existing_lines.append(line)

    key = (kind, name)
    if key not in seen:
        existing_lines.append(f"{kind}\t{name}\t{cid}\t{path}")
        seen.add(key)

    CID_FILE.write_text("\n".join(existing_lines) + "\n", encoding="utf-8")


def mint_all():
    REAL_DIR.mkdir(parents=True, exist_ok=True)
    RECEIPT_DIR.mkdir(parents=True, exist_ok=True)

    # Step 1: compute realization CID
    print("[1] Minting concept:result->python:dataclass-tagged-union realization (N edge)...")
    real_memento = build_realization_result_python()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:result->python:dataclass-tagged-union.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:result->python:dataclass-tagged-union: {real_cid[:40]}...")

    # Step 2: mint discharge receipt
    print("[2] Minting MorphismDischargeReceipt...")
    receipt_memento = build_discharge_receipt_result_python(real_cid)
    receipt_cid = compute_cid(receipt_memento)
    receipt_path = RECEIPT_DIR / f"morphism_python_result_attempt.receipt.json"
    write_json(receipt_path, {"memento": receipt_memento, "cid": receipt_cid, "signature": UNSIGNED_SIG})
    print(f"  receipt: {receipt_cid[:40]}...")

    # Step 3: stability check (mint realization a second time)
    print("\n[STABILITY] Re-minting realization for byte-stability check...")
    real_memento_check = build_realization_result_python()
    real_entry_check, real_cid_check = catalog_entry(real_memento_check)
    if real_cid_check != real_cid:
        print(f"ERROR: CID mismatch! First mint: {real_cid}, second mint: {real_cid_check}")
        return False
    print(f"  ✓ Stable: {real_cid[:40]}...")

    # Step 4: append to cids.tsv
    print("\n[4] Updating cids.tsv...")
    append_cid_row("realization", "concept:result->python:dataclass-tagged-union", real_cid, str(real_path))
    append_cid_row("receipt", "morphism_python_result_attempt", receipt_cid, str(receipt_path))
    print("  ✓ cids.tsv updated")

    print("\n✓ All artifacts minted successfully")
    return True


if __name__ == "__main__":
    success = mint_all()
    sys.exit(0 if success else 1)
