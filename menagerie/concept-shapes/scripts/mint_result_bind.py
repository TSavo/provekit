#!/usr/bin/env python3
"""
mint_result_bind.py -- mint concept:result-bind abstraction + C realization.

Mints:
  (A) ConceptAbstractionMemento for concept:result-bind
  (B) RealizationDesugaringMemento: concept:result-bind -> c:result-bind-macro  (N edge)

This cell depends on concept:result -> c (being minted in parallel as PR #668).
The abstraction's realizations list references the realization CID produced here;
the concept:result dependency is documented in the contract_note.

All CIDs are BLAKE3-512 via compute_fixture_cid.
All discharge_receipts are deferred: "deferred:pending-668".

Loss-record shape: BTreeMap<String, IrFormula> wire format (string values per dimension),
matching PR #636 empirical wire format.

Contract: monadic-bind laws --
  bind(Ok(v), f) == f(v)
  bind(Err(e), f) == Err(e)
  bind(bind(m, f), g) == bind(m, x -> bind(f(x), g))

C realization:
  #define RESULT_BIND(name, r, var, body) \\
    ((r).tag == name##_OK ? ({ T var = (r).v.ok; (body); }) : RESULT_ERR(name, (r).v.err))
composes on the result-c macros (concept:result -> c, PR #668).
"""
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

BASE = Path(__file__).resolve().parents[1]
CATALOG_REAL = BASE / "catalog"
ABST_DIR = CATALOG_REAL / "abstractions"
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

# Discharge deferred pending resolution of concept:result -> c (PR #668).
DEFERRED_RECEIPT = "deferred:pending-668"
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
# IR formula helpers (matching discharge.py / mint_trinity.py conventions)
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
    concept:result-bind is the monadic-bind operation over concept:result<T,E>.

    Slots:
      - result: the input Result value (sort ResultOfTE)
      - f: the continuation (sort T -> ResultOfUE)

    Contract: monadic-bind laws (wp-rule over result_bind_inhabitant):
      1. bind(Ok(v), f) == f(v)                                 -- left identity
      2. bind(Err(e), f) == Err(e)                              -- right identity / error propagation
      3. bind(bind(m, f), g) == bind(m, x -> bind(f(x), g))    -- associativity

    result_sort: ResultBindOfTUE -- the output Result after threading f.

    Dependency: concept:result -> c is being minted in parallel as PR #668.
    The abstraction's realizations list is populated with the N-edge CID
    produced by this script; the concept:result hub CID will be referenced
    once PR #668 lands (successor mint required).
    """
    return {
        "kind": "concept-abstraction",
        "operator": "concept:result-bind",
        "tier": "abstraction",
        "slots": [
            {"name": "result"},
            {"name": "f"},
        ],
        "formal_sorts": [
            "T",
            "E",
            "U",
        ],
        "result_sort": "ResultBindOfTUE",
        "contract": {
            "kind": "wp-rule",
            "formals": ["result", "f"],
            "body": skolem(
                "result_bind_inhabitant",
                [
                    {"kind": "var", "name": "self"},
                    {"kind": "var", "name": "T"},
                    {"kind": "var", "name": "E"},
                    {"kind": "var", "name": "U"},
                ],
            ),
        },
        "contract_note": (
            "result_bind_inhabitant(self, T, E, U) holds iff the monadic-bind laws hold: "
            "(1) bind(Ok(v), f) = f(v) for all v : T; "
            "(2) bind(Err(e), f) = Err(e) for all e : E; "
            "(3) bind(bind(m, f), g) = bind(m, x -> bind(f(x), g)) for all m, f, g. "
            "The f slot must produce a Result<U,E>; the error type is preserved across bind. "
            "Depends on concept:result -> c (PR #668) for the underlying Ok/Err constructors."
        ),
        "realizations": [],
    }


# ---------------------------------------------------------------------------
# (B) RealizationMemento: concept:result-bind -> c:result-bind-macro  (N edge)
# ---------------------------------------------------------------------------

def build_realization_result_bind_c():
    """
    N edge: concept:result-bind -> c:result-bind-macro.

    The C realization encodes monadic bind as a statement-expression macro
    composing on the result-c macro family (RESULT_OK / RESULT_ERR from
    concept:result -> c, PR #668):

      #define RESULT_BIND(name, r, var, body) \\
        ((r).tag == name##_OK ? ({ T var = (r).v.ok; (body); }) : RESULT_ERR(name, (r).v.err))

    where:
      - name: the type-name token (used to resolve name##_OK, name##_ERR tags)
      - r: the input Result expression (evaluated once via statement-expression)
      - var: the variable name bound to the Ok value in body
      - body: the continuation expression producing a Result<U,E>

    Monadic laws in the C encoding:
      left_identity:  RESULT_BIND(name, RESULT_OK(name, v), var, f(var)) == f(v)
      right_identity: RESULT_BIND(name, RESULT_ERR(name, e), var, body) == RESULT_ERR(name, e)
      associativity:  RESULT_BIND(name, RESULT_BIND(name, m, x, f(x)), y, g(y))
                      == RESULT_BIND(name, m, x, RESULT_BIND(name, f(x), y, g(y)))

    Loss record (concrete):

    structural_divergence:
      Monadic bind is a first-class function in the abstraction; the C realization
      encodes it as a macro requiring four explicit tokens (name, r, var, body).
      No polymorphic bind function exists: the macro must be instantiated per
      result type via the name token. Statement expressions (GNU extension __extension__)
      are required for the Ok-arm let-binding; this is not portable ISO C.
      The continuation f is inlined as a body expression, not passed as a function pointer.

    domain_narrowing:
      The abstraction permits any f : T -> Result<U,E>; the C realization narrows
      to continuations expressible as a single statement-expression body, ruling out
      multi-statement continuations that require a wrapper function. Recursive or
      higher-order bind chains require manually nested macro invocations.

    ub_introduction:
      Accessing r.v.ok when r.tag == name##_ERR, or r.v.err when r.tag == name##_OK,
      is undefined behaviour in C (union field read without active arm). The macro
      guards via the tag check but only within the single expansion; the abstraction
      statically excludes cross-arm access; the C realization introduces UB if the
      name token is mismatched to the actual result type.

    extension_dependency:
      Statement expressions (({ ... })) are a GCC/Clang extension. The realization
      is not conforming ISO C11; programs targeting MSVC or strict C conformance are
      in the narrowed-out domain.
    """
    return {
        "kind": "equation",
        "fn_name": "concept:result-bind->c:result-bind-macro",
        "formals": ["result", "f"],
        "formal_sorts": [
            ctor("T"),
            ctor("E"),
            ctor("U"),
        ],
        "post": {
            "lhs": op("concept:result-bind", [var("result"), var("f")]),
            "rhs": op(
                "c:result-bind-macro",
                [
                    op("c:macro-expand", [
                        {"kind": "const", "value": "RESULT_BIND(name, r, var, body)", "sort": ctor("MacroName")},
                        var("result"),
                        var("f"),
                    ]),
                ],
            ),
        },
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": "c",
        "loss_record": {
            "structural_divergence": (
                "monadic bind requires four explicit tokens (name, r, var, body); "
                "no polymorphic function: macro must be instantiated per result type via name token; "
                "continuation f is inlined as body expression, not a function pointer; "
                "GNU statement-expression extension required for Ok-arm let-binding"
            ),
            "domain_narrowing": (
                "continuations must be expressible as a single statement-expression body; "
                "multi-statement continuations require a wrapper function; "
                "recursive or higher-order bind chains require manually nested macro invocations"
            ),
            "ub_introduction": (
                "accessing union field without checking tag is undefined behaviour in C; "
                "the macro guards within a single expansion but name-token mismatch introduces "
                "UB on exactly the states the abstraction statically excludes"
            ),
            "extension_dependency": (
                "statement expressions (({ ... })) are a GCC/Clang extension; "
                "not conforming ISO C11; programs targeting MSVC or strict C conformance "
                "are in the narrowed-out domain"
            ),
        },
        "discharge_receipt": DEFERRED_RECEIPT,
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
    ABST_DIR.mkdir(parents=True, exist_ok=True)
    REAL_DIR.mkdir(parents=True, exist_ok=True)

    cid_rows = []

    # Step 1: compute realization CID first so abstraction can reference it.
    print("[B] Minting concept:result-bind->c:result-bind-macro realization (N edge)...")
    real_memento = build_realization_result_bind_c()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:result-bind->c:result-bind-macro.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:result-bind->c:result-bind-macro: {real_cid[:40]}...")
    cid_rows.append({
        "kind": "realization",
        "name": "concept:result-bind->c:result-bind-macro",
        "cid": real_cid,
        "path": str(real_path),
    })

    # Step 2: build and mint abstraction once, with realizations already populated.
    print("[A] Minting concept:result-bind abstraction (with realization CID populated)...")
    abst_memento = build_abstraction_result_bind()
    abst_memento["realizations"] = [real_cid]
    abst_entry, abst_cid = catalog_entry(abst_memento)
    abst_path = ABST_DIR / f"concept:result-bind.{abst_cid}.json"
    write_json(abst_path, abst_entry)
    print(f"  concept:result-bind: {abst_cid[:40]}...")
    cid_rows.append({
        "kind": "abstraction",
        "name": "concept:result-bind",
        "cid": abst_cid,
        "path": str(abst_path),
    })

    # Stability check: mint each artifact a second time and compare.
    print("\n[STABILITY] Re-minting all artifacts for byte-stability check...")
    stable = True
    for check_name, memento, expected_cid in [
        ("abstraction", abst_memento, abst_cid),
        ("realization", real_memento, real_cid),
    ]:
        check_cid = compute_cid(memento)
        if check_cid != expected_cid:
            print(f"  UNSTABLE: {check_name}: {check_cid} != {expected_cid}")
            stable = False
        else:
            print(f"  STABLE: {check_name}: ok")
    if not stable:
        raise SystemExit("ABORTING: CID instability detected. Fix canonical key order.")

    append_cids_tsv(cid_rows)

    print("\n[DONE] Minted CIDs:")
    print(f"  abstraction CID:     {abst_cid}")
    print(f"  realize (N edge) CID:{real_cid}")

    return abst_cid, real_cid


if __name__ == "__main__":
    mint_all()
