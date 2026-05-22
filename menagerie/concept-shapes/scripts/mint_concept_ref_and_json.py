#!/usr/bin/env python3
"""Mint concept:Ref<T> + concept:Json — substrate-canonical primitives
identified by the libprovekit-rpc-cross-platform → java cross-language gap audit.

concept:Ref<T>:
  Parametric reference / handle / out-parameter. Substrate-canonical name for
  the &mut T pattern (rust), StringBuilder/AtomicReference (java),
  list-of-one (python), pointers (C). The mutability is declared at the
  morphism (each kit specifies whether its realization is mutable, output-
  only, or in-out via the morphism's runtime_guards / loss profile).

concept:Json:
  JSON value tree primitive — substrate-canonical for serde_json::Value
  (rust), JsonNode/JsonElement (java), dict (python), generic Object (ts).
  Internally recursive (a Json is null/bool/number/string/array-of-Json/
  object-of-string-to-Json) but at the substrate level a single named sort.

Both are parametric where applicable. Per-kit sort-morphisms follow in a
separate mint script; this script just creates the concept-hub sort entries.
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


def mint_sort(name: str, sort_kind: str) -> str:
    memento: dict[str, Any] = {
        "schema_version": "1",
        "protocol": "LSP",
        "kind": "SortMemento",
        "fn_name": name,
        "formals": [],
        "formal_sorts": [],
        "pre": {"args": [], "kind": "atomic", "name": "true"},
        "post": {
            "kind": "sort-instance",
            "name": name,
            "sort_kind": sort_kind,
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
    # Filesystem-safe filename: replace < > , with _of_ "" "_" so the file
    # is checkout-able on Windows NTFS (rejects < and >) and shell-friendly
    # on POSIX. Substrate identity is the CID + memento content, NOT the
    # filename — so this rename loses nothing.
    safe_name = (
        name.replace("<", "_of_").replace(">", "").replace(",", "_")
    )
    filename = f"{safe_name}.{cid}.json"
    out_path = SORTS_DIR / filename
    content = json.dumps(envelope, indent=2, sort_keys=True, ensure_ascii=False) + "\n"
    out_path.write_text(content, encoding="utf-8")
    return cid


def main() -> int:
    ref_cid = mint_sort("Ref<T>", "parametric")
    print(f"concept:Ref<T> CID: {ref_cid}")
    json_cid = mint_sort("Json", "primitive")  # treated as primitive at substrate level
    print(f"concept:Json CID:   {json_cid}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
