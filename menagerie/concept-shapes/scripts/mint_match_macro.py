#!/usr/bin/env python3
"""Mint concept:match, concept:match-arm, and concept:macro-call —
last 2 structural lifts replacing source_text fallbacks in walk_rpc.

concept:match(scrutinee, arm1, arm2, ...):
  Pattern-matching dispatch. scrutinee is the expression being matched;
  each arm is a concept:match-arm.

concept:match-arm(pattern, body):
  Single arm of a match. pattern is a leaf carrying the pattern's
  textual form (full pattern decomposition to concept:literal-pattern /
  concept:constructor-pattern / concept:wildcard-pattern is follow-up
  substrate-mint work; for now pattern is a symbol leaf with kit-side
  parsing). body is the arm's structural term-shape.

concept:macro-call(path, args...):
  Macro invocation. path is the macro's identifier (e.g. "println",
  "writeln"); args are the argument expressions structural shapes.
  Kits with macro systems (rust, scheme, lisp variants) use this;
  others can refuse the lift or decompose to concept:call.
"""

from __future__ import annotations
import json
from pathlib import Path
from typing import Any

BASE = Path(__file__).resolve().parents[1]
ALGORITHMS = BASE / "catalog" / "algorithms"


def jcs_canonical(value: object) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def blake3_512_of_bytes(data: bytes) -> str:
    from blake3 import blake3 as _blake3
    return f"blake3-512:{_blake3(data).digest(length=64).hex()}"


def mint(operator: str, slots: list[dict], formal_sorts: list[str],
         result_sort: str, contract_note: str) -> str:
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
    out = ALGORITHMS / f"{operator}.{cid}.json"
    out.write_text(json.dumps(envelope, indent=2, sort_keys=True, ensure_ascii=False) + "\n", encoding="utf-8")
    return cid


def main() -> int:
    match_cid = mint(
        "concept:match",
        slots=[{"name": "scrutinee"}, {"name": "arms"}],
        formal_sorts=["Expr", "ListOfMatchArm"],
        result_sort="Expr",
        contract_note="Pattern-matching dispatch on scrutinee against the ordered arms.",
    )
    print(f"concept:match CID:      {match_cid}")
    arm_cid = mint(
        "concept:match-arm",
        slots=[{"name": "pattern"}, {"name": "body"}],
        formal_sorts=["Pattern", "Expr"],
        result_sort="MatchArm",
        contract_note="Single arm of a match: pattern + body. Pattern is currently a kit-side symbol leaf; full pattern decomposition is follow-up substrate-mint work.",
    )
    print(f"concept:match-arm CID:  {arm_cid}")
    macro_cid = mint(
        "concept:macro-call",
        slots=[{"name": "path"}, {"name": "args"}],
        formal_sorts=["Name", "ListOfExpr"],
        result_sort="Expr",
        contract_note="Macro invocation: macro identified by path; args is the structural argument list.",
    )
    print(f"concept:macro-call CID: {macro_cid}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
