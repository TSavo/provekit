#!/usr/bin/env python3
"""
mint_result_bind.py -- mint concept:result-bind monadic bind + C realization.

Purpose: land the monadic-bind primitive for Result<T,E> via macro composition.

Mints:
  (A) ConceptAbstractionMemento for concept:result-bind
  (B) RealizationDesugaringMemento: concept:result-bind -> c (macro-composition)

All CIDs are BLAKE3-512 via compute_fixture_cid.
All discharge_receipts are deferred: "deferred:pending-result-bind-impl"

Loss-record dimensions:
  - structural_divergence: macro_composition_replaces_native_monad_op
    (C has no native monad typeclass; macro enables do-notation shape)
  - domain_narrowing: T_must_be_block_expression_compatible
    (GNU statement-expression extension required; limits to C99-compatible blocks)
  - domain_narrowing: E_must_be_uniform_through_chain
    (error type E is preserved across all bind steps; no polymorphic error chaining)

Monadic-bind laws:
  - bind(Ok(v), f) == f(v)  (left identity)
  - bind(m, Ok)    == m     (right identity)
  - bind(bind(m, f), g) == bind(m, |x| bind(f(x), g))  (associativity)

Dependency: concept:result (being minted in parallel by feat/cell-result-c).
If feat/cell-result-c has NOT merged, this PR depends on it and requires rebase.
"""
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

BASE = Path(__file__).resolve().parent
CATALOG_REAL = BASE / "catalog"
ABST_DIR = CATALOG_REAL / "abstractions"
REAL_DIR = CATALOG_REAL / "realizations"
CID_FILE = BASE / "cids.tsv"

ROOT = BASE.parent
if not ROOT.is_dir():
    ROOT = Path("/Users/tsavo/provekit")

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

DEFERRED_RECEIPT = "deferred:pending-result-bind-impl"
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

def true_formula():
    return {"kind": "atomic", "name": "true", "args": []}


def var(name):
    return {"kind": "var", "name": name}


def op(name, args):
    return {"kind": "op", "name": name, "args": args}


def skolem(predicate, args):
    return {"kind": "skolem", "predicate": predicate, "args": args}


def ctor(name):
    return {"kind": "ctor", "name": name, "args": []}


# ---------------------------------------------------------------------------
# (A) ConceptAbstractionMemento: concept:result-bind
# ---------------------------------------------------------------------------

def build_abstraction_result_bind():
    """
    concept:result-bind<T,E,U> is the monadic bind operation for Result.

    Signature: (Result<T,E>, T -> Result<U,E>) -> Result<U,E>

    Contract: result_bind_monad(input, f, output) means:
      - If input is Ok(v), then output == f(v)
      - If input is Err(e), then output == Err(e)
      - output has the same error type E as input (no polymorphic error coercion)

    The bind operation forms a monad with Ok as return and bind as >>= in Haskell.
    Error short-circuits: Err(e) propagates through the chain unchanged.

    Slots:
      - input: the input Result<T,E>
      - transform: the function T -> Result<U,E>
      - output: the result Result<U,E>

    formal_sorts: [T, E, U]
    result_sort: ResultOfUE
    """
    return {
        "kind": "concept-abstraction",
        "operator": "concept:result-bind",
        "tier": "abstraction",
        "slots": [
            {"name": "input"},
            {"name": "transform"},
            {"name": "output"},
        ],
        "formal_sorts": [
            "T",
            "E",
            "U",
        ],
        "result_sort": "ResultOfUE",
        "contract": {
            "kind": "wp-rule",
            "formals": ["input", "transform", "output"],
            "body": skolem(
                "result_bind_monad",
                [
                    {"kind": "var", "name": "input"},
                    {"kind": "var", "name": "transform"},
                    {"kind": "var", "name": "output"},
                ],
            ),
        },
        "contract_note": (
            "result_bind_monad(input, transform, output) holds iff: "
            "(1) if input is Ok(v), then output == transform(v); "
            "(2) if input is Err(e), then output == Err(e); "
            "(3) output is Result<U,E> where E is uniform across all steps. "
            "This characterizes the monadic bind law: error propagation + chained transformation."
        ),
        "realizations": [],
    }


# ---------------------------------------------------------------------------
# (B) RealizationDesugaringMemento: concept:result-bind -> c (macro composition)
# ---------------------------------------------------------------------------

def build_realization_result_bind_c():
    """
    C realization via GNU statement-expression macro.

    The C macro implements bind as a ternary composition:
      #define RESULT_BIND(name, r, var, body) \
        ((r).tag == name##_OK ? ({ T var = (r).v.ok; (body); }) : RESULT_ERR(name, (r).v.err))

    This macro:
      - Extracts the tag from the result r
      - If Ok: destructures into var and evaluates body (which should return Result<U,E>)
      - If Err: returns the error unchanged via RESULT_ERR
      - Uses GNU statement-expression ({ ... }) to allow block-like composition

    Loss-record:
      - structural_divergence: macro_composition_replaces_native_monad_op
        (C has no Haskell-style monad; the macro is the closest composition available)
      - domain_narrowing: T_must_be_block_expression_compatible
        (body must be a valid statement expression; lambdas/closures not directly supported)
      - domain_narrowing: E_must_be_uniform_through_chain
        (error type E is preserved, no heterogeneous error coercion)

    Proof of correctness:
      - Left identity: RESULT_BIND(name, Ok(v), x, f(x)) == f(v)
        (the Ok branch destructures v into x, evaluates f(x))
      - Right identity: RESULT_BIND(name, m, x, Ok(x)) == m
        (if m is Ok(v), we extract v and re-wrap as Ok(v) == m;
         if m is Err(e), we return Err(e) == m)
      - Associativity: RESULT_BIND(name, RESULT_BIND(name, m, x, f(x)), y, g(y))
        == RESULT_BIND(name, m, x, RESULT_BIND(name, f(x), y, g(y)))
        (nesting left-associates; both expand to chained error checks)
    """
    return {
        "kind": "realization-desugaring",
        "operator": "concept:result-bind",
        "target_lang": "c",
        "tier": "realization",
        "realizer_role": "macro-composition",
        "realization_memento": {
            "macro_name": "RESULT_BIND",
            "macro_signature": "(name, r, var, body)",
            "macro_body": "((r).tag == name##_OK ? ({ T var = (r).v.ok; (body); }) : RESULT_ERR(name, (r).v.err))",
            "desugaring_note": (
                "Maps concept:result-bind to C macro composition. "
                "Uses GNU statement-expression for block semantics. "
                "Requires: -std=gnu99 or later; tag-based result representation with .tag, .v.ok, .v.err fields."
            ),
        },
        "loss_record": {
            "structural_divergence": "macro_composition_replaces_native_monad_op",
            "domain_narrowing_1": "T_must_be_block_expression_compatible",
            "domain_narrowing_2": "E_must_be_uniform_through_chain",
            "ub_introduction": "none",
            "effect_divergence": "none",
        },
        "discharge_receipt": DEFERRED_RECEIPT,
        "discharge_note": "Discharge deferred pending concept:result minting (feat/cell-result-c).",
    }


# ---------------------------------------------------------------------------
# Main: mint and write
# ---------------------------------------------------------------------------

def main():
    # Build mementos
    abst = build_abstraction_result_bind()
    real = build_realization_result_bind_c()

    # Compute CIDs
    abst_entry, abst_cid = catalog_entry(abst)
    real_entry, real_cid = catalog_entry(real)

    # Write to catalog
    abst_path = ABST_DIR / f"{abst_cid}.json"
    real_path = REAL_DIR / f"{real_cid}.json"

    write_json(abst_path, abst_entry)
    write_json(real_path, real_entry)

    # Log CIDs
    cids = [
        f"concept:result-bind (abstraction)\t{abst_cid}",
        f"concept:result-bind → c (realization)\t{real_cid}",
    ]
    for line in cids:
        print(line)

    # Write CID file for PR
    with open(CID_FILE, "w") as f:
        f.write("# Result-bind minting CIDs\n")
        f.write("# Commit: feat/cell-result-bind-c\n")
        f.write("# Dependency: feat/cell-result-c (parallel)\n")
        f.write("\n")
        for cid_line in cids:
            f.write(cid_line + "\n")

    print(f"\nMementos written:")
    print(f"  Abstraction: {abst_path}")
    print(f"  Realization: {real_path}")
    print(f"\nCIDs logged to: {CID_FILE}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
