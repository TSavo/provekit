#!/usr/bin/env python3
"""Mint concept:Unit — the substrate-canonical primitive for the empty/void
return / unit-type sort.

Substrate-honest gap surfaced 2026-05-21: cross-language materialize of
libprovekit-rpc-cross-platform → java refused 3 sites (stdio-write-line,
stderr-write-line, jcs-encode-* helpers with () return type) because the
rust kit could not lift `()` to a concept-hub sort CID — concept:Unit
didn't exist in catalog/sorts/. This script mints it.

Every kit's *:Unit sort already exists (c11:Unit, java:Unit, python:Unit,
rust:Unit, typescript:Unit, etc.). After this mint, the per-language
sort-morphisms can be minted to map each lang:Unit → concept:Unit.
"""

from __future__ import annotations
import json
from pathlib import Path
from typing import Any

BASE = Path(__file__).resolve().parents[1]
SORTS_DIR = BASE / "catalog" / "sorts"


def jcs_canonical(value: object) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def blake3_512_of_bytes(data: bytes) -> str:
    from blake3 import blake3 as _blake3
    return f"blake3-512:{_blake3(data).digest(length=64).hex()}"


def main() -> int:
    memento: dict[str, Any] = {
        "schema_version": "1",
        "protocol": "LSP",
        "kind": "SortMemento",
        "fn_name": "Unit",
        "formals": [],
        "formal_sorts": [],
        "pre": {"args": [], "kind": "atomic", "name": "true"},
        "post": {
            "kind": "sort-instance",
            "name": "Unit",
            "sort_kind": "primitive",
        },
        "effects": {"effects": []},
        "auto_minted_mementos": [],
        "return_sort": {"args": [], "kind": "ctor", "name": "SortCid"},
    }
    cid = blake3_512_of_bytes(jcs_canonical(memento).encode("utf-8"))
    envelope = {
        "memento": memento,
        "cid": cid,
        "signature": {
            "alg": "ed25519",
            "key_id": "UNSIGNED_DEV_ONLY",
            "sig_b64": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        },
    }
    filename = f"Unit.{cid}.json"
    out_path = SORTS_DIR / filename
    content = json.dumps(envelope, indent=2, sort_keys=True, ensure_ascii=False) + "\n"
    out_path.write_text(content, encoding="utf-8")
    print(f"minted concept:Unit at: {out_path}")
    print(f"CID: {cid}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
