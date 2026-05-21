#!/usr/bin/env python3
"""Mint the 38 missing per-language SortMementos that the sort-morphism gap
audit identified as blockers.

Walks (language, primitive) pairs where:
- The substrate-canonical primitive sort exists in
  menagerie/concept-shapes/catalog/sorts/.
- The per-language sort with the corresponding name does NOT exist in
  menagerie/<lang>-language-signature/catalog/sorts/.
- The sort-morphism-gap memento classified the gap as could-be (i.e.
  the language HAS a clean realization, just not minted).

For each such pair, mints a SortMemento with `kind: sort-description`
matching the shape of existing language sorts (e.g. csharp:Bool).

After this script runs, mint_remaining_sort_morphisms.py can be re-run
to close the corresponding 38 sort-morphism gaps.

Usage:
    python3 mint_missing_lang_sorts.py
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

BASE = Path(__file__).resolve().parents[1]
ROOT = BASE.parents[1]

DECLARED_AT = "2026-05-21T00:00:00Z"

# (language, primitive_name, lang_sort_name, description)
# Mints lang_sort_name as a SortMemento under <lang>:<lang_sort_name>.
# Most use lang_sort_name == primitive; aliases (typescript:Number etc.) handled separately.
MISSING: list[tuple[str, str, str, str]] = [
    # csharp gaps (9)
    ("csharp", "Bytes", "Bytes", "C# byte sequences (byte[] / Span<byte> / ReadOnlySpan<byte>)."),
    ("csharp", "Cid", "Cid", "C# content-addressed identifier (string-formatted hash). Realized as System.String with cid-format constraint."),
    ("csharp", "EffectName", "EffectName", "C# effect identifier (string-formatted). Realized as System.String."),
    ("csharp", "Float", "Float", "C# IEEE 754 floating-point (float / double / decimal). Polymorphic over precision."),
    ("csharp", "Formula", "Formula", "C# substrate-IR formula (kit's own AST type for proofs/formulas)."),
    ("csharp", "Null", "Null", "C# null literal — applies to reference types and nullable value types (T?)."),
    ("csharp", "OpCid", "OpCid", "C# operator content-id (string-formatted hash for an op declaration). Subtype of csharp:Cid."),
    ("csharp", "SortCid", "SortCid", "C# sort content-id (string-formatted hash for a sort declaration). Subtype of csharp:Cid."),
    ("csharp", "Term", "Term", "C# substrate-IR term (kit's own AST type for term-shape proofs)."),
    # go gaps (6)
    ("go", "Bytes", "Bytes", "Go byte sequences ([]byte / []uint8)."),
    ("go", "Cid", "Cid", "Go content-addressed identifier (string-formatted hash). Realized as string with cid-format constraint."),
    ("go", "EffectName", "EffectName", "Go effect identifier (string-formatted). Realized as string."),
    ("go", "Formula", "Formula", "Go substrate-IR formula (kit's own AST type)."),
    ("go", "OpCid", "OpCid", "Go operator content-id (string-formatted). Subtype of go:Cid."),
    ("go", "SortCid", "SortCid", "Go sort content-id (string-formatted). Subtype of go:Cid."),
    # java gaps (7)
    ("java", "Bytes", "Bytes", "Java byte sequences (byte[] — signed 8-bit primitive). Range reinterpretation via & 0xFF for unsigned semantics."),
    ("java", "Cid", "Cid", "Java content-addressed identifier (string-formatted hash). Realized as java.lang.String with cid-format constraint."),
    ("java", "EffectName", "EffectName", "Java effect identifier (string-formatted). Realized as java.lang.String."),
    ("java", "Formula", "Formula", "Java substrate-IR formula (kit's own AST type)."),
    ("java", "OpCid", "OpCid", "Java operator content-id. Subtype of java:Cid."),
    ("java", "SortCid", "SortCid", "Java sort content-id. Subtype of java:Cid."),
    ("java", "Term", "Term", "Java substrate-IR term (kit's own AST type)."),
    # typescript gaps (7)
    ("typescript", "Bytes", "Bytes", "TypeScript byte sequences (Uint8Array / ArrayBuffer / Buffer in Node)."),
    ("typescript", "Cid", "Cid", "TypeScript content-addressed identifier (string-formatted). Realized as string with cid-format constraint."),
    ("typescript", "EffectName", "EffectName", "TypeScript effect identifier (string-formatted). Realized as string."),
    ("typescript", "Formula", "Formula", "TypeScript substrate-IR formula (kit's own AST type)."),
    ("typescript", "OpCid", "OpCid", "TypeScript operator content-id. Subtype of typescript:Cid."),
    ("typescript", "SortCid", "SortCid", "TypeScript sort content-id. Subtype of typescript:Cid."),
    ("typescript", "Term", "Term", "TypeScript substrate-IR term (kit's own AST type)."),
    # zig gaps (9)
    ("zig", "Bytes", "Bytes", "Zig byte sequences ([]const u8 / []u8)."),
    ("zig", "Cid", "Cid", "Zig content-addressed identifier (string-formatted). Realized as []const u8 with cid-format constraint."),
    ("zig", "EffectName", "EffectName", "Zig effect identifier (string-formatted). Realized as []const u8."),
    ("zig", "Float", "Float", "Zig IEEE 754 floating-point (f16 / f32 / f64 / f80 / f128)."),
    ("zig", "Formula", "Formula", "Zig substrate-IR formula (kit's own AST type)."),
    ("zig", "Null", "Null", "Zig null literal — applies to optional types (?T)."),
    ("zig", "OpCid", "OpCid", "Zig operator content-id. Subtype of zig:Cid."),
    ("zig", "SortCid", "SortCid", "Zig sort content-id. Subtype of zig:Cid."),
    ("zig", "Term", "Term", "Zig substrate-IR term (kit's own AST type)."),
]


def jcs_canonical(value: object) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def blake3_512_of_bytes(data: bytes) -> str:
    from blake3 import blake3 as _blake3
    digest = _blake3(data).digest(length=64)
    return f"blake3-512:{digest.hex()}"


def build_sort(lang: str, primitive: str, lang_sort_name: str, description: str) -> tuple[str, Path]:
    fn_name = f"{lang}:{lang_sort_name}"
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
            "name": lang_sort_name,
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
    filename = f"{fn_name}.{cid}.json"
    out_path = sorts_dir / filename
    content = json.dumps(envelope, indent=2, sort_keys=True, ensure_ascii=False) + "\n"
    out_path.write_text(content, encoding="utf-8")
    return cid, out_path


def already_present(lang: str, lang_sort_name: str) -> bool:
    sorts_dir = ROOT / "menagerie" / f"{lang}-language-signature" / "catalog" / "sorts"
    if not sorts_dir.is_dir():
        return False
    prefix = f"{lang}:{lang_sort_name}."
    for fn in sorts_dir.iterdir():
        if fn.name.startswith(prefix):
            return True
    return False


def main() -> int:
    written = []
    skipped = []
    for lang, primitive, lang_sort_name, description in MISSING:
        if already_present(lang, lang_sort_name):
            skipped.append((lang, lang_sort_name))
            continue
        cid, path = build_sort(lang, primitive, lang_sort_name, description)
        written.append((lang, lang_sort_name, cid, path))

    print(f"=== Missing lang-sort mint summary ===")
    print(f"Specified: {len(MISSING)}")
    print(f"Already present: {len(skipped)}")
    print(f"Newly minted: {len(written)}")
    for lang, name, cid, path in written:
        print(f"  {lang}:{name} → {cid[:35]}...")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
