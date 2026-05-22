#!/usr/bin/env python3
"""Mint KitSourceAliasMemento for the rust kit (#1370).

Mirrors mint_java_kit_source_aliases.py for the rust kit's source-text tokens.
Each memento captures rust's signed declaration: "these source-text tokens
in my language denote this kit-sort." The rust lifter (walk_rpc +
source_transform) reads these via catalog query instead of hardcoded switches.
"""

from __future__ import annotations
import json
import os
from pathlib import Path
from typing import Any
from blake3 import blake3


BASE = Path(__file__).resolve().parents[1]
ALIASES_DIR = BASE / "catalog" / "kit-source-aliases"


def jcs_canonical(value: object) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def blake3_512_of_bytes(data: bytes) -> str:
    return f"blake3-512:{blake3(data).digest(length=64).hex()}"


def find_morphism_cid(kit_sort_lc: str, concept_sort_safe: str) -> str:
    """The filename pattern is sort-morphism:rust:<sort>:to:concept:<concept>.<full_cid>.json
    where <full_cid> already includes the `blake3-512:` prefix."""
    algorithms = BASE / "catalog" / "algorithms"
    prefix = f"sort-morphism:rust:{kit_sort_lc}:to:concept:{concept_sort_safe}."
    for fn in os.listdir(algorithms):
        if fn.startswith(prefix) and fn.endswith(".json"):
            rest = fn[len(prefix):]
            return rest.rsplit(".json", 1)[0]
    raise FileNotFoundError(f"No morphism for {prefix}*")


def mint_alias(
    sort_morphism_cid: str,
    source_aliases: list[str],
    denotes_parametric_application: dict | None = None,
    parametric_arity: int | None = None,
    out_name: str = "",
) -> tuple[str, Path]:
    memento: dict[str, Any] = {
        "kind": "KitSourceAlias",
        "kit": "rust",
        "sort_morphism_cid": sort_morphism_cid,
        "source_aliases": source_aliases,
        "denotes_parametric_application": denotes_parametric_application,
        "parametric_arity": parametric_arity,
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
    ALIASES_DIR.mkdir(parents=True, exist_ok=True)
    filename = f"rust-{out_name}.{cid}.json"
    (ALIASES_DIR / filename).write_text(
        json.dumps(envelope, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    return cid, (ALIASES_DIR / filename)


# Concept-hub primitive CIDs (referenced by shorthand applications).
CONCEPT_STRING_CID = "blake3-512:be8721d24849feb74c4721520bdba02d352a94f49253a627cd509127472aa1c47cbe99cb705cac4159b5365abcce0c9aaa4901fe67630827deb6be1f9daeea10"
CONCEPT_BYTES_CID = "blake3-512:7116ef6e62e6739b213a8394f975a53c771b89f08c36d27143827acfcfebc0e39e5b82c530be668c3cfd5ec6966ccaa42930b37fdb1f4ac25652a970be10fb6b"
CONCEPT_REF_CID = "blake3-512:37d8efe0ce6321d1a16f80aa06cbdf056c846b8a99613731e8d64d9581af61bc517fd8c87daaff2c817585a7dfd763e09ed729fdc71d25fe16fb1b2e6ca33534"


def main() -> int:
    primitives = [
        ("bool",   "Bool",   ["bool"], "bool"),
        ("int",    "Int",    ["i8", "i16", "i32", "i64", "i128", "isize",
                                "u8", "u16", "u32", "u64", "u128", "usize"], "int"),
        ("float",  "Float",  ["f32", "f64"], "float"),
        ("string", "String", ["str", "String", "&str"], "string"),
        ("unit",   "Unit",   ["()"], "unit"),
        ("json",   "Json",   ["Value", "serde_json::Value"], "json"),
    ]
    print("Primitives:")
    for kit_lc, concept_safe, aliases, out_name in primitives:
        try:
            morphism_cid = find_morphism_cid(kit_lc, concept_safe)
        except FileNotFoundError as e:
            print(f"  SKIP {kit_lc}: {e}")
            continue
        cid, _ = mint_alias(morphism_cid, aliases, None, None, out_name)
        print(f"  {out_name}: {cid[:30]}... (aliases: {aliases})")

    # Parametric constructors.
    parametric = [
        ("list_of_t", "List_of_T", ["Vec"], 1, "list"),
        ("ref_of_t",  "Ref_of_T",  ["RefMut", "Box"], 1, "ref"),
        ("map_of_k_v", "Map_of_K_V", ["HashMap", "BTreeMap", "Map"], 2, "map"),
    ]
    print("\nParametric constructors:")
    for kit_lc, concept_safe, aliases, arity, out_name in parametric:
        try:
            morphism_cid = find_morphism_cid(kit_lc, concept_safe)
        except FileNotFoundError as e:
            print(f"  SKIP {kit_lc}: {e}")
            continue
        cid, _ = mint_alias(morphism_cid, aliases, None, arity, out_name)
        print(f"  {out_name}: {cid[:30]}... (arity={arity}, aliases: {aliases})")

    # Shorthand applications. Vec<u8> and [u8; N] are concept:Bytes shorthand
    # (rust treats byte buffers as a distinct concept from generic list-of-T).
    # &mut T is the parametric application of Ref to T — its outer syntax IS
    # `&mut` but the inner T is at use-site, so it's a CONSTRUCTOR not a
    # shorthand. Rust source_transform handles &mut specially as a
    # parametric application (constructor with arity-1 inner).
    bytes_morphism = None
    try:
        bytes_morphism = find_morphism_cid("value", "Bytes")  # rust uses :value: filename pattern for Bytes
    except FileNotFoundError:
        try:
            bytes_morphism = find_morphism_cid("bytes", "Bytes")
        except FileNotFoundError:
            pass
    print("\nShorthand applications:")
    if bytes_morphism:
        cid, _ = mint_alias(
            bytes_morphism,
            ["Vec<u8>", "[u8]"],
            None, None, "bytes",
        )
        print(f"  bytes: {cid[:30]}... (Vec<u8>, [u8] = concept:Bytes)")

    # &mut T (rust's mutable reference) registered as a PARAMETRIC CONSTRUCTOR
    # (arity 1). Its source token is the bare prefix `&mut`. Lifter parses
    # `&mut T` and treats `&mut` as a constructor token. This is unusual
    # (the syntax is prefix not call-form) but the lifter can handle it.
    ref_morphism = find_morphism_cid("ref_of_t", "Ref_of_T")
    cid, _ = mint_alias(ref_morphism, ["&mut"], None, 1, "ref-mut-prefix")
    print(f"  ref-mut-prefix: {cid[:30]}... (&mut T = Ref<T>; lifter handles prefix syntax)")

    print("\nDone.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
