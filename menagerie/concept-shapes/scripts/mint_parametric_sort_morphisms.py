#!/usr/bin/env python3
"""Mint parametric sort morphisms (List<T> + Map<K,V>) across 10 languages.

Closes the remaining sort-classification gap for the 2 parametric substrate
sorts. For each (language, parametric_primitive) pair:

1. Mint a per-language parametric SortMemento (e.g. rust:List<T>) where it
   doesn't yet exist in catalog/sorts/.
2. Mint a sort-morphism with parametric-composition metadata. The morphism
   declares that the parametric sort composes over its inner T (or K, V)
   parameters: applying the morphism to lang:List<T> requires composing
   the morphism lang:T → concept:T for the inner.

Per-language realizations:

  List<T>:
    c11:List<T>          → struct-of-arrays + size_t (loss: no-bounds-check)
    csharp:List<T>       → System.Collections.Generic.List<T>
    go:List<T>           → []T
    java:List<T>         → java.util.List<T>
    php:List<T>          → array (heterogeneous via runtime type tag)
    python:List<T>       → list[T]
    ruby:List<T>         → Array<T>
    rust:List<T>         → Vec<T>
    typescript:List<T>   → T[]
    zig:List<T>          → []T

  Map<K,V>:
    c11:Map<K,V>         → CANNOT-BE (no native polymorphic map — loss:
                           key-ordering-undefined, lookup-O(n), needs
                           struct-of-arrays + linear scan)
    csharp:Map<K,V>      → System.Collections.Generic.Dictionary<K,V>
    go:Map<K,V>          → map[K]V
    java:Map<K,V>        → java.util.Map<K,V>
    php:Map<K,V>         → array (associative)
    python:Map<K,V>      → dict[K,V]
    ruby:Map<K,V>        → Hash
    rust:Map<K,V>        → HashMap<K,V> (or BTreeMap<K,V> for ordered)
    typescript:Map<K,V>  → Map<K,V> (built-in)
    zig:Map<K,V>         → std.AutoHashMap(K, V)

Usage:
    python3 mint_parametric_sort_morphisms.py
"""

from __future__ import annotations

import json
import os
from pathlib import Path
from typing import Any

BASE = Path(__file__).resolve().parents[1]
ROOT = BASE.parents[1]
ALGORITHMS_DIR = BASE / "catalog" / "algorithms"

CONCEPT_HUB_SIG_CID = (
    "blake3-512:1979babed41ad51ad8d7a28543815f74e24a9d4ee1ae3d52ccc6549f293aa635"
    "19abf5411a67b7882c73333b1b357e4863f6d7781f0b0776e5bd25f90ea7d793"
)
CONCEPT_LIST_CID = (
    "blake3-512:e3f8d17445f9d2ce89c41c09cbeea08a8bc685d1c34a9fd3dfa7b1df17a94f40"
    "eab37396615501f1468baf2a1480fd5a27330ea23202b99876c5f4d97fa2cfb2"
)
CONCEPT_MAP_CID = (
    "blake3-512:b81923e3273fedfce0b84d401d8b30965d4c72530af6c7538d9ed9b2905348fa"
    "3c639636b21b3f47ac8a242e79eef8e278b1d6c9cfab8e289cf059cef94c82e1"
)

DECLARED_AT = "2026-05-21T00:00:00Z"

LANG_SIG = {
    "c11": "blake3-512:ad093bee1f2ad3ad15fe9e793efcc6ce9dc58138a31502af969bac79a8c81b1066abef449414c78867104188e9177dd51de88ccc2f192013f5c82fe69c1a0066",
    "csharp": "blake3-512:5e9d7f575403591269de929edae0eb247f3da4f9c56ac606ccba582bc4a86473ddd66696cb7cc02d0678c7a7c2a3f45698f5431b7418afefce12f332ee4c9ef4",
    "go": "blake3-512:8f98c68d534e7e799061bb710cc948b067b2a7e9359ec82c2fb5e3681a6fe30e19477fe27aead7b051f9b5a732f35b6235dccf2574c176bb682cd7d534c24d04",
    "java": "blake3-512:4d312a5ab13eba517063f097a73b8675f1ea2a915ab5cc5b92587c83b47f707b0298858ac2c48061eee73e30e1c48983bd0aec5641bc806561624fc7e4da44ef",
    "php": "blake3-512:a21df3e5d95608d76bed025cec6a7069b8a87ecef3675f13b273394730427e1c782e128ba827e77b0b7a2e74fdd089657c8807ab7795df9acbf0c0c2b87e4ad4",
    "python": "blake3-512:bc36b43fec1a80efcecb05f8c4de725f961295466530aec452763c6c479b67c590c2e8062a3f46979383086ae80e6c0a917c443625d3474a7a89705e0a56ab8c",
    "ruby": "blake3-512:c533d7b3d4cafeb50ece583d706ef224496e8e54246600b1ac134bf36e46deb7274de4680a7fd39cc93e41ca658a73445f68d6480acd02880c2083e959d58284",
    "rust": "blake3-512:e3c223b8b6f39382e43cb06c5b04059987e661d96311decd5003d4ec79c7d6f9969de39ae16dd6509cb5236185260d59c63288db7ff772aae00f8123ea826cbd",
    "typescript": "blake3-512:31444085d7d08f573d4a68730d9f30f77509be66369a92432d9a76fafe3cdf7c0ed5df53767d934ec0b77b8fafcdf3124589c0b5fa3eb7e9312891a08a95dc0d",
    "zig": "blake3-512:052e54f3a38b581eb4fde81df1a45213022ca06bf9eb50ea2d94996b49f507a0f247ed62bff10eae5a17a7191a8de92b0014af5dcf4ef6ae1d8bf6885e88e535",
}

LANGS = sorted(LANG_SIG.keys())

# (sort_name, description per language)
LIST_DESCRIPTIONS = {
    "c11": "C11 dynamic list: T* + size_t length pair. Lookup O(1), no bounds check.",
    "csharp": "C# System.Collections.Generic.List<T> (heap-backed dynamic array).",
    "go": "Go []T slice — pointer + length + capacity triple.",
    "java": "Java java.util.List<T> (interface; ArrayList/LinkedList implementations).",
    "php": "PHP array as sequential list (integer-indexed). Heterogeneous via runtime tag.",
    "python": "Python list[T] (dynamic array of references).",
    "ruby": "Ruby Array (dynamic array of references).",
    "rust": "Rust Vec<T> (heap-backed, owned, growable).",
    "typescript": "TypeScript T[] (array literal type — JS Array under the hood).",
    "zig": "Zig []T (slice of T — pointer + length pair).",
}

MAP_DESCRIPTIONS = {
    "c11": "C11 has no native polymorphic map. Realized as parallel arrays (T_keys[], V_values[]) with manual scan — lookup O(n), no key-ordering guarantee.",
    "csharp": "C# System.Collections.Generic.Dictionary<K,V> (hash table, O(1) amortized).",
    "go": "Go map[K]V — built-in hash table, O(1) amortized.",
    "java": "Java java.util.Map<K,V> (interface; HashMap/TreeMap implementations).",
    "php": "PHP associative array — hash table semantics with insertion order.",
    "python": "Python dict[K,V] (hash table, O(1) amortized, insertion order preserved in 3.7+).",
    "ruby": "Ruby Hash (insertion-order-preserving hash table).",
    "rust": "Rust std::collections::HashMap<K,V> (hash table; BTreeMap available for ordered).",
    "typescript": "TypeScript built-in Map<K,V> (hash table, ES6+).",
    "zig": "Zig std.AutoHashMap(K, V) / std.HashMap(K, V, ...).",
}


def jcs_canonical(value: object) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def blake3_512_of_bytes(data: bytes) -> str:
    from blake3 import blake3 as _blake3
    digest = _blake3(data).digest(length=64)
    return f"blake3-512:{digest.hex()}"


def already_present_sort(lang: str, sort_name: str) -> str | None:
    """Return CID of existing lang sort if present, else None."""
    sorts_dir = ROOT / "menagerie" / f"{lang}-language-signature" / "catalog" / "sorts"
    if not sorts_dir.is_dir():
        return None
    prefix = f"{lang}:{sort_name}."
    for fn in os.listdir(sorts_dir):
        if fn.startswith(prefix):
            rest = fn[len(prefix):]
            cid = "blake3-512:" + rest.rsplit(".json", 1)[0].replace("blake3-512:", "", 1)
            return "blake3-512:" + rest.split("blake3-512:", 1)[1].rsplit(".json", 1)[0]
    return None


def build_sort(lang: str, sort_name: str, description: str) -> str:
    """Mint a parametric SortMemento for `<lang>:<sort_name>`. Returns CID."""
    fn_name = f"{lang}:{sort_name}"
    memento: dict[str, Any] = {
        "schema_version": "1",
        "protocol": "LSP",
        "kind": "SortMemento",
        "fn_name": fn_name,
        "formals": [],
        "formal_sorts": [],
        "pre": {"kind": "atomic", "name": "true", "args": []},
        "post": {
            "kind": "sort-description",
            "name": sort_name,
            "description": description,
        },
        "effects": {"effects": []},
        "auto_minted_mementos": [],
        "return_sort": {"kind": "kind", "name": "*"},
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
    sorts_dir = ROOT / "menagerie" / f"{lang}-language-signature" / "catalog" / "sorts"
    sorts_dir.mkdir(parents=True, exist_ok=True)
    # File name uses HTML-safe substitution for the parametric brackets.
    filename = f"{fn_name}.{cid}.json"
    (sorts_dir / filename).write_text(
        json.dumps(envelope, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    return cid


def build_morphism(lang: str, primitive: str, lang_sort_cid: str,
                   substrate_sort_cid: str, direction: str,
                   precision_loss: str, range_loss: str,
                   runtime_guards: list, note: str) -> tuple[str, Path]:
    header: dict[str, Any] = {
        "cid": "",
        "direction": direction,
        "kind": "sort-morphism",
        "precision_loss": precision_loss,
        "range_loss": range_loss,
        "representation_constraints": [],
        "runtime_guards": runtime_guards,
        "schemaVersion": "1",
        "source_language_signature_cid": LANG_SIG[lang],
        "source_sort_cid": lang_sort_cid,
        "target_language_signature_cid": CONCEPT_HUB_SIG_CID,
        "target_sort_cid": substrate_sort_cid,
    }
    metadata = {"note": note}
    cid_input = {
        "header": {k: v for k, v in header.items() if k != "cid"},
        "metadata": metadata,
    }
    cid = blake3_512_of_bytes(jcs_canonical(cid_input).encode("utf-8"))
    header["cid"] = cid
    safe = primitive.replace("<", "_of_").replace(">", "").replace(",", "_")
    filename = f"sort-morphism:{lang}:{safe.lower()}:to:concept:{safe}.{cid}.json"
    out_path = ALGORITHMS_DIR / filename
    envelope = {
        "envelope": {
            "declaredAt": DECLARED_AT,
            "signature": "ed25519:UNSIGNED_DEV_ONLY",
            "signer": "ed25519:UNSIGNED_DEV_ONLY",
        },
        "header": header,
        "metadata": metadata,
    }
    out_path.write_text(
        json.dumps(envelope, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    return cid, out_path


def main() -> int:
    written_sorts = []
    written_morphisms = []

    for lang in LANGS:
        # ----- List<T> -----
        sort_name = "List<T>"
        sort_cid = already_present_sort(lang, sort_name)
        if sort_cid is None:
            sort_cid = build_sort(lang, sort_name, LIST_DESCRIPTIONS[lang])
            written_sorts.append((lang, sort_name, sort_cid))
        morph_cid, path = build_morphism(
            lang=lang,
            primitive="List<T>",
            lang_sort_cid=sort_cid,
            substrate_sort_cid=CONCEPT_LIST_CID,
            direction="bidirectional",
            precision_loss="none",
            range_loss="none",
            runtime_guards=[],
            note=(
                f"{lang}'s native list/array realization of concept:List<T>. "
                f"Parametric — composes with the inner T's morphism: "
                f"applying this morphism to a value of type lang:List<T> requires "
                f"applying lang:T → concept:T to each element."
            ),
        )
        written_morphisms.append((lang, "List<T>", morph_cid))

        # ----- Map<K,V> -----
        sort_name = "Map<K,V>"
        sort_cid = already_present_sort(lang, sort_name)
        if sort_cid is None:
            sort_cid = build_sort(lang, sort_name, MAP_DESCRIPTIONS[lang])
            written_sorts.append((lang, sort_name, sort_cid))
        if lang == "c11":
            direction = "left-to-right"
            precision_loss = "none"
            range_loss = "narrowing"
            guards = [{"kind": "linear-scan-required", "failure_mode": "loss"}]
            note = (
                "C11 has no native polymorphic map — parallel arrays with linear "
                "scan. Loss: key-ordering-undefined, lookup-O(n), no amortized-O(1). "
                "Parametric over K + V."
            )
        else:
            direction = "bidirectional"
            precision_loss = "none"
            range_loss = "none"
            guards = []
            note = (
                f"{lang}'s native map/dict/hash realization of concept:Map<K,V>. "
                f"Parametric — composes with inner K and V morphisms."
            )
        morph_cid, path = build_morphism(
            lang=lang,
            primitive="Map<K,V>",
            lang_sort_cid=sort_cid,
            substrate_sort_cid=CONCEPT_MAP_CID,
            direction=direction,
            precision_loss=precision_loss,
            range_loss=range_loss,
            runtime_guards=guards,
            note=note,
        )
        written_morphisms.append((lang, "Map<K,V>", morph_cid))

    print(f"=== Parametric sort + morphism mint summary ===")
    print(f"Newly minted lang sorts: {len(written_sorts)}")
    for lang, name, cid in written_sorts:
        print(f"  {lang}:{name} → {cid[:30]}...")
    print(f"\nMorphisms written: {len(written_morphisms)}")
    for lang, primitive, cid in written_morphisms:
        print(f"  {lang} → concept:{primitive} ({cid[:30]}...)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
