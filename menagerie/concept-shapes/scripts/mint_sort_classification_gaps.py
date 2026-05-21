#!/usr/bin/env python3
"""Mint SortMorphismGap mementos for unanswered sort-classification exam questions.

Per the substrate-honest principle: if a primitive sort has a clean realization
in a language but the morphism has not yet been minted, that gap MUST be a loss
report — content-addressed, signed, citable. Silent absence is not acceptable;
the substrate needs to KNOW what's missing so it can be cited from blocked
boundaries and so the mint path is named.

This script enumerates:
- 17 substrate-canonical primitive sorts the exam asks about (only those with
  a published SortMemento in catalog/sorts/ — 14 of 17; the 3 Exam* meta sorts
  are deferred until their own minting).
- 10 target languages (c11, csharp, go, java, php, python, ruby, rust,
  typescript, zig).
- 15 already-minted sort-morphisms (Float across 8 langs, Null across 7 langs)
  which are SKIPPED.

For every remaining (language, primitive) gap, mints a sort-morphism-gap memento
with classification + mint_path + blocked_boundaries.

Classification:
- could-be: the language has a clean realization but the morphism isn't minted
  yet. The mint_path names which sort to mint and what loss profile to expect.
- cannot-be: the language can't realize this primitive without bounded loss.
  The mint_path names the loss dimensions that need to be declared.

Per the boundaries → exam → primitives stack (per the 2026-05-21 discussion):
- sugar = library-vendored concepts (top layer, what we've already wired)
- boundaries = contract-pinned handoffs
- EXAM = primitive sort + concept morphisms per language (THIS IS THE SUBSTRATE)

Cross-language materialize can't compose signatures across kits until the
exam-declared primitive translations are complete. This script declares the
gaps so the substrate knows what work remains.

Usage:
    python3 mint_sort_classification_gaps.py
"""

from __future__ import annotations

import json
import os
from pathlib import Path
from typing import Any


BASE = Path(__file__).resolve().parents[1]
ALGORITHMS_DIR = BASE / "catalog" / "algorithms"

CONCEPT_HUB_SIG_CID = (
    "blake3-512:1979babed41ad51ad8d7a28543815f74e24a9d4ee1ae3d52ccc6549f293aa635"
    "19abf5411a67b7882c73333b1b357e4863f6d7781f0b0776e5bd25f90ea7d793"
)

DECLARED_AT = "2026-05-21T00:00:00Z"
PLACEHOLDER_SIGNER = "ed25519:UNSIGNED_DEV_ONLY"
PLACEHOLDER_SIGNATURE = "ed25519:UNSIGNED_DEV_ONLY"

# Per-language signature CIDs.
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


def discover_substrate_sort_cids() -> dict[str, str]:
    """Walk catalog/sorts/ and return {SortName: cid} for primitive sorts."""
    sorts_dir = BASE / "catalog" / "sorts"
    sort_cids: dict[str, str] = {}
    for fn in sorted(sorts_dir.iterdir()):
        if not fn.name.endswith(".json"):
            continue
        if ".blake3-512:" not in fn.name:
            continue
        base, rest = fn.name.split(".blake3-512:", 1)
        cid = "blake3-512:" + rest.rsplit(".json", 1)[0]
        sort_cids[base] = cid
    return sort_cids


def discover_minted_morphisms() -> set[tuple[str, str]]:
    """Return set of (language, target_concept_label) pairs already minted."""
    minted: set[tuple[str, str]] = set()
    for fn in sorted(ALGORITHMS_DIR.iterdir()):
        if not fn.name.startswith("sort-morphism:"):
            continue
        head = fn.name.split(".blake3-512:")[0]
        parts = head.split(":")
        # ["sort-morphism", lang, src_sort, "to", "concept", dst_sort]
        if len(parts) >= 6:
            minted.add((parts[1], parts[5]))
    return minted


# Per-language classification + mint_path templates for each primitive.
# Each entry: (classification, note, mint_path).
# classification: "could-be" or "cannot-be".
GAP_PROFILE = {
    # ===== Bool =====
    ("c11", "Bool"): ("could-be",
        "C11 has `_Bool` (and `bool` via stdbool.h). Clean realization.",
        "Mint c11:Bool sort + sort-morphism:c11:bool:to:concept:Bool with direction=bidirectional, no loss."),
    ("csharp", "Bool"): ("could-be",
        "C# has `bool` (System.Boolean). Clean realization.",
        "Mint csharp:Bool sort + sort-morphism:csharp:bool:to:concept:Bool with direction=bidirectional, no loss."),
    ("go", "Bool"): ("could-be",
        "Go has `bool`. Clean realization.",
        "Mint go:Bool sort + sort-morphism:go:bool:to:concept:Bool with direction=bidirectional, no loss."),
    ("java", "Bool"): ("could-be",
        "Java has `boolean` (primitive) and `Boolean` (boxed). Clean realization on primitive.",
        "Mint java:Bool sort (primitive) + sort-morphism:java:bool:to:concept:Bool with direction=bidirectional, no loss."),
    ("php", "Bool"): ("could-be",
        "PHP has `bool` (with type-juggling polymorphism). Clean typed realization.",
        "Mint php:Bool sort + sort-morphism:php:bool:to:concept:Bool with direction=bidirectional. Note: PHP loose comparison treats 0/'0'/null as bool false — declare runtime_guard=strict-bool-check for narrowing morphisms from php:Value."),
    ("python", "Bool"): ("could-be",
        "Python has `bool` (a subclass of int). Clean realization.",
        "Mint python:Bool sort + sort-morphism:python:bool:to:concept:Bool with direction=bidirectional. Note: bool ⊂ int in python so morphisms TO bool from value should declare runtime_guard=isinstance-bool-check (excluding 0/1 from int)."),
    ("ruby", "Bool"): ("could-be",
        "Ruby has TrueClass + FalseClass (no shared Bool superclass below Object). Realization via union.",
        "Mint ruby:Bool sort (union of TrueClass/FalseClass) + sort-morphism:ruby:bool:to:concept:Bool, runtime_guard=true-or-false-check for narrowing from ruby:Value."),
    ("rust", "Bool"): ("could-be",
        "Rust has `bool`. Clean realization.",
        "Mint rust:Bool sort + sort-morphism:rust:bool:to:concept:Bool with direction=bidirectional, no loss."),
    ("typescript", "Bool"): ("could-be",
        "TypeScript has `boolean`. Clean realization.",
        "Mint typescript:Bool sort + sort-morphism:typescript:bool:to:concept:Bool with direction=bidirectional, no loss."),
    ("zig", "Bool"): ("could-be",
        "Zig has `bool`. Clean realization.",
        "Mint zig:Bool sort + sort-morphism:zig:bool:to:concept:Bool with direction=bidirectional, no loss."),

    # ===== Int =====
    ("c11", "Int"): ("could-be",
        "C11 has int family (int, long, short, char) + <stdint.h> typedefs (int8_t..int64_t, uint8_t..uint64_t). Polymorphic over width.",
        "Mint c11:Int sort with width-family metadata + sort-morphism:c11:int:to:concept:Int. Width specialization (i32 vs i64) is platform-implementation per #1363."),
    ("csharp", "Int"): ("could-be",
        "C# has int/long/short/byte (signed + unsigned variants) + BigInteger for unbounded.",
        "Mint csharp:Int sort + sort-morphism:csharp:int:to:concept:Int. Standard widths covered; BigInteger is separate sort if/when needed."),
    ("go", "Int"): ("could-be",
        "Go has int/int8/int16/int32/int64 + uint variants. No native bignum (math/big lib).",
        "Mint go:Int sort with width-family metadata + sort-morphism:go:int:to:concept:Int."),
    ("java", "Int"): ("could-be",
        "Java has byte/short/int/long primitives + Integer/Long boxed + BigInteger.",
        "Mint java:Int sort + sort-morphism:java:int:to:concept:Int. Standard widths; BigInteger separate."),
    ("php", "Int"): ("could-be",
        "PHP has `int` (platform-dependent 32 or 64-bit) + GMP for arbitrary precision.",
        "Mint php:Int sort with width=PHP_INT_SIZE metadata + sort-morphism:php:int:to:concept:Int."),
    ("python", "Int"): ("could-be",
        "Python has unbounded `int` (PEP 237). No width loss within concept:Int.",
        "Mint python:Int sort + sort-morphism:python:int:to:concept:Int with direction=bidirectional, range_loss=none (unbounded)."),
    ("ruby", "Int"): ("could-be",
        "Ruby has unbounded `Integer` (auto-promotes Fixnum → Bignum). No width loss.",
        "Mint ruby:Int sort + sort-morphism:ruby:int:to:concept:Int with direction=bidirectional, range_loss=none."),
    ("rust", "Int"): ("could-be",
        "Rust has i8/i16/i32/i64/i128/isize + u8/u16/u32/u64/u128/usize. 12 platform-implementation widths.",
        "Mint rust:Int sort with width-family metadata + sort-morphism:rust:int:to:concept:Int. Width specialization (i64 vs i32) is platform-implementation per #1363 integer_width annotation."),
    ("typescript", "Int"): ("cannot-be",
        "TypeScript number is IEEE 754 64-bit float — integer range bounded by 2^53-1 (Number.MAX_SAFE_INTEGER). For unbounded Int, must use bigint (separate primitive).",
        "Mint typescript:Int sort as 53-bit-safe + sort-morphism:typescript:number:to:concept:Int with precision_loss=none, range_loss=2^53-bounded. For full concept:Int coverage, mint a separate typescript:bigint:to:concept:Int morphism with direction=left-to-right."),
    ("zig", "Int"): ("could-be",
        "Zig has comptime_int (arbitrary precision at compile time) + sized i8..i65535 + u8..u65535.",
        "Mint zig:Int sort + sort-morphism:zig:int:to:concept:Int with width-family metadata."),

    # ===== String =====
    ("c11", "String"): ("cannot-be",
        "C has no native String value-type — only `const char*` with manual length tracking. UTF-8 is convention not type.",
        "Mint c11:String sort as char-pointer-with-length-tuple + sort-morphism:c11:string:to:concept:String with loss dimensions: encoding-undeclared, length-via-strlen-O(n), no-immutability-guarantee. Runtime guards: utf8-validity-check."),
    ("csharp", "String"): ("could-be",
        "C# has `string` (System.String, UTF-16 internally). Clean realization.",
        "Mint csharp:String sort + sort-morphism:csharp:string:to:concept:String. Note: UTF-16 internal encoding (lossless re-encoding to UTF-8 at substrate boundary)."),
    ("go", "String"): ("could-be",
        "Go has `string` (immutable byte sequence, UTF-8 by convention).",
        "Mint go:String sort + sort-morphism:go:string:to:concept:String with direction=bidirectional. Note: Go strings can contain invalid UTF-8; declare runtime_guard=utf8-validity-check for strict-UTF-8 morphisms."),
    ("java", "String"): ("could-be",
        "Java has `String` (UTF-16 internally, immutable).",
        "Mint java:String sort + sort-morphism:java:string:to:concept:String. Same UTF-16-to-UTF-8 conversion at substrate boundary."),
    ("php", "String"): ("could-be",
        "PHP has `string` (byte sequence — encoding is convention).",
        "Mint php:String sort + sort-morphism:php:string:to:concept:String, runtime_guard=mb_check_encoding for strict-UTF-8 morphisms."),
    ("python", "String"): ("could-be",
        "Python 3 has `str` (Unicode code points). Clean realization.",
        "Mint python:String sort + sort-morphism:python:str:to:concept:String with direction=bidirectional, no loss."),
    ("ruby", "String"): ("could-be",
        "Ruby has `String` (byte sequence + encoding tag).",
        "Mint ruby:String sort + sort-morphism:ruby:string:to:concept:String with direction=bidirectional. Note: Ruby's per-string encoding tag handles UTF-8 cleanly."),
    ("rust", "String"): ("could-be",
        "Rust has String (owned UTF-8) + &str (borrowed UTF-8). Two-way realization.",
        "Mint rust:String sort + sort-morphism:rust:string:to:concept:String with direction=bidirectional, no loss. Note: &str vs String is borrow-distinction, not separate primitive."),
    ("typescript", "String"): ("could-be",
        "TypeScript has `string` (UTF-16 internally).",
        "Mint typescript:String sort + sort-morphism:typescript:string:to:concept:String. UTF-16-to-UTF-8 at substrate boundary."),
    ("zig", "String"): ("cannot-be",
        "Zig has no native String value-type — `[]const u8` is the convention, with no encoding declaration.",
        "Mint zig:String sort as []const u8 + sort-morphism:zig:string:to:concept:String with loss dimensions: encoding-undeclared, no-immutability-guarantee."),

    # ===== Bytes =====
    ("c11", "Bytes"): ("could-be",
        "C has uint8_t* + length. The substrate-canonical Bytes is byte-array-with-length-tuple.",
        "Mint c11:Bytes sort as (uint8_t*, size_t) tuple + sort-morphism:c11:bytes:to:concept:Bytes with no encoding-claim."),
    ("csharp", "Bytes"): ("could-be",
        "C# has byte[]. Clean realization.",
        "Mint csharp:Bytes sort + sort-morphism:csharp:byte-array:to:concept:Bytes with direction=bidirectional."),
    ("go", "Bytes"): ("could-be",
        "Go has []byte (alias for []uint8). Clean realization.",
        "Mint go:Bytes sort + sort-morphism:go:bytes:to:concept:Bytes."),
    ("java", "Bytes"): ("could-be",
        "Java has byte[] (signed 8-bit). Range loss on byte-value-as-uint8 morphism.",
        "Mint java:Bytes sort + sort-morphism:java:byte-array:to:concept:Bytes with runtime_guard=signed-to-unsigned-reinterpret."),
    ("php", "Bytes"): ("could-be",
        "PHP uses `string` for byte sequences (no separate Bytes type).",
        "Mint php:Bytes as alias for php:String + sort-morphism:php:string:to:concept:Bytes. Lossy only if encoding tag is set."),
    ("python", "Bytes"): ("could-be",
        "Python has `bytes` (immutable byte sequence). Clean realization.",
        "Mint python:Bytes sort + sort-morphism:python:bytes:to:concept:Bytes with direction=bidirectional, no loss."),
    ("ruby", "Bytes"): ("could-be",
        "Ruby has String with ASCII-8BIT (BINARY) encoding for bytes.",
        "Mint ruby:Bytes sort + sort-morphism:ruby:bytes:to:concept:Bytes."),
    ("rust", "Bytes"): ("could-be",
        "Rust has Vec<u8> + &[u8]. Clean realization.",
        "Mint rust:Bytes sort + sort-morphism:rust:bytes:to:concept:Bytes with direction=bidirectional."),
    ("typescript", "Bytes"): ("could-be",
        "TypeScript has Uint8Array. Clean realization.",
        "Mint typescript:Bytes sort + sort-morphism:typescript:uint8-array:to:concept:Bytes."),
    ("zig", "Bytes"): ("could-be",
        "Zig has []const u8 / []u8.",
        "Mint zig:Bytes sort + sort-morphism:zig:bytes:to:concept:Bytes."),

    # ===== Cid =====  (content-addressed identifier — typically string-formatted)
    # All languages can realize Cid as String-with-format-constraint. Same shape across.
    **{(lang, "Cid"): ("could-be",
        f"Cid is a string-formatted hash (e.g. blake3-512:<hex>). {lang} realizes via its String type with a format constraint.",
        f"Mint {lang}:Cid sort as String-with-cid-format-pattern + sort-morphism:{lang}:cid:to:concept:Cid, runtime_guard=cid-format-regex-check.")
       for lang in ["c11", "csharp", "go", "java", "php", "python", "ruby", "rust", "typescript", "zig"]},

    # ===== OpCid + SortCid =====  (subtypes of Cid)
    **{(lang, sort): ("could-be",
        f"{sort} is a Cid subtype (string-formatted with op-/sort- prefix or context).",
        f"Mint {lang}:{sort} sort as subtype of {lang}:Cid + sort-morphism:{lang}:{sort.lower()}:to:concept:{sort}, runtime_guard=cid-format-regex-check + context-claim.")
       for lang in ["c11", "csharp", "go", "java", "php", "python", "ruby", "rust", "typescript", "zig"]
       for sort in ["OpCid", "SortCid"]},

    # ===== EffectName =====  (string-formatted effect identifier)
    **{(lang, "EffectName"): ("could-be",
        f"EffectName is a string identifier for an effect (e.g. 'IO', 'Async'). {lang} realizes via String.",
        f"Mint {lang}:EffectName sort as String-with-effect-name-pattern + sort-morphism:{lang}:effect-name:to:concept:EffectName.")
       for lang in ["c11", "csharp", "go", "java", "php", "python", "ruby", "rust", "typescript", "zig"]},

    # ===== Formula + Term =====  (substrate-internal — likely structural)
    **{(lang, sort): ("could-be",
        f"{sort} is a substrate-internal structural type (substrate IR-side). {lang} realizes via the kit's own AST/IR types.",
        f"Mint {lang}:{sort} sort as the kit's AST/IR equivalent + sort-morphism:{lang}:{sort.lower()}:to:concept:{sort} with direction=left-to-right (lift-only).")
       for lang in ["c11", "csharp", "go", "java", "php", "python", "ruby", "rust", "typescript", "zig"]
       for sort in ["Formula", "Term"]},

    # ===== List<T> + Map<K,V> =====  (parametric)
    **{(lang, "List<T>"): ("could-be",
        f"{lang} has a native list/array type. Parametric over T.",
        f"Mint {lang}:List<T> sort + sort-morphism:{lang}:list:to:concept:List<T>. Note: parametric — composes with T's morphism.")
       for lang in ["c11", "csharp", "go", "java", "php", "python", "ruby", "rust", "typescript", "zig"]},
    ("c11", "Map<K,V>"): ("cannot-be",
        "C11 has no native polymorphic map — requires struct-of-arrays + linear scan, or extern hash-table lib.",
        "Mint c11:Map<K,V> sort as struct-of-arrays + sort-morphism:c11:map:to:concept:Map<K,V> with loss dimensions: key-ordering-undefined, lookup-O(n), no-amortized-O(1)-guarantee."),
    ("zig", "Map<K,V>"): ("could-be",
        "Zig has std.AutoHashMap + std.StringHashMap. Clean realization.",
        "Mint zig:Map<K,V> sort + sort-morphism:zig:hashmap:to:concept:Map<K,V>."),
    **{(lang, "Map<K,V>"): ("could-be",
        f"{lang} has a native map/dict/hash type. Parametric.",
        f"Mint {lang}:Map<K,V> sort + sort-morphism:{lang}:map:to:concept:Map<K,V>. Parametric.")
       for lang in ["csharp", "go", "java", "php", "python", "ruby", "rust", "typescript"]},

    # Existing-minted (Float, Null) — skipped by minted_morphisms check, but
    # include profiles for those NOT yet covered in mint script (csharp, zig
    # for Float; csharp, rust, zig for Null) so future runs surface them.
    ("csharp", "Float"): ("could-be",
        "C# has float/double/decimal. Decimal is exact arbitrary-precision.",
        "Mint csharp:Float sort + sort-morphism:csharp:float:to:concept:Float."),
    ("zig", "Float"): ("could-be",
        "Zig has f16/f32/f64/f80/f128.",
        "Mint zig:Float sort with width-family + sort-morphism:zig:float:to:concept:Float."),
    ("csharp", "Null"): ("could-be",
        "C# has null (reference types) + nullable value types (T?).",
        "Mint csharp:Null sort + sort-morphism:csharp:null:to:concept:Null."),
    ("rust", "Null"): ("cannot-be",
        "Rust has no null — Option<T>::None is the null-equivalent, but it's a tagged variant not a unitary null. Per #1284 sort morphism script's exclusion.",
        "Mint rust:Null sort as Option<()>::None + sort-morphism:rust:none:to:concept:Null with loss dimension: not-a-singleton-but-a-variant + runtime_guard=match-on-none."),
    ("zig", "Null"): ("could-be",
        "Zig has `null` literal for optional types (T? = ?T).",
        "Mint zig:Null sort + sort-morphism:zig:null:to:concept:Null."),
}


def jcs_canonical(value: object) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def blake3_512_of_bytes(data: bytes) -> str:
    from blake3 import blake3 as _blake3
    digest = _blake3(data).digest(length=64)
    return f"blake3-512:{digest.hex()}"


def build_gap(language: str, target_concept_label: str, target_sort_cid: str,
              classification: str, note: str, mint_path: str) -> tuple[str, Path]:
    header: dict[str, Any] = {
        "cid": "",
        "kind": "sort-morphism-gap",
        "schemaVersion": "1",
        "source_language_signature_cid": LANG_SIG[language],
        "target_language_signature_cid": CONCEPT_HUB_SIG_CID,
        "target_sort_cid": target_sort_cid,
        "target_concept_label": target_concept_label,
        "classification": classification,
        "declared_at": "2026-05-21",
    }
    metadata: dict[str, Any] = {
        "note": note,
        "mint_path": mint_path,
        "blocked_boundaries": [
            "Cross-language materialize of any @boundary whose signature uses this primitive.",
        ],
    }
    cid_input = {
        "header": {k: v for k, v in header.items() if k != "cid"},
        "metadata": metadata,
    }
    cid = blake3_512_of_bytes(jcs_canonical(cid_input).encode("utf-8"))
    header["cid"] = cid

    safe_label = target_concept_label.replace("<", "_of_").replace(">", "").replace(",", "_")
    filename = f"sort-morphism-gap:{language}:to:concept:{safe_label}.{cid}.json"
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
    substrate_sorts = discover_substrate_sort_cids()
    minted = discover_minted_morphisms()
    written = []
    skipped_minted = []
    skipped_no_profile = []
    skipped_no_substrate_cid = []

    languages = sorted(LANG_SIG.keys())
    # Pull the list of substrate sorts the exam asks about. We use only sorts
    # that have CIDs in catalog/sorts/ — the 3 Exam* meta-sorts are deferred.
    exam_sorts = ["Bool", "Bytes", "Cid", "EffectName", "Float", "Formula", "Int",
                  "List<T>", "Map<K,V>", "Null", "OpCid", "SortCid", "String", "Term"]

    for lang in languages:
        for sort_label in exam_sorts:
            if sort_label not in substrate_sorts:
                skipped_no_substrate_cid.append((lang, sort_label))
                continue
            if (lang, sort_label) in minted:
                skipped_minted.append((lang, sort_label))
                continue
            profile = GAP_PROFILE.get((lang, sort_label))
            if profile is None:
                skipped_no_profile.append((lang, sort_label))
                continue
            classification, note, mint_path = profile
            cid, path = build_gap(
                language=lang,
                target_concept_label=sort_label,
                target_sort_cid=substrate_sorts[sort_label],
                classification=classification,
                note=note,
                mint_path=mint_path,
            )
            written.append((lang, sort_label, classification, cid, path))

    print(f"=== Sort-classification gap mint summary ===")
    print(f"Substrate sorts in exam: {len(exam_sorts)}")
    print(f"Languages: {len(languages)}")
    print(f"Already-minted morphisms (skipped): {len(skipped_minted)}")
    print(f"No-profile gaps (skipped — please add): {len(skipped_no_profile)}")
    print(f"No-substrate-sort-CID (skipped): {len(skipped_no_substrate_cid)}")
    print(f"Gaps written: {len(written)}")

    if skipped_no_profile:
        print("\nMISSING PROFILES:")
        for lang, sort in skipped_no_profile:
            print(f"  - ({lang!r}, {sort!r})")

    cb = sum(1 for _, _, c, _, _ in written if c == "could-be")
    cnb = sum(1 for _, _, c, _, _ in written if c == "cannot-be")
    print(f"\nClassification breakdown:")
    print(f"  could-be: {cb}")
    print(f"  cannot-be: {cnb}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
