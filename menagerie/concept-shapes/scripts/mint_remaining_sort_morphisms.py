#!/usr/bin/env python3
"""Mint the remaining could-be sort morphisms identified by the gap audit.

Companion to mint_sort_morphisms_to_concept_hub.py (which minted Float + Null
across 8 + 7 languages) and mint_sort_classification_gaps.py (which declared
125 gap mementos for the unanswered sort-classification exam questions).

This script walks the 14 substrate-canonical primitive sorts (Bool, Bytes,
Cid, EffectName, Float, Formula, Int, Null, OpCid, SortCid, String, Term —
parametric List<T> + Map<K,V> deferred to a future parametric-mint script)
× 10 target languages, picks the best-fit language sort from each kit's
catalog/sorts/ directory, and mints a SortMorphismMemento with appropriate
direction + loss profile.

Selection rules:
1. Direct match (e.g. rust:Int for concept:Int) → bidirectional, no loss.
2. Per-language aliases (typescript:Number ↔ concept:Float, java:Real, etc.)
   → bidirectional, no loss UNLESS the alias inherits an existing loss
   (typescript:Number → concept:Int is left-to-right with range_loss=2^53-bounded).
3. Polymorphic Value fallback (python:Value, ruby:Value, c11:Value, php:Value)
   → left-to-right with narrowing-via-runtime-guard.
4. No suitable sort in the language signature → SKIP. The gap memento
   remains in the catalog; mint the language sort first, then re-run.

This preserves the previously-minted 15 morphisms (Float × 8 + Null × 7)
by checking the catalog before writing.

Usage:
    python3 mint_remaining_sort_morphisms.py
"""

from __future__ import annotations

import json
import os
from pathlib import Path
from typing import Any


BASE = Path(__file__).resolve().parents[1]
ALGORITHMS_DIR = BASE / "catalog" / "algorithms"
ROOT = BASE.parents[1]

CONCEPT_HUB_SIG_CID = (
    "blake3-512:1979babed41ad51ad8d7a28543815f74e24a9d4ee1ae3d52ccc6549f293aa635"
    "19abf5411a67b7882c73333b1b357e4863f6d7781f0b0776e5bd25f90ea7d793"
)

DECLARED_AT = "2026-05-21T00:00:00Z"
PLACEHOLDER_SIGNER = "ed25519:UNSIGNED_DEV_ONLY"
PLACEHOLDER_SIGNATURE = "ed25519:UNSIGNED_DEV_ONLY"

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
PRIMITIVES = ["Bool", "Bytes", "Cid", "EffectName", "Float", "Formula", "Int",
              "Null", "OpCid", "SortCid", "String", "Term"]


def load_substrate_sort_cids() -> dict[str, str]:
    sorts_dir = BASE / "catalog" / "sorts"
    out: dict[str, str] = {}
    for fn in sorted(sorts_dir.iterdir()):
        if not fn.name.endswith(".json") or ".blake3-512:" not in fn.name:
            continue
        base, rest = fn.name.split(".blake3-512:", 1)
        cid = "blake3-512:" + rest.rsplit(".json", 1)[0]
        out[base] = cid
    return out


def load_lang_sort_cids(lang: str) -> dict[str, str]:
    d = ROOT / "menagerie" / f"{lang}-language-signature" / "catalog" / "sorts"
    if not d.is_dir():
        return {}
    out: dict[str, str] = {}
    for fn in os.listdir(d):
        if not fn.endswith(".json") or ".blake3-512:" not in fn:
            continue
        base, rest = fn.split(".blake3-512:", 1)
        cid = "blake3-512:" + rest.rsplit(".json", 1)[0]
        out[base] = cid
    return out


def existing_morphisms() -> set[tuple[str, str]]:
    """Return set of (language, target_concept_label) already minted."""
    out: set[tuple[str, str]] = set()
    for fn in sorted(ALGORITHMS_DIR.iterdir()):
        if not fn.name.startswith("sort-morphism:"):
            continue
        head = fn.name.split(".blake3-512:")[0]
        parts = head.split(":")
        if len(parts) >= 6:
            out.add((parts[1], parts[5]))
    return out


# Per-(language, primitive) selection profile. Returns None to skip.
def select(lang: str, primitive: str, lang_sorts: dict[str, str]) -> dict | None:
    # Direct match.
    direct = f"{lang}:{primitive}"
    if direct in lang_sorts:
        return {
            "lang_sort_name": primitive.lower(),
            "lang_sort_key": direct,
            "direction": "bidirectional",
            "precision_loss": "none",
            "range_loss": "none",
            "runtime_guards": [],
            "note": f"{lang}'s `{primitive}` sort maps cleanly to concept:{primitive}.",
        }
    # Per-language aliases.
    aliases = {
        ("typescript", "Bool"): ("ts:Boolean", "bool", "bidirectional", "none", "none", [],
            "TypeScript's `Boolean` sort maps cleanly to concept:Bool."),
        ("typescript", "String"): ("ts:String", "string", "bidirectional", "none", "none", [],
            "TypeScript's `String` sort maps cleanly to concept:String."),
        ("typescript", "Int"): ("ts:Number", "number", "left-to-right", "none", "2^53-bounded", [
            {"kind": "is-safe-integer-check", "failure_mode": "refuse"},
        ],
            "TypeScript's `Number` is IEEE 754 64-bit — concept:Int values exceeding 2^53-1 lose precision. Full Int coverage requires a separate typescript:bigint:to:concept:Int morphism."),
        ("typescript", "Float"): ("ts:Number", "number", "bidirectional", "none", "none", [],
            "TypeScript's `Number` is IEEE 754 64-bit float."),
        ("typescript", "Null"): ("ts:Null", "null", "bidirectional", "none", "none", [],
            "TypeScript has a typed `Null` sort."),
        ("java", "Float"): ("java:Real", "real", "bidirectional", "none", "none", [],
            "Java's `Real` sort covers `float` + `double` under one polymorphic sort."),
        ("java", "Null"): ("java:Ref", "ref", "left-to-right", "none", "narrowing", [
            {"kind": "is-null-check", "failure_mode": "refuse"},
        ],
            "Java's null is a specific value within the reference sort; narrowing via runtime null-check."),
        ("go", "Float"): ("go:Real", "real", "bidirectional", "none", "none", [],
            "Go's `Real` sort covers `float32` + `float64`."),
        ("go", "Null"): ("go:Ref", "ref", "left-to-right", "none", "narrowing", [
            {"kind": "is-nil-check", "failure_mode": "refuse"},
        ],
            "Go's nil is a specific value within reference types; narrowing via runtime nil-check."),
        ("php", "Float"): ("php:Real", "real", "bidirectional", "none", "none", [],
            "PHP's `Real` sort is IEEE 754 64-bit."),
        ("rust", "Null"): ("rust:Bottom", "bottom", "left-to-right", "none", "narrowing", [
            {"kind": "is-none-variant-check", "failure_mode": "refuse"},
        ],
            "Rust has no null — Option<()>::None is the closest equivalent, but it's a variant not a singleton. Per #1284 the rust:Null mapping was deferred; this morphism uses rust:Bottom as the closest available sort with a runtime guard."),
    }
    if (lang, primitive) in aliases:
        sort_key, sort_name, direction, ploss, rloss, guards, note = aliases[(lang, primitive)]
        if sort_key in lang_sorts:
            return {
                "lang_sort_name": sort_name,
                "lang_sort_key": sort_key,
                "direction": direction,
                "precision_loss": ploss,
                "range_loss": rloss,
                "runtime_guards": guards,
                "note": note,
            }
    # Polymorphic Value fallback.
    val_sort = f"{lang}:Value"
    if val_sort in lang_sorts:
        # cannot-be cases for narrowing via runtime guard.
        guard_kind = {
            "Bool": "is-bool-check",
            "Int": "is-int-check",
            "Float": "is-float-check",
            "String": "is-string-check",
            "Bytes": "is-bytes-check",
            "Null": "is-null-check",
            "Cid": "cid-format-check",
            "OpCid": "cid-format-check",
            "SortCid": "cid-format-check",
            "EffectName": "effect-name-format-check",
            "Formula": "formula-structural-check",
            "Term": "term-structural-check",
        }.get(primitive, f"is-{primitive.lower()}-check")
        return {
            "lang_sort_name": "value",
            "lang_sort_key": val_sort,
            "direction": "left-to-right",
            "precision_loss": "none",
            "range_loss": "narrowing",
            "runtime_guards": [{"kind": guard_kind, "failure_mode": "refuse"}],
            "note": f"{lang}'s polymorphic Value sort narrows to concept:{primitive} via runtime `{guard_kind}` guard.",
        }
    return None


def jcs_canonical(value: object) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def blake3_512_of_bytes(data: bytes) -> str:
    from blake3 import blake3 as _blake3
    digest = _blake3(data).digest(length=64)
    return f"blake3-512:{digest.hex()}"


def build_morphism(lang: str, primitive: str, profile: dict,
                   substrate_sort_cid: str, lang_sort_cid: str) -> tuple[str, Path]:
    header: dict[str, Any] = {
        "cid": "",
        "direction": profile["direction"],
        "kind": "sort-morphism",
        "precision_loss": profile["precision_loss"],
        "range_loss": profile["range_loss"],
        "representation_constraints": [],
        "runtime_guards": profile["runtime_guards"],
        "schemaVersion": "1",
        "source_language_signature_cid": LANG_SIG[lang],
        "source_sort_cid": lang_sort_cid,
        "target_language_signature_cid": CONCEPT_HUB_SIG_CID,
        "target_sort_cid": substrate_sort_cid,
    }
    metadata = {"note": profile["note"]}
    cid_input = {
        "header": {k: v for k, v in header.items() if k != "cid"},
        "metadata": metadata,
    }
    cid = blake3_512_of_bytes(jcs_canonical(cid_input).encode("utf-8"))
    header["cid"] = cid
    filename = (
        f"sort-morphism:{lang}:{profile['lang_sort_name']}"
        f":to:concept:{primitive}.{cid}.json"
    )
    out_path = ALGORITHMS_DIR / filename
    envelope = {
        "envelope": {
            "declaredAt": DECLARED_AT,
            "signature": PLACEHOLDER_SIGNATURE,
            "signer": PLACEHOLDER_SIGNER,
        },
        "header": header,
        "metadata": metadata,
    }
    content = json.dumps(envelope, indent=2, sort_keys=True, ensure_ascii=False) + "\n"
    out_path.write_text(content, encoding="utf-8")
    return cid, out_path


def main() -> int:
    substrate_sorts = load_substrate_sort_cids()
    minted = existing_morphisms()
    written = []
    skipped_minted = []
    skipped_no_lang_sort = []
    skipped_no_substrate_sort = []

    for lang in LANGS:
        lang_sorts = load_lang_sort_cids(lang)
        for primitive in PRIMITIVES:
            if primitive not in substrate_sorts:
                skipped_no_substrate_sort.append((lang, primitive))
                continue
            if (lang, primitive) in minted:
                skipped_minted.append((lang, primitive))
                continue
            profile = select(lang, primitive, lang_sorts)
            if profile is None:
                skipped_no_lang_sort.append((lang, primitive))
                continue
            lang_sort_cid = lang_sorts[profile["lang_sort_key"]]
            cid, path = build_morphism(
                lang, primitive, profile,
                substrate_sorts[primitive], lang_sort_cid,
            )
            written.append((lang, primitive, profile["direction"], cid))

    print(f"=== Sort-morphism mint summary ===")
    print(f"Languages: {len(LANGS)}")
    print(f"Substrate primitives (non-parametric): {len(PRIMITIVES)}")
    print(f"Already-minted (skipped): {len(skipped_minted)}")
    print(f"No language sort available (skipped — gap stays): {len(skipped_no_lang_sort)}")
    print(f"No substrate sort CID: {len(skipped_no_substrate_sort)}")
    print(f"Morphisms written: {len(written)}")

    dirs = {}
    for _, _, d, _ in written:
        dirs[d] = dirs.get(d, 0) + 1
    print(f"\nDirection breakdown:")
    for d, c in sorted(dirs.items()):
        print(f"  {d}: {c}")

    if skipped_no_lang_sort:
        print(f"\nSkipped (no lang sort — gap remains in catalog):")
        for lang, p in skipped_no_lang_sort:
            print(f"  {lang}:{p}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
