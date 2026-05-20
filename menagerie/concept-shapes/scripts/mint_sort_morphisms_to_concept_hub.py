#!/usr/bin/env python3
"""Mint SortMorphismMementos linking per-language sorts to substrate-canonical concept-hub sorts.

Per #1284 (sort-classification exam-manifest coverage) + #1290 (concept-hub
LanguageSignatureMemento, landed via PR #1295).

The exam manifest at `menagerie/concept-shapes/exams/v1.1.<cid>.json` carries
`sort-classification` questions per (language, substrate-canonical sort)
pair (added by #1282 for Float + Null). Each question expects a
`SortMorphismMemento` answer mapping the language-native sort to the
substrate-canonical sort.

This script mints 15 morphisms across 8 languages:

- 8 Float morphisms (for languages with a published value-tier sort).
- 7 Null morphisms (skipping Rust's null-free posture per #1284).

Per-language sort-source decisions:

- Languages with a TYPED float-equivalent sort (rust:Float, java:Real,
  go:Real, php:Real, ts:Number): use the typed sort. direction=bidirectional,
  precision_loss=none, range_loss=none.
- Languages with only a polymorphic Value-equivalent sort (c11:Value,
  python:Value, ruby:Value): use Value with narrowing-via-guard.
  direction=left-to-right, precision_loss=none, range_loss=narrowing,
  runtime_guards=[is-float check, panic-on-fail].

- Typescript Null: use ts:Null (typed). bidirectional, no loss.
- Java + Go Null: use java:Ref / go:Ref with narrowing-via-guard
  (null is a specific value within the reference sort).
- Other Null cases: use Value with narrowing-via-guard.

Languages NOT in this batch:

- **C#** and **Zig**: have published LanguageSignatureMementos but NO
  Value-equivalent sort and no typed Float/Null sort. Mint a polymorphic
  Value sort (or typed Float/Null) for these languages first, then
  add the morphisms.

Per the ruling at
`docs/plans/2026-05-19-concept-hub-language-signature-ruling.md`:
`target_language_signature_cid` = concept-hub-signature CID.
`target_sort_cid` = the substrate-canonical sort CID
(concept:Float or concept:Null).

Usage:
    python3 mint_sort_morphisms_to_concept_hub.py
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any


BASE = Path(__file__).resolve().parents[1]
ROOT = BASE.parents[1]
ALGORITHMS_DIR = BASE / "catalog" / "algorithms"
CIDS_TSV = BASE / "cids.tsv"

# Concept-hub signature CID (minted by #1290 / PR #1295).
CONCEPT_HUB_SIG_CID = (
    "blake3-512:1979babed41ad51ad8d7a28543815f74e24a9d4ee1ae3d52ccc6549f293aa635"
    "19abf5411a67b7882c73333b1b357e4863f6d7781f0b0776e5bd25f90ea7d793"
)

# Substrate-canonical sort CIDs (minted by #1282).
CONCEPT_FLOAT_CID = (
    "blake3-512:b979e70c4d5e53d9bdf13d6f08330be3c5b0714b8c770d69bbd05946b86c36df"
    "5274be8145a2683cc29c278155c9c1ee65b6897913524eecb9e4c89c71862f57"
)
CONCEPT_NULL_CID = (
    "blake3-512:62f6040bd3f414c1e6c2b7bdf276669cd5613b33cb508a81170170064ca3ffba"
    "771a4b0002dc52e059fce5f9f63a1874ef71bd4ec89ae06e89c87a3e91aac3b5"
)

DECLARED_AT = "2026-05-19T00:00:00Z"
PLACEHOLDER_SIGNER = "ed25519:UNSIGNED_DEV_ONLY"
PLACEHOLDER_SIGNATURE = "ed25519:UNSIGNED_DEV_ONLY"


# Per-language signature CIDs from
# menagerie/<lang>-language-signature/catalog/signatures/<lang>.<cid>.json
LANG_SIG = {
    "c11": "blake3-512:ad093bee1f2ad3ad15fe9e793efcc6ce9dc58138a31502af969bac79a8c81b1066abef449414c78867104188e9177dd51de88ccc2f192013f5c82fe69c1a0066",
    "go": "blake3-512:8f98c68d534e7e799061bb710cc948b067b2a7e9359ec82c2fb5e3681a6fe30e19477fe27aead7b051f9b5a732f35b6235dccf2574c176bb682cd7d534c24d04",
    "java": "blake3-512:4d312a5ab13eba517063f097a73b8675f1ea2a915ab5cc5b92587c83b47f707b0298858ac2c48061eee73e30e1c48983bd0aec5641bc806561624fc7e4da44ef",
    "php": "blake3-512:a21df3e5d95608d76bed025cec6a7069b8a87ecef3675f13b273394730427e1c782e128ba827e77b0b7a2e74fdd089657c8807ab7795df9acbf0c0c2b87e4ad4",
    "python": "blake3-512:bc36b43fec1a80efcecb05f8c4de725f961295466530aec452763c6c479b67c590c2e8062a3f46979383086ae80e6c0a917c443625d3474a7a89705e0a56ab8c",
    "ruby": "blake3-512:c533d7b3d4cafeb50ece583d706ef224496e8e54246600b1ac134bf36e46deb7274de4680a7fd39cc93e41ca658a73445f68d6480acd02880c2083e959d58284",
    "rust": "blake3-512:e3c223b8b6f39382e43cb06c5b04059987e661d96311decd5003d4ec79c7d6f9969de39ae16dd6509cb5236185260d59c63288db7ff772aae00f8123ea826cbd",
    "typescript": "blake3-512:31444085d7d08f573d4a68730d9f30f77509be66369a92432d9a76fafe3cdf7c0ed5df53767d934ec0b77b8fafcdf3124589c0b5fa3eb7e9312891a08a95dc0d",
}

# Per-language sort CIDs (typed where available, Value otherwise).
SORT_CIDS = {
    "c11:Value": "blake3-512:79486b695251166bd059a9132f9191488449eaf57b0e5de511a4154d38411939b505f7d264a75a0ee65106f2daef3cba0eba313f066597595a01d551edc25bd6",
    "go:Real": "blake3-512:ee0649863f9e2a94735c89ac99fc62ea0064b070be9a6c9f2f369c981a7fdf520b7a19abf359991b5273e1ce78438771dd2f40187fd19ffac16ea626e630b20a",
    "go:Ref": "blake3-512:a5726d76fb8e31f54b577bf1f2a5e96cbbfbf0a7b78ff21bf1076eac57714b6a23f022d1728e39cd6e72ebfe095d08a568e98b91f92033095da730b1fb379cd9",
    "java:Real": "blake3-512:484a52211f7da78267052614488f602dd5cb2ea30b1819a33ac00a36903a1ca76ef5868bfa24ec2779effca022f55da40ac23c8186cd9b57f59bed4aedcab1c5",
    "java:Ref": "blake3-512:ce36d6b03f5bf92d45f2859c37fb3ef5caafe17cbc7a85659ecac8c6622b307e42f068a5782a92a6de1b4161e8876462677ea332067f5313e62277c84ec327d2",
    "php:Real": "blake3-512:3e46819b946be6efa38d44d53a9b6af8f59ecbbe340fbc1442fde6b64c102f0d8d75147e0b27ee71b68abcc4392d42e8aa470387270d9bec2a81ce7a4fc9cc57",
    "php:Value": "blake3-512:e77f967b57778c67fc5488f54594fa911f7b272b698bf1dc98b25c3b03f98769f32aacb960c42b9b2d8c53ff97c98b554e4851472a0038eda81c2067e676f929",
    "python:Value": "blake3-512:89577b5e614db5ec2442478f9865fb112228dda0a867cd7b61ab7b859a021421904e12c6e4e0e5a465795f7a2b73ba2fd43d7db0a6adb7aba2f03c8b22a36a5b",
    "ruby:Value": "blake3-512:9e35b043f26d653b661693bfd8cd13c40ab6cc288f97a2e8bd5721cce74b68d8953c8825b38147fb3cb9820fd15db50f843f5e7c76521e1e470e97f731f38df9",
    "rust:Float": "blake3-512:fcc27aaf5069685809ba9e5e5c3eadeaaf78aa513eeea2a349df57c3b3d183220de317a606170a11c4abd4adf79c34f4d2692a2cf4aace71856fea4ffb8d611e",
    "ts:Number": "blake3-512:1a39cc3dd4bfb8f4131c4923615c68534d479f7d331b46fda21c3c93912b4ca716cc56232ce6f4d7e8890649883b55dc6a2ee4c537ca80f3c8cd8834c3f617a3",
    "ts:Null": "blake3-512:2c966d3cabe7779fde92755e34e809de88c59f541c49fa91737a8d3f91b54f23b3b84c98d4cb6674ed6e43a63f04b3a620f152221fc0dad9b8ea41b95793163b",
}


# Morphism specifications. Tuples of:
#   (lang_label, lang_sort_name, lang_sort_key, target_concept_cid, direction,
#    precision_loss, range_loss, runtime_guards, note)
#
# lang_sort_key indexes SORT_CIDS; lang_sort_name appears in the filename.
MORPHISMS: list[dict[str, Any]] = [
    # ===== Float morphisms (8 languages) =====
    {
        "lang_label": "rust",
        "lang_sort_name": "float",
        "lang_sort_key": "rust:Float",
        "target_concept_cid": CONCEPT_FLOAT_CID,
        "target_concept_label": "Float",
        "direction": "bidirectional",
        "precision_loss": "none",
        "range_loss": "none",
        "runtime_guards": [],
        "note": (
            "Rust's `Float` sort covers `f32` and `f64` (IEEE 754). Per-precision "
            "distinction is deferred to a future FloatPrecision dimension."
        ),
    },
    {
        "lang_label": "java",
        "lang_sort_name": "real",
        "lang_sort_key": "java:Real",
        "target_concept_cid": CONCEPT_FLOAT_CID,
        "target_concept_label": "Float",
        "direction": "bidirectional",
        "precision_loss": "none",
        "range_loss": "none",
        "runtime_guards": [],
        "note": (
            "Java's `real` sort covers `float` and `double` under one polymorphic "
            "real sort. Per-precision distinction deferred to FloatPrecision."
        ),
    },
    {
        "lang_label": "go",
        "lang_sort_name": "real",
        "lang_sort_key": "go:Real",
        "target_concept_cid": CONCEPT_FLOAT_CID,
        "target_concept_label": "Float",
        "direction": "bidirectional",
        "precision_loss": "none",
        "range_loss": "none",
        "runtime_guards": [],
        "note": (
            "Go's `real` sort covers `float32` and `float64`. Per-precision deferred."
        ),
    },
    {
        "lang_label": "php",
        "lang_sort_name": "real",
        "lang_sort_key": "php:Real",
        "target_concept_cid": CONCEPT_FLOAT_CID,
        "target_concept_label": "Float",
        "direction": "bidirectional",
        "precision_loss": "none",
        "range_loss": "none",
        "runtime_guards": [],
        "note": "PHP's `real` sort is IEEE 754 64-bit (single float type).",
    },
    {
        "lang_label": "typescript",
        "lang_sort_name": "number",
        "lang_sort_key": "ts:Number",
        "target_concept_cid": CONCEPT_FLOAT_CID,
        "target_concept_label": "Float",
        "direction": "bidirectional",
        "precision_loss": "none",
        "range_loss": "none",
        "runtime_guards": [],
        "note": (
            "TypeScript's `Number` is IEEE 754 64-bit (single primitive numeric type). "
            "BigInt (a separate primitive) is not covered by this morphism."
        ),
    },
    {
        "lang_label": "c11",
        "lang_sort_name": "value",
        "lang_sort_key": "c11:Value",
        "target_concept_cid": CONCEPT_FLOAT_CID,
        "target_concept_label": "Float",
        "direction": "left-to-right",
        "precision_loss": "none",
        "range_loss": "narrowing",
        "runtime_guards": [
            {"kind": "is-float", "failure_mode": "refuse"}
        ],
        "note": (
            "C11 lacks a published typed Float sort; uses polymorphic `Value`. "
            "Narrowing from Value to Float requires a runtime is-float check; "
            "refuse on non-float Value. Mint a typed c11:Float sort and replace "
            "this morphism when finer precision discrimination is needed."
        ),
    },
    {
        "lang_label": "python",
        "lang_sort_name": "value",
        "lang_sort_key": "python:Value",
        "target_concept_cid": CONCEPT_FLOAT_CID,
        "target_concept_label": "Float",
        "direction": "left-to-right",
        "precision_loss": "none",
        "range_loss": "narrowing",
        "runtime_guards": [
            {"kind": "is-float", "failure_mode": "refuse"}
        ],
        "note": (
            "Python's `Value` covers all types; the morphism is narrowing-via-guard "
            "to substrate Float. Python's `float` is IEEE 754 64-bit; runtime "
            "guard checks `isinstance(v, float)` (or numeric tower equivalent)."
        ),
    },
    {
        "lang_label": "ruby",
        "lang_sort_name": "value",
        "lang_sort_key": "ruby:Value",
        "target_concept_cid": CONCEPT_FLOAT_CID,
        "target_concept_label": "Float",
        "direction": "left-to-right",
        "precision_loss": "none",
        "range_loss": "narrowing",
        "runtime_guards": [
            {"kind": "is-float", "failure_mode": "refuse"}
        ],
        "note": (
            "Ruby's `Value` covers all objects; narrowing-via-guard to substrate "
            "Float. Ruby's `Float` is IEEE 754 64-bit; runtime guard checks "
            "`v.is_a?(Float)`."
        ),
    },
    # ===== Null morphisms (7 languages; skip Rust per null-free posture) =====
    {
        "lang_label": "typescript",
        "lang_sort_name": "null",
        "lang_sort_key": "ts:Null",
        "target_concept_cid": CONCEPT_NULL_CID,
        "target_concept_label": "Null",
        "direction": "bidirectional",
        "precision_loss": "none",
        "range_loss": "none",
        "runtime_guards": [],
        "note": (
            "TypeScript's `Null` sort denotes the single-value null primitive. "
            "Bidirectional with substrate Null; `undefined` is a distinct sort "
            "and would mint its own morphism if Trinity surfaces the distinction."
        ),
    },
    {
        "lang_label": "java",
        "lang_sort_name": "ref",
        "lang_sort_key": "java:Ref",
        "target_concept_cid": CONCEPT_NULL_CID,
        "target_concept_label": "Null",
        "direction": "left-to-right",
        "precision_loss": "none",
        "range_loss": "narrowing",
        "runtime_guards": [
            {"kind": "is-null", "failure_mode": "refuse"}
        ],
        "note": (
            "Java's `null` is a value of any reference type. Narrowing from "
            "java:Ref to substrate Null requires a runtime null check. Refuse "
            "on non-null Ref."
        ),
    },
    {
        "lang_label": "go",
        "lang_sort_name": "ref",
        "lang_sort_key": "go:Ref",
        "target_concept_cid": CONCEPT_NULL_CID,
        "target_concept_label": "Null",
        "direction": "left-to-right",
        "precision_loss": "none",
        "range_loss": "narrowing",
        "runtime_guards": [
            {"kind": "is-nil", "failure_mode": "refuse"}
        ],
        "note": (
            "Go's `nil` is a value of any reference type (pointer, slice, map, "
            "channel, function, interface). Narrowing from go:Ref to substrate "
            "Null requires a runtime nil check."
        ),
    },
    {
        "lang_label": "c11",
        "lang_sort_name": "value",
        "lang_sort_key": "c11:Value",
        "target_concept_cid": CONCEPT_NULL_CID,
        "target_concept_label": "Null",
        "direction": "left-to-right",
        "precision_loss": "none",
        "range_loss": "narrowing",
        "runtime_guards": [
            {"kind": "is-null-pointer", "failure_mode": "refuse"}
        ],
        "note": (
            "C11's NULL is a null pointer constant; narrowing from c11:Value to "
            "substrate Null requires a runtime null-pointer check. Mint a typed "
            "c11:Null sort when finer C-specific null semantics are needed."
        ),
    },
    {
        "lang_label": "php",
        "lang_sort_name": "value",
        "lang_sort_key": "php:Value",
        "target_concept_cid": CONCEPT_NULL_CID,
        "target_concept_label": "Null",
        "direction": "left-to-right",
        "precision_loss": "none",
        "range_loss": "narrowing",
        "runtime_guards": [
            {"kind": "is-null", "failure_mode": "refuse"}
        ],
        "note": (
            "PHP's `null` is a value of the polymorphic Value sort. Narrowing "
            "via runtime `is_null($v)` check."
        ),
    },
    {
        "lang_label": "python",
        "lang_sort_name": "value",
        "lang_sort_key": "python:Value",
        "target_concept_cid": CONCEPT_NULL_CID,
        "target_concept_label": "Null",
        "direction": "left-to-right",
        "precision_loss": "none",
        "range_loss": "narrowing",
        "runtime_guards": [
            {"kind": "is-none", "failure_mode": "refuse"}
        ],
        "note": (
            "Python's `None` is the singleton NoneType instance; narrowing from "
            "python:Value to substrate Null via runtime `v is None` check."
        ),
    },
    {
        "lang_label": "ruby",
        "lang_sort_name": "value",
        "lang_sort_key": "ruby:Value",
        "target_concept_cid": CONCEPT_NULL_CID,
        "target_concept_label": "Null",
        "direction": "left-to-right",
        "precision_loss": "none",
        "range_loss": "narrowing",
        "runtime_guards": [
            {"kind": "is-nil", "failure_mode": "refuse"}
        ],
        "note": (
            "Ruby's `nil` is the singleton NilClass instance; narrowing from "
            "ruby:Value to substrate Null via runtime `v.nil?` check."
        ),
    },
]


def jcs_canonical(value: object) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def blake3_512_of_bytes(data: bytes) -> str:
    try:
        from blake3 import blake3 as _blake3
    except ImportError as exc:
        raise SystemExit(
            "mint_sort_morphisms_to_concept_hub.py: missing 'blake3' package.\n"
            "  Install with: pip install blake3"
        ) from exc
    digest = _blake3(data).digest(length=64)
    return f"blake3-512:{digest.hex()}"


def build_morphism(spec: dict[str, Any]) -> tuple[str, str, Path]:
    """Mint one morphism per spec. Returns (lang_label, cid, file_path)."""
    header: dict[str, Any] = {
        "cid": "",
        "direction": spec["direction"],
        "kind": "sort-morphism",
        "precision_loss": spec["precision_loss"],
        "range_loss": spec["range_loss"],
        "representation_constraints": [],
        "runtime_guards": spec["runtime_guards"],
        "schemaVersion": "1",
        "source_language_signature_cid": LANG_SIG[spec["lang_label"]],
        "source_sort_cid": SORT_CIDS[spec["lang_sort_key"]],
        "target_language_signature_cid": CONCEPT_HUB_SIG_CID,
        "target_sort_cid": spec["target_concept_cid"],
    }
    metadata: dict[str, Any] = {"note": spec["note"]}
    cid_input = {
        "header": {k: v for k, v in header.items() if k != "cid"},
        "metadata": metadata,
    }
    cid = blake3_512_of_bytes(jcs_canonical(cid_input).encode("utf-8"))
    header["cid"] = cid

    filename = (
        f"sort-morphism:{spec['lang_label']}:{spec['lang_sort_name']}"
        f":to:concept:{spec['target_concept_label']}.{cid}.json"
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
    return spec["lang_label"], cid, out_path


def update_cids_tsv(entries: list[tuple[str, str, str, Path]]) -> None:
    """Append/update sort-morphism rows in cids.tsv. Idempotent.

    entries: list of (lang, sort_name, target_concept_label, cid, path) tuples
    (we accept 4-tuple via parameter typing simplification: see caller).
    """
    lines = CIDS_TSV.read_text(encoding="utf-8").splitlines(keepends=True)
    name_to_new: dict[str, str] = {}
    for lang, sort_name, target, cid, path in entries:
        name = f"sort_morphism_{lang}_{sort_name}_to_concept_{target.lower()}"
        rel = path.relative_to(ROOT).as_posix()
        name_to_new[name] = f"sort-morphism\t{name}\t{cid}\t{rel}\n"
    new_lines: list[str] = []
    seen = set()
    for line in lines:
        parts = line.split("\t")
        if len(parts) >= 2 and parts[1] in name_to_new:
            new_lines.append(name_to_new[parts[1]])
            seen.add(parts[1])
        else:
            new_lines.append(line)
    for name, new_line in name_to_new.items():
        if name in seen:
            continue
        if new_lines and not new_lines[-1].endswith("\n"):
            new_lines[-1] = new_lines[-1] + "\n"
        new_lines.append(new_line)
    CIDS_TSV.write_text("".join(new_lines), encoding="utf-8")


def main() -> None:
    minted: list[tuple[str, str, str, str, Path]] = []
    float_count = 0
    null_count = 0
    for spec in MORPHISMS:
        lang, cid, path = build_morphism(spec)
        target_label = spec["target_concept_label"]
        sort_name = spec["lang_sort_name"]
        minted.append((lang, sort_name, target_label, cid, path))
        if target_label == "Float":
            float_count += 1
        elif target_label == "Null":
            null_count += 1
        print(f"  {lang}:{sort_name} -> concept:{target_label} = {cid}")
    update_cids_tsv(minted)
    print(
        f"Minted {len(minted)} sort-morphisms ({float_count} Float + "
        f"{null_count} Null); cids.tsv updated."
    )
    print(
        "Deferred (no source sort available): csharp + zig "
        "(language signatures published but no Value/Float/Null sort yet)."
    )


if __name__ == "__main__":
    main()
