#!/usr/bin/env python3
"""Mint concept:cast + concept:index — two more substrate-canonical operators
closing the remaining same-language refusals.

concept:cast(value, target_type_name):
  Type-coercion: `value as TargetType` in rust, `(TargetType)value` in C/Java,
  `T(value)` in C++/Python. target_type_name is a symbol leaf carrying the
  type identifier (concept-hub sort CID is the substrate-canonical form;
  for kits without a concept-hub sort for the target, falls back to symbol).

concept:index(receiver, index):
  Indexed access: `receiver[index]` across most languages. receiver +
  index are structural shapes.
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
    memento = {
        "kind": "concept-abstraction", "operator": operator, "tier": "abstraction",
        "slots": slots, "formal_sorts": formal_sorts, "result_sort": result_sort,
        "contract": {"kind": "wp-rule", "formals": [s["name"] for s in slots],
                     "body": {"kind": "atomic", "name": "true"}},
        "contract_note": contract_note, "realizations": [],
    }
    cid = blake3_512_of_bytes(jcs_canonical(memento).encode("utf-8"))
    envelope = {"memento": memento, "cid": cid,
                "signature": {"alg": "ed25519", "key_id": "UNSIGNED_DEV_ONLY",
                              "sig_b64": "A"*86 + "AA"}}
    (ALGORITHMS / f"{operator}.{cid}.json").write_text(
        json.dumps(envelope, indent=2, sort_keys=True, ensure_ascii=False) + "\n", encoding="utf-8")
    return cid


def main() -> int:
    cast_cid = mint("concept:cast", [{"name":"value"},{"name":"target_type"}],
                    ["Expr","Name"], "Expr",
                    "Type coercion: realizes as `value as T` (rust), `(T)value` (C/Java), etc.")
    print(f"concept:cast CID:  {cast_cid}")
    index_cid = mint("concept:index", [{"name":"receiver"},{"name":"index"}],
                     ["Expr","Expr"], "Expr",
                     "Indexed access: realizes as `receiver[index]` across most languages.")
    print(f"concept:index CID: {index_cid}")

    # Register in catalog index
    idx_path = BASE / "catalog" / "index.json"
    doc = json.loads(idx_path.read_text())
    entries = doc["entries"]
    for op, cid in [("concept:cast", cast_cid), ("concept:index", index_cid)]:
        # Find the file
        for fn in ALGORITHMS.iterdir():
            if fn.name.startswith(f"{op}.blake3-512:") and cid in fn.name:
                entries[cid] = {"cid": cid, "kind": "algorithm", "name": op,
                                "path": f"algorithms/{fn.name}"}
                break
    doc["entries"] = dict(sorted(entries.items()))
    idx_path.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"Registered. Catalog entries: {len(entries)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
