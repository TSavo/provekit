#!/usr/bin/env python3
"""Compute the substrate-canonical composite CID for a parametric sort application.

A parametric sort identity (e.g. concept:Ref<concept:String>) is the application
of a constructor (concept:Ref) to arguments (concept:String). The composite CID
is content-addressed from the canonical form:

    {
      "kind": "parametric-sort-application",
      "constructor_cid": "blake3-512:<constructor-cid>",
      "arg_cids": ["blake3-512:<arg1>", "blake3-512:<arg2>", ...]
    }

cid = blake3-512(JCS(<canonical-form-above>))

This file provides:
  - compose(constructor_cid, *arg_cids) → composite_cid
  - canonical_form(constructor_cid, *arg_cids) → dict (for serialization)

Issue #1369: Parametric concept-hub sort identities via compositional content-
addressing.
"""

from __future__ import annotations
import json
from typing import Iterable
from blake3 import blake3


def jcs(value: object) -> bytes:
    """JCS-canonicalize JSON for content-addressing."""
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def blake3_512(data: bytes) -> str:
    return "blake3-512:" + blake3(data).digest(length=64).hex()


def canonical_form(constructor_cid: str, arg_cids: Iterable[str]) -> dict:
    """Build the substrate-canonical form for a parametric sort application.

    Args:
      constructor_cid: CID of the constructor sort (e.g. concept:Ref, concept:List).
      arg_cids: ordered argument CIDs (e.g. for Ref<String>, args = [String_cid]).
    """
    return {
        "kind": "parametric-sort-application",
        "constructor_cid": constructor_cid,
        "arg_cids": list(arg_cids),
    }


def compose(constructor_cid: str, *arg_cids: str) -> str:
    """Compute the composite CID for a parametric sort application.

    The same constructor + same args always produces the same CID
    (content-addressing). Different args produce different CIDs:
      compose(Ref, String_cid) != compose(Ref, Bytes_cid)
    """
    return blake3_512(jcs(canonical_form(constructor_cid, arg_cids)))


# Known constructor CIDs for the demo (substrate-minted in #1365).
REF_T_CID = "blake3-512:37d8efe0ce6321d1a16f80aa06cbdf056c846b8a99613731e8d64d9581af61bc517fd8c87daaff2c817585a7dfd763e09ed729fdc71d25fe16fb1b2e6ca33534"
LIST_T_CID = "blake3-512:e3f8d17445f9d2ce89c41c09cbeea08a8bc685d1c34a9fd3dfa7b1df17a94f40eab37396615501f1468baf2a1480fd5a27330ea23202b99876c5f4d97fa2cfb2"
MAP_KV_CID = "blake3-512:b81923e3aef0bafd5fde1d1f6c63fa9aa7e1b58c0ce9c64f10dac1eaa28aa0bb1d8b1bc4a2c0c8b75f4b9b6a1d3fc1f6b54bca8a83d23d63b3a45d4eb6a4e84d"  # placeholder

# Primitive arg CIDs.
STRING_CID = "blake3-512:be8721d24849feb74c4721520bdba02d352a94f49253a627cd509127472aa1c47cbe99cb705cac4159b5365abcce0c9aaa4901fe67630827deb6be1f9daeea10"
BYTES_CID = "blake3-512:7116ef6e62e6739b213a8394f975a53c771b89f08c36d27143827acfcfebc0e39e5b82c530be668c3cfd5ec6966ccaa42930b37fdb1f4ac25652a970be10fb6b"
INT_CID = "blake3-512:30ffc51350121a7172f3e4064a33c45bbd345756979fccff6875cd2ab33e4964d098a99df80cfbdf1ec1a0738c5ac3476f0ff8f75589ea511d1acd82c74ecd58"
JSON_CID = "blake3-512:702064722b23410fde0d1fd7afac165bf5914441d67abe1e19d63b0e8fe8117296d2677cc721ad096b8b3bb82d178af699bf14fd70bfb18756c5bed6f4434108"


def main() -> int:
    """Print the composite CIDs for the demo cases. Verifies the math works:
    different args produce different CIDs (the content-addressing invariant)."""
    cases = [
        ("Ref<String>", REF_T_CID, [STRING_CID]),
        ("Ref<Bytes>", REF_T_CID, [BYTES_CID]),
        ("Ref<Json>", REF_T_CID, [JSON_CID]),
        ("List<Int>", LIST_T_CID, [INT_CID]),
        ("List<String>", LIST_T_CID, [STRING_CID]),
        ("List<Bytes>", LIST_T_CID, [BYTES_CID]),
        ("List<Json>", LIST_T_CID, [JSON_CID]),
    ]
    print(f"{'Sort':<20} CID")
    print(f"{'-'*20} {'-'*88}")
    cids = {}
    for name, ctor, args in cases:
        cid = compose(ctor, *args)
        cids[name] = cid
        print(f"{name:<20} {cid}")
    # Invariant: different args → different CIDs.
    assert cids["Ref<String>"] != cids["Ref<Bytes>"], "Ref<String> and Ref<Bytes> MUST have different CIDs"
    assert cids["List<Int>"] != cids["List<String>"], "List<Int> and List<String> MUST have different CIDs"
    assert cids["Ref<String>"] != cids["List<String>"], "Ref<String> and List<String> MUST have different CIDs"
    # Invariant: same (constructor, args) → same CID (determinism).
    assert compose(REF_T_CID, STRING_CID) == compose(REF_T_CID, STRING_CID), "Determinism violated"
    print("\nInvariants verified.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
