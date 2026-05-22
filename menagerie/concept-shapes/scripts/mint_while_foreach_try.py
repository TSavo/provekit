#!/usr/bin/env python3
"""Mint concept:while, concept:for-each, concept:try — the 3 substrate-canonical
operators identified by the libprovekit-rpc-cross-platform → java gap audit
(2026-05-21). Closes the residual "literal source_text" fallbacks in the
java lifter's loop and try-catch handling.

concept:while(condition, body):
  General loop while condition holds. Both `while (c) { b }` and
  `do { b } while (c)` decompose to this (do-while is `concept:seq(body,
  concept:while(c, body))`).

concept:for-each(var, iterable, body):
  Iterator-based loop — covers Java enhanced-for (`for (T v : iter)`),
  Rust `for v in iter`, Python `for v in iter`, etc. The `var` arg is
  the binding pattern; `iterable` is the collection expression; `body`
  is the per-iteration statement.

concept:try(body, catch_arms...):
  Exception-handling block. First arg is the try body's term-shape;
  subsequent args are catch arms (each containing the exception type
  binding + catch-block term-shape). Finally blocks can be appended as
  a trailing arg (or modeled separately).

Classic C-style for(init; cond; update; body) intentionally NOT minted —
it decomposes to seq(init, while(cond, seq(body, update))) using
existing primitives.
"""

from __future__ import annotations
import json
from pathlib import Path
from typing import Any

BASE = Path(__file__).resolve().parents[1]
ALGORITHMS_DIR = BASE / "catalog" / "algorithms"


def jcs_canonical(value: object) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def blake3_512_of_bytes(data: bytes) -> str:
    from blake3 import blake3 as _blake3
    return f"blake3-512:{_blake3(data).digest(length=64).hex()}"


def mint_abstraction(operator: str, slots: list[dict], formal_sorts: list[str],
                     result_sort: str, contract_note: str) -> str:
    """Mint a concept-abstraction memento matching the shape of existing
    operator concepts (concept:closure, concept:assign, etc.)."""
    memento: dict[str, Any] = {
        "kind": "concept-abstraction",
        "operator": operator,
        "tier": "abstraction",
        "slots": slots,
        "formal_sorts": formal_sorts,
        "result_sort": result_sort,
        "contract": {
            "kind": "wp-rule",
            "formals": [s["name"] for s in slots],
            "body": {"kind": "atomic", "name": "true"},
        },
        "contract_note": contract_note,
        "realizations": [],
    }
    cid = blake3_512_of_bytes(jcs_canonical(memento).encode("utf-8"))
    envelope = {
        "memento": memento,
        "cid": cid,
        "signature": {
            "alg": "ed25519",
            "key_id": "UNSIGNED_DEV_ONLY",
            "sig_b64": "A" * 86 + "AA",
        },
    }
    # Write to catalog/algorithms (where peer concept files live and where
    # the catalog index expects them). Filesystem-safe filename: replace
    # angle brackets with `_of_` so the file is checkout-able on Windows.
    safe_op = operator.replace("<", "_of_").replace(">", "").replace(",", "_")
    filename = f"{safe_op}.{cid}.json"
    out_path = ALGORITHMS_DIR / filename
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(
        json.dumps(envelope, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    return cid


def main() -> int:
    while_cid = mint_abstraction(
        "concept:while",
        slots=[{"name": "condition"}, {"name": "body"}],
        formal_sorts=["Expr", "Stmt"],
        result_sort="Stmt",
        contract_note=("Repeatedly evaluate body while condition holds. "
                       "do-while decomposes to seq(body, concept:while(condition, body))."),
    )
    print(f"concept:while CID:    {while_cid}")
    for_each_cid = mint_abstraction(
        "concept:for-each",
        slots=[{"name": "var"}, {"name": "iterable"}, {"name": "body"}],
        formal_sorts=["Name", "Expr", "Stmt"],
        result_sort="Stmt",
        contract_note=("Iterator-based loop. Bind each element from iterable "
                       "to var; evaluate body per iteration. Covers Java enhanced-for, "
                       "Rust for, Python for in-iterator patterns."),
    )
    print(f"concept:for-each CID: {for_each_cid}")
    try_cid = mint_abstraction(
        "concept:try",
        slots=[{"name": "body"}, {"name": "catches"}],
        formal_sorts=["Stmt", "ListOfCatchArm"],
        result_sort="Stmt",
        contract_note=("Exception-handling block. Evaluate body; on thrown "
                       "exception, dispatch to the matching catch arm. Used by "
                       "all exception-aware kits (Java, Python, C++, ...)."),
    )
    print(f"concept:try CID:      {try_cid}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
