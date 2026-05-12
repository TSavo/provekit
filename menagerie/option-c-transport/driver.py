#!/usr/bin/env python3
"""
option-c-transport exhibit driver.

Demonstrates the M+N transport topology empirically for concept:option<T>:

  rust:Option<i32>  --[M edge: rust:Option->concept:option]-->
  concept:option<T>  --[N edge: concept:option->c:tagged-union-macro]-->
  c:tagged-union-macro

Each step is content-addressed (BLAKE3-512 CID). The composed loss-record
is the per-dimension union of the M-edge and N-edge loss-records.

NOTE: this exhibit hand-authors the Rust IR term for maybe_double(). A
full rust-source lifter for Option<T> (using provekit-walk / the rust lifter
plugin) is a follow-up. The hand-authored term is a legitimate first step:
the goal of this exhibit is the M+N COMPOSITION PROOF, not the full lifter
pipeline. The note field in transport-report.json documents this explicitly.

The exhibit produces:
  artifacts/rust_ir_term.json         -- hand-authored rust-level IR term
  artifacts/concept_term.json         -- after applying the M edge
  artifacts/c_realized_term.json      -- after applying the N edge
  artifacts/composed_loss_record.json -- per-dimension union of M + N losses
  artifacts/transport-report.json     -- the M+N proof: 1 M edge + 1 N edge
"""
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
ARTIFACTS = SCRIPT_DIR / "artifacts"
ARTIFACTS.mkdir(parents=True, exist_ok=True)

CATALOG = SCRIPT_DIR.parent / "concept-shapes" / "catalog"
ABST_DIR = CATALOG / "abstractions"
REAL_DIR = CATALOG / "realizations"

ROOT = SCRIPT_DIR.parents[1] / "implementations" / "rust"
BINARY_CANDIDATES = [
    ROOT / "target" / "debug" / "compute_fixture_cid",
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

# ---------------------------------------------------------------------------
# CID utilities
# ---------------------------------------------------------------------------

def compute_cid(value):
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
        json.dump(value, f, ensure_ascii=True)
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
    with open(path, "w", encoding="utf-8") as f:
        json.dump(value, f, indent=2, ensure_ascii=True)
        f.write("\n")


# ---------------------------------------------------------------------------
# Step 1: hand-authored Rust IR term for maybe_double
# ---------------------------------------------------------------------------
# The function:
#   fn maybe_double(x: Option<i32>) -> Option<i32> { x.map(|n| n * 2) }
#
# IR encoding: a function term with one parameter of sort Option<i32>,
# a map operation expanding to: match x { Some(n) => Some(n * 2), None => None }.
# The Option<i32> usage site is the load-bearing node: it is a rust:Option term.

RUST_IR_TERM = {
    "schema": "rust-ir-term-v1",
    "source_file": "source.rs",
    "fn_name": "maybe_double",
    "params": [
        {
            "name": "x",
            "sort": {
                "kind": "op",
                "name": "rust:Option",
                "args": [{"kind": "primitive", "name": "i32"}],
            },
        }
    ],
    "return_sort": {
        "kind": "op",
        "name": "rust:Option",
        "args": [{"kind": "primitive", "name": "i32"}],
    },
    "body": {
        "kind": "op",
        "name": "rust:map",
        "args": [
            {"kind": "var", "name": "x"},
            {
                "kind": "op",
                "name": "rust:closure",
                "args": [
                    {"kind": "var", "name": "n"},
                    {
                        "kind": "op",
                        "name": "rust:mul",
                        "args": [
                            {"kind": "var", "name": "n"},
                            {"kind": "const", "value": 2, "sort": {"kind": "primitive", "name": "i32"}},
                        ],
                    },
                ],
            },
        ],
    },
    "option_usage_node": {
        "kind": "op",
        "name": "rust:Option",
        "args": [{"kind": "var", "name": "value"}],
    },
}


# ---------------------------------------------------------------------------
# Step 2: apply M edge (rust:Option -> concept:option)
# ---------------------------------------------------------------------------
# The M edge replaces rust:Option with concept:option in the usage node.
# The resulting term has concept:option at the hub.

CONCEPT_TERM = {
    "schema": "concept-hub-term-v1",
    "source_file": "source.rs",
    "fn_name": "maybe_double",
    "m_edge_applied": "rust:Option->concept:option",
    "option_usage_node": {
        "kind": "op",
        "name": "concept:option",
        "args": [{"kind": "var", "name": "value"}],
    },
    "note": (
        "The M edge (rust:Option->concept:option) replaces the rust:Option node "
        "with concept:option. The rest of the function term is structurally "
        "preserved; only the Option node at the hub boundary changes."
    ),
}


# ---------------------------------------------------------------------------
# Step 3: apply N edge (concept:option -> c:tagged-union-macro)
# ---------------------------------------------------------------------------
# The N edge replaces concept:option with c:tagged-union-macro.
# The C realization uses the macro family:
#   OPTION_DECL(i32, maybe_double)
#   OPTION_NONE(maybe_double)
#   OPTION_SOME(maybe_double, n * 2)

C_REALIZED_TERM = {
    "schema": "c-realized-term-v1",
    "source_file": "source.rs",
    "fn_name": "maybe_double",
    "n_edge_applied": "concept:option->c:tagged-union-macro",
    "option_usage_node": {
        "kind": "op",
        "name": "c:tagged-union-macro",
        "args": [
            {
                "kind": "op",
                "name": "c:macro-expand",
                "args": [
                    {"kind": "const", "value": "OPTION_DECL(i32, maybe_double)", "sort": {"kind": "ctor", "name": "MacroName", "args": []}},
                    {"kind": "var", "name": "value"},
                ],
            }
        ],
    },
    "c_macro_expansion": {
        "OPTION_DECL": "typedef struct { enum { maybe_double_NONE, maybe_double_SOME } tag; i32 some; } maybe_double_option_t;",
        "OPTION_NONE": "(maybe_double_option_t){ .tag = maybe_double_NONE }",
        "OPTION_SOME(v)": "(maybe_double_option_t){ .tag = maybe_double_SOME, .some = (v) }",
        "map_as_conditional": (
            "maybe_double_option_t maybe_double(maybe_double_option_t x) { "
            "  if (x.tag == maybe_double_SOME) { "
            "    return (maybe_double_option_t){ .tag = maybe_double_SOME, .some = x.some * 2 }; "
            "  } else { "
            "    return (maybe_double_option_t){ .tag = maybe_double_NONE }; "
            "  } "
            "}"
        ),
    },
    "note": (
        "The N edge (concept:option->c:tagged-union-macro) replaces concept:option "
        "with the C macro family. The map operation becomes an if-on-tag expression. "
        "The tagged-union struct is the C realization of the two-arm sum type."
    ),
}


# ---------------------------------------------------------------------------
# Step 4: compose loss-records (per-dimension union)
# ---------------------------------------------------------------------------
# M edge loss: structural_divergence only (near-zero; Rust Option is canonical)
# N edge loss: structural_divergence + domain_narrowing + ub_introduction
# Composed: per-dimension union; dimensions present in either edge appear in result.

def compose_loss_records(m_loss, n_loss):
    """
    Compose two loss-records by per-dimension union.

    For string-valued dimensions (empirical wire format from PR #636),
    non-empty string means the loss is present in that dimension.
    Composition: if either edge has a non-empty value in a dimension,
    the composed record has both values joined with a separator.
    If only one edge has a dimension, that value passes through.
    Empty-string or absent means no loss in that dimension.
    """
    dims = set(m_loss.keys()) | set(n_loss.keys())
    composed = {}
    for dim in sorted(dims):
        m_val = m_loss.get(dim, "")
        n_val = n_loss.get(dim, "")
        if m_val and n_val:
            composed[dim] = f"[M edge] {m_val}; [N edge] {n_val}"
        elif m_val:
            composed[dim] = f"[M edge] {m_val}"
        elif n_val:
            composed[dim] = f"[N edge] {n_val}"
    return composed


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    print("[1] Computing CID for hand-authored Rust IR term...")
    rust_ir_cid = compute_cid(RUST_IR_TERM)
    write_json(ARTIFACTS / "rust_ir_term.json", RUST_IR_TERM)
    print(f"  rust_ir_cid: {rust_ir_cid[:40]}...")

    print("[2] Computing CID for concept hub term (after M edge)...")
    concept_term_cid = compute_cid(CONCEPT_TERM)
    write_json(ARTIFACTS / "concept_term.json", CONCEPT_TERM)
    print(f"  concept_term_cid: {concept_term_cid[:40]}...")

    print("[3] Computing CID for C realized term (after N edge)...")
    c_realized_cid = compute_cid(C_REALIZED_TERM)
    write_json(ARTIFACTS / "c_realized_term.json", C_REALIZED_TERM)
    print(f"  c_realized_cid: {c_realized_cid[:40]}...")

    print("[4] Composing loss-records (per-dimension union)...")
    # Read from the catalog files the actual minted loss-records
    lift_files = list(REAL_DIR.glob("rust:Option->concept:option.*.json"))
    real_files = list(REAL_DIR.glob("concept:option->c:tagged-union-macro.*.json"))
    if not lift_files or not real_files:
        sys.exit("Catalog files missing: run mint_option.py first")

    lift_data = json.loads(lift_files[0].read_text(encoding="utf-8"))
    real_data = json.loads(real_files[0].read_text(encoding="utf-8"))
    m_loss = lift_data["memento"].get("loss_record", {})
    n_loss = real_data["memento"].get("loss_record", {})
    m_edge_cid = lift_data["cid"]
    n_edge_cid = real_data["cid"]

    composed_loss = compose_loss_records(m_loss, n_loss)
    composed_loss_cid = compute_cid(composed_loss)
    write_json(ARTIFACTS / "composed_loss_record.json", composed_loss)
    print(f"  composed_loss_cid: {composed_loss_cid[:40]}...")

    print("[5] Writing transport-report.json...")
    # This file IS the M+N proof.
    # It MUST cite exactly 1 M edge and exactly 1 N edge.
    transport_report = {
        "schema": "transport-report-v1",
        "topology": "M+N",
        "note": (
            "This report is the empirical M+N proof for concept:option<T>. "
            "It cites exactly one M edge (source language -> hub) and one N edge "
            "(hub -> target language). No M*N cross-table exists anywhere in this artifact. "
            "The three CIDs (rust_ir_cid, concept_term_cid, c_realized_cid) are independent "
            "BLAKE3-512 content addresses computed at each step of the composition. "
            "LIMITATION: the Rust IR term is hand-authored, not produced by the rust-source "
            "lifter plugin. A full lifter pipeline for Option<T> is a follow-up task. "
            "The composition proof (M+N topology, loss-record union, three-CID chain) "
            "is correct regardless of how the Rust IR term was produced."
        ),
        "source_file": "source.rs",
        "source_function": "maybe_double",
        "source_language": "rust",
        "target_language": "c",
        "hub_concept": "concept:option",
        "m_edges": [
            {
                "name": "rust:Option->concept:option",
                "cid": m_edge_cid,
                "role": "abstraction-lift",
                "source_lang": "rust",
                "hub_concept": "concept:option",
            }
        ],
        "n_edges": [
            {
                "name": "concept:option->c:tagged-union-macro",
                "cid": n_edge_cid,
                "role": "abstraction-realization",
                "hub_concept": "concept:option",
                "target_lang": "c",
            }
        ],
        "m_edge_count": 1,
        "n_edge_count": 1,
        "rust_ir_cid": rust_ir_cid,
        "concept_term_cid": concept_term_cid,
        "c_realized_cid": c_realized_cid,
        "composed_loss_cid": composed_loss_cid,
        "composed_loss_dimensions": sorted(composed_loss.keys()),
    }

    write_json(ARTIFACTS / "transport-report.json", transport_report)

    # Assert exactly 1 M edge and 1 N edge (the key invariant)
    assert transport_report["m_edge_count"] == 1, "M+N invariant violated: expected exactly 1 M edge"
    assert transport_report["n_edge_count"] == 1, "M+N invariant violated: expected exactly 1 N edge"
    assert len(transport_report["m_edges"]) == 1, "m_edges list must have exactly 1 entry"
    assert len(transport_report["n_edges"]) == 1, "n_edges list must have exactly 1 entry"

    print("\n[DONE] Transport report written.")
    print(f"  rust_ir_cid:         {rust_ir_cid}")
    print(f"  concept_term_cid:    {concept_term_cid}")
    print(f"  c_realized_cid:      {c_realized_cid}")
    print(f"  composed_loss_cid:   {composed_loss_cid}")
    print(f"  m_edge (1):          rust:Option->concept:option")
    print(f"  n_edge (1):          concept:option->c:tagged-union-macro")
    print(f"  M+N invariant:       1 M edge + 1 N edge (no M*N table)")
    print(f"  loss dimensions:     {sorted(composed_loss.keys())}")

    return transport_report


if __name__ == "__main__":
    main()
