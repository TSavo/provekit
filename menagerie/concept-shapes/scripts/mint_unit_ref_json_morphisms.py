#!/usr/bin/env python3
"""Mint per-language sort-morphisms for the 3 new substrate-canonical sorts
(concept:Unit, concept:Ref<T>, concept:Json) across all 10 federated languages.

These three sorts were minted 2026-05-21 to close gaps surfaced by the
libprovekit-rpc-cross-platform → java cross-language materialize demo. This
script mints the morphism mementos that declare "language X realizes
concept:<Sort> as lang:<sort>" — completing the substrate catalog the same
way mint_sort_morphisms_to_concept_hub.py + mint_remaining_sort_morphisms.py
+ mint_parametric_sort_morphisms.py did for the original 14 primitives.

Per-language realization choices:

  concept:Unit:
    c11:       void
    csharp:    void
    go:        (no explicit unit — Go's struct{} convention)
    java:      void
    php:       null (no separate Unit type)
    python:    None
    ruby:      nil
    rust:      ()
    typescript: void
    zig:       void

  concept:Ref<T>:
    c11:       T*
    csharp:    ref T
    go:        *T
    java:      AtomicReference<T> / mutable container (StringBuilder for String)
    php:       reference (&$var)
    python:    list-of-one (mutable wrapper)
    ruby:      mutable object
    rust:      &mut T
    typescript: { value: T }
    zig:       *T

  concept:Json:
    c11:       (no native — cJSON struct via cjson library, deferred)
    csharp:    JsonNode (System.Text.Json)
    go:        interface{} / map[string]interface{}
    java:      JsonNode (Jackson) — note: Gson realizes as JsonElement
    php:       array / stdClass
    python:    dict / list / scalar
    ruby:      Hash / Array / scalar
    rust:      serde_json::Value
    typescript: any / unknown
    zig:       (no native, deferred)
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

# Substrate-canonical CIDs for the 3 new sorts.
SORTS = {
    "Unit": "blake3-512:47682b09e5dba71f563db6249c6cb352f7d540986dc7f4cd8d4fb1aa6d9a503064033ee3eb9f36ee6f9e000f700f2f030ebfcfe2b2b8b7e81a345b0d56551f1b",
    "Ref<T>": "blake3-512:37d8efe0ce6321d1a16f80aa06cbdf056c846b8a99613731e8d64d9581af61bc517fd8c87daaff2c817585a7dfd763e09ed729fdc71d25fe16fb1b2e6ca33534",
    "Json": "blake3-512:702064722b23410fde0d1fd7afac165bf5914441d67abe1e19d63b0e8fe8117296d2677cc721ad096b8b3bb82d178af699bf14fd70bfb18756c5bed6f4434108",
}

# Per-(lang, substrate-sort) realization. Each entry:
#   (lang_sort_name, direction, precision_loss, range_loss, runtime_guards, note)
# Empty lang_sort_name means "lang doesn't yet have this sort minted" — skip.
PROFILES = {
    # Unit
    ("c11", "Unit"): ("Unit", "bidirectional", "none", "none", [],
        "C11's void return is the substrate-canonical Unit realization."),
    ("csharp", "Unit"): ("Unit", "bidirectional", "none", "none", [],
        "C#'s void return type."),
    ("go", "Unit"): ("Unit", "bidirectional", "none", "none", [],
        "Go's empty struct (struct{}) or absence of return value."),
    ("java", "Unit"): ("Unit", "bidirectional", "none", "none", [],
        "Java's void return type."),
    ("php", "Unit"): ("Unit", "left-to-right", "none", "narrowing", [
        {"kind": "is-null-check", "failure_mode": "refuse"}
    ],
        "PHP has no distinct Unit type; null serves as the singleton bottom element."),
    ("python", "Unit"): ("Unit", "bidirectional", "none", "none", [],
        "Python's None as the singleton Unit value."),
    ("ruby", "Unit"): ("Unit", "bidirectional", "none", "none", [],
        "Ruby's nil as the singleton Unit value."),
    ("rust", "Unit"): ("Unit", "bidirectional", "none", "none", [],
        "Rust's () unit type."),
    ("typescript", "Unit"): ("Unit", "bidirectional", "none", "none", [],
        "TypeScript's void return type."),
    ("zig", "Unit"): ("Unit", "bidirectional", "none", "none", [],
        "Zig's void return type."),
    # Ref<T> — parametric. Each kit's realization is its native mutable-reference idiom.
    ("c11", "Ref<T>"): ("Ref<T>", "bidirectional", "none", "none", [],
        "C11's T* pointer as parametric reference. Composes over inner T."),
    ("csharp", "Ref<T>"): ("Ref<T>", "bidirectional", "none", "none", [],
        "C# ref T parameter modifier. Composes over inner T."),
    ("go", "Ref<T>"): ("Ref<T>", "bidirectional", "none", "none", [],
        "Go's *T pointer. Composes over inner T."),
    ("java", "Ref<T>"): ("Ref<T>", "bidirectional", "none", "none", [],
        "Java's mutable-container idiom: AtomicReference<T> generically, StringBuilder for String. Composes."),
    ("php", "Ref<T>"): ("Ref<T>", "bidirectional", "none", "none", [],
        "PHP's reference parameter (&$var). Composes over inner T."),
    ("python", "Ref<T>"): ("Ref<T>", "bidirectional", "none", "none", [],
        "Python's list-of-one mutable wrapper pattern. Composes."),
    ("ruby", "Ref<T>"): ("Ref<T>", "bidirectional", "none", "none", [],
        "Ruby's mutable-object pattern (most objects are mutable). Composes."),
    ("rust", "Ref<T>"): ("Ref<T>", "bidirectional", "none", "none", [],
        "Rust's &mut T mutable reference. Composes over inner T."),
    ("typescript", "Ref<T>"): ("Ref<T>", "bidirectional", "none", "none", [],
        "TypeScript's { value: T } wrapper or mutable object property. Composes."),
    ("zig", "Ref<T>"): ("Ref<T>", "bidirectional", "none", "none", [],
        "Zig's *T pointer. Composes over inner T."),
    # Json
    ("c11", "Json"): ("Json", "left-to-right", "none", "narrowing", [
        {"kind": "cjson-validate", "failure_mode": "refuse"}
    ],
        "C11 has no native JSON sort; cJSON library or struct-tagged-union realization."),
    ("csharp", "Json"): ("Json", "bidirectional", "none", "none", [],
        "C#'s System.Text.Json.Nodes.JsonNode tree type."),
    ("go", "Json"): ("Json", "bidirectional", "none", "none", [],
        "Go's interface{} / map[string]interface{} convention (encoding/json package)."),
    ("java", "Json"): ("Json", "bidirectional", "none", "none", [],
        "Java's JsonNode (Jackson). Gson library realizes the same concept as JsonElement (separate morphism)."),
    ("php", "Json"): ("Json", "bidirectional", "none", "none", [],
        "PHP's associative array / stdClass via json_decode."),
    ("python", "Json"): ("Json", "bidirectional", "none", "none", [],
        "Python's dict/list/scalar via stdlib json module."),
    ("ruby", "Json"): ("Json", "bidirectional", "none", "none", [],
        "Ruby's Hash/Array/scalar via JSON module."),
    ("rust", "Json"): ("Json", "bidirectional", "none", "none", [],
        "Rust's serde_json::Value tree type."),
    ("typescript", "Json"): ("Json", "bidirectional", "none", "none", [],
        "TypeScript's typed JSON value (often `any` or library-typed)."),
    ("zig", "Json"): ("Json", "bidirectional", "none", "none", [],
        "Zig's std.json.Value tree."),
}


def jcs_canonical(value: object) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def blake3_512_of_bytes(data: bytes) -> str:
    from blake3 import blake3 as _blake3
    return f"blake3-512:{_blake3(data).digest(length=64).hex()}"


def lang_sort_cid(lang: str, sort_name: str) -> str | None:
    """Look up the lang's existing sort CID by name, or return None if missing."""
    d = ROOT / "menagerie" / f"{lang}-language-signature" / "catalog" / "sorts"
    if not d.is_dir():
        return None
    prefix = f"{lang}:{sort_name}."
    for fn in os.listdir(d):
        if fn.startswith(prefix):
            rest = fn[len(prefix):]
            cid = rest.rsplit(".json", 1)[0]
            return cid
    return None


def mint_lang_sort_if_missing(lang: str, sort_name: str, description: str) -> str:
    """Mint a new lang sort if it doesn't yet exist. Returns CID."""
    existing = lang_sort_cid(lang, sort_name)
    if existing:
        return existing
    fn_name = f"{lang}:{sort_name}"
    memento: dict[str, Any] = {
        "schema_version": "1",
        "protocol": "LSP",
        "kind": "SortMemento",
        "fn_name": fn_name,
        "formals": [],
        "formal_sorts": [],
        "pre": {"kind": "atomic", "name": "true", "args": []},
        "post": {"kind": "sort-description", "name": sort_name, "description": description},
        "effects": {"effects": []},
        "auto_minted_mementos": [],
        "return_sort": {"kind": "kind", "name": "*"},
    }
    cid = blake3_512_of_bytes(jcs_canonical(memento).encode("utf-8"))
    envelope = {
        "memento": memento,
        "cid": cid,
        "signature": {"alg": "ed25519", "key_id": "UNSIGNED_DEV_ONLY",
                      "sig_b64": "A" * 86 + "AA"},
    }
    d = ROOT / "menagerie" / f"{lang}-language-signature" / "catalog" / "sorts"
    d.mkdir(parents=True, exist_ok=True)
    (d / f"{fn_name}.{cid}.json").write_text(
        json.dumps(envelope, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    return cid


def existing_morphism(lang: str, sort_name: str) -> bool:
    safe = sort_name.replace("<", "_of_").replace(">", "").replace(",", "_")
    for fn in os.listdir(ALGORITHMS_DIR):
        if fn.startswith(f"sort-morphism:{lang}:") and f":to:concept:{safe}." in fn:
            return True
    return False


def build_morphism(lang: str, sort_name: str, profile: tuple,
                   substrate_sort_cid: str) -> tuple[str, Path]:
    lang_sort_name, direction, ploss, rloss, guards, note = profile
    lang_sort_label = f"{lang}:{lang_sort_name}"
    description = note + f" Realizes substrate concept:{sort_name}."
    src_sort_cid = mint_lang_sort_if_missing(lang, lang_sort_name, description)
    header: dict[str, Any] = {
        "cid": "",
        "direction": direction,
        "kind": "sort-morphism",
        "precision_loss": ploss,
        "range_loss": rloss,
        "representation_constraints": [],
        "runtime_guards": guards,
        "schemaVersion": "1",
        "source_language_signature_cid": LANG_SIG[lang],
        "source_sort_cid": src_sort_cid,
        "target_language_signature_cid": CONCEPT_HUB_SIG_CID,
        "target_sort_cid": substrate_sort_cid,
    }
    metadata = {"note": note}
    cid_input = {"header": {k: v for k, v in header.items() if k != "cid"}, "metadata": metadata}
    cid = blake3_512_of_bytes(jcs_canonical(cid_input).encode("utf-8"))
    header["cid"] = cid
    safe = sort_name.replace("<", "_of_").replace(">", "").replace(",", "_")
    fn = f"sort-morphism:{lang}:{lang_sort_name.lower().replace('<','_of_').replace('>','').replace(',','_')}:to:concept:{safe}.{cid}.json"
    out_path = ALGORITHMS_DIR / fn
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
    minted = 0
    skipped = 0
    for (lang, sort_name), profile in PROFILES.items():
        if existing_morphism(lang, sort_name):
            skipped += 1
            continue
        cid, _ = build_morphism(lang, sort_name, profile, SORTS[sort_name])
        minted += 1
        print(f"  {lang}:{profile[0]} → concept:{sort_name} ({cid[:30]}...)")
    print(f"\nMinted: {minted}, Skipped (already present): {skipped}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
