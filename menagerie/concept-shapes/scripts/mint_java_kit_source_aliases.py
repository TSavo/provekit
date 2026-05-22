#!/usr/bin/env python3
"""Mint KitSourceAliasMemento for the java kit covering primitives + parametrics + shorthands.

Each memento captures the kit's signed commitment: "these source-text tokens
in my language denote this kit-sort." The lifter reads these mementos to
translate java type strings to concept-hub sort CIDs without hardcoded switches.

Output: menagerie/concept-shapes/catalog/kit-source-aliases/<kit>-<sort>.<cid>.json

Issue #1370 — catalog-driven lifter (prerequisite for #1369).
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
    """Find a sort-morphism's CID by its filename pattern.
    kit_sort_lc: lowercase kit sort identifier (e.g. 'int', 'list_of_t').
    concept_sort_safe: concept-hub sort filename-safe form (e.g. 'Int', 'List_of_T').
    Filename pattern: sort-morphism:java:<sort>:to:concept:<concept>.<full_cid>.json
    where <full_cid> already includes the `blake3-512:` prefix.
    """
    algorithms = BASE / "catalog" / "algorithms"
    prefix = f"sort-morphism:java:{kit_sort_lc}:to:concept:{concept_sort_safe}."
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
        "kit": "java",
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
    filename = f"java-{out_name}.{cid}.json"
    out_path = ALIASES_DIR / filename
    out_path.write_text(
        json.dumps(envelope, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    return cid, out_path


# Substrate-canonical primitive sort CIDs (referenced by parametric applications below).
CONCEPT_BOOL_CID = "blake3-512:0ee13bf3fd6b7ecfbee72dfbfc18a7c0ea7f1663de6cca43cefb36f5b4c03665452646094a7c296e819e75d683c6ce4821f3d7db3c3c78ae97f2d4e3451d2074"
CONCEPT_INT_CID = "blake3-512:30ffc51350121a7172f3e4064a33c45bbd345756979fccff6875cd2ab33e4964d098a99df80cfbdf1ec1a0738c5ac3476f0ff8f75589ea511d1acd82c74ecd58"
CONCEPT_FLOAT_CID = "blake3-512:b979e70c4d5e53d9bdf13d6f08330be3c5b0714b8c770d69bbd05946b86c36df5274be8145a2683cc29c278155c9c1ee65b6897913524eecb9e4c89c71862f57"
CONCEPT_STRING_CID = "blake3-512:be8721d24849feb74c4721520bdba02d352a94f49253a627cd509127472aa1c47cbe99cb705cac4159b5365abcce0c9aaa4901fe67630827deb6be1f9daeea10"
CONCEPT_UNIT_CID = "blake3-512:47682b09e5dba71f563db6249c6cb352f7d540986dc7f4cd8d4fb1aa6d9a503064033ee3eb9f36ee6f9e000f700f2f030ebfcfe2b2b8b7e81a345b0d56551f1b"
CONCEPT_BYTES_CID = "blake3-512:7116ef6e62e6739b213a8394f975a53c771b89f08c36d27143827acfcfebc0e39e5b82c530be668c3cfd5ec6966ccaa42930b37fdb1f4ac25652a970be10fb6b"
CONCEPT_JSON_CID = "blake3-512:702064722b23410fde0d1fd7afac165bf5914441d67abe1e19d63b0e8fe8117296d2677cc721ad096b8b3bb82d178af699bf14fd70bfb18756c5bed6f4434108"
CONCEPT_REF_CID = "blake3-512:37d8efe0ce6321d1a16f80aa06cbdf056c846b8a99613731e8d64d9581af61bc517fd8c87daaff2c817585a7dfd763e09ed729fdc71d25fe16fb1b2e6ca33534"
CONCEPT_LIST_CID = "blake3-512:e3f8d17445f9d2ce89c41c09cbeea08a8bc685d1c34a9fd3dfa7b1df17a94f40eab37396615501f1468baf2a1480fd5a27330ea23202b99876c5f4d97fa2cfb2"


def main() -> int:
    # Primitives — each alias references its sort-morphism by CID; source_aliases
    # lists the java source-text tokens that denote this kit-sort.
    primitives = [
        ("bool",   "Bool",   ["boolean", "Boolean"], "bool"),
        ("int",    "Int",    ["int", "Integer", "byte", "Byte", "short", "Short", "long", "Long"], "int"),
        ("real",   "Float",  ["float", "Float", "double", "Double"], "float"),
        ("string", "String", ["String", "CharSequence"], "string"),
        ("bytes",  "Bytes",  ["byte[]"], "bytes"),
        ("json",   "Json",   ["JsonNode", "JsonElement", "com.fasterxml.jackson.databind.JsonNode", "com.google.gson.JsonElement"], "json"),
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

    # Parametric constructors (the source-token denotes a constructor that
    # takes args at use-site).
    parametric = [
        ("list_of_t", "List_of_T", ["List", "ArrayList", "Collection", "Iterable",
                                      "java.util.List", "java.util.ArrayList",
                                      "java.util.Collection", "java.lang.Iterable"], 1, "list"),
        ("ref_of_t",  "Ref_of_T",  ["AtomicReference",
                                      "java.util.concurrent.atomic.AtomicReference"], 1, "ref"),
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

    # Shorthand applications (the source-token denotes a FIXED application
    # of a parametric constructor — StringBuilder is Ref<String>, etc.).
    # These reference the ref-morphism but specify the bound arg via
    # denotes_parametric_application.
    try:
        ref_morphism = find_morphism_cid("ref_of_t", "Ref_of_T")
    except FileNotFoundError:
        ref_morphism = None
    shorthands = [
        ("StringBuilder", CONCEPT_STRING_CID, "stringbuilder"),
        ("ByteArrayOutputStream", CONCEPT_BYTES_CID, "byte-array-output-stream"),
        ("java.lang.StringBuilder", CONCEPT_STRING_CID, "stringbuilder-fqn"),
        ("java.io.ByteArrayOutputStream", CONCEPT_BYTES_CID, "byte-array-output-stream-fqn"),
    ]
    print("\nShorthand applications:")
    if ref_morphism:
        for token, inner_cid, out_name in shorthands:
            cid, _ = mint_alias(
                ref_morphism,
                [token],
                {
                    "constructor_cid": CONCEPT_REF_CID,
                    "arg_cids": [inner_cid],
                },
                None,
                out_name,
            )
            inner_name = "String" if inner_cid == CONCEPT_STRING_CID else "Bytes"
            print(f"  {out_name}: {cid[:30]}... ({token} = Ref<{inner_name}>)")

    # concept:Unit's morphism doesn't exist yet by that exact filename;
    # the void alias maps to it via concept:Unit which the kit's sort
    # catalog should already have a java sort for. Skip if missing.
    print("\nDone.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
