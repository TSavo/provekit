from __future__ import annotations

from typing import Any

from .realizer import _cid_of_json

Json = dict[str, Any]

PYTHON_PLATFORM_KIT_CID = (
    "blake3-512:"
    "bc36b43fec1a80efcecb05f8c4de725f961295466530aec452763c6c479b67c"
    "590c2e8062a3f46979383086ae80e6c0a917c443625d3474a7a89705e0a56ab8c"
)

PYTHON_PLATFORM_DIMENSIONS: tuple[tuple[str, str], ...] = (
    ("ArithmeticOverflow", "ArbitraryPrecision"),
    ("IntegerDivisionRounding", "Floor"),
    ("ShiftMode", "Arithmetic"),
    ("NullSemantics", "RaiseZeroDivisionError"),
    ("BitwiseSemantics", "TwosComplement"),
)

PYTHON_PLATFORM_CONCEPT_OP_CIDS: tuple[str, ...] = (
    "blake3-512:95fc70e63a5550fd2e25142f13932919c59d085654ab387789c798886b0111c61d28fe533fc98b50df70eea9428a9af8aa75372c8b1c1deb3acc1a4094790468",
    "blake3-512:b7c54558573348bb3a9297732547a8e6e9d152403d292df7426b6bb8a248f705b4b030bf2a22ba547a17d6f1bfaf8e75a6843e02e8f23a8226ebc09e2a8622af",
    "blake3-512:46cd627de058c8d4f7d087ea33f4904af65ad4b2e3cfd3aff8f44bf27db96b33c2dae39cd30f53898c233c9465ba8d2701c69e5903d48935113103b4db00fd03",
    "blake3-512:c6a13abbcafdf83edcff49d883a7c7440faadd8af896da0ad46e2bcb177ed0649d005b4ddecd4689cf565b10679219a07c784399bafe5c6174642e1b808d7839",
    "blake3-512:92340897b43965e01454b00a6a43ec54b2bf0e01213a45fa2311f730dde18adf8da97a22458c1a2a0fb23ce85ef3ad9b22e704804c74f41997aba3ba02cefe0d",
    "blake3-512:f9cdfcba8d0e223803126504a2a6ed10005fa61acb5c55b74b270bc66d963eb7648ab6763f0510760df93145c0f6670087a403417e8b3100c7142e121111807a",
    "blake3-512:c90e3c159b25e4c4c7f9c899da5aa3ee048a548719ced7360f3e514450811096b21cd5473f22d7a05df088f92210bbc916e65970b9fa1e1511c193ed969f112b",
    "blake3-512:9e96c2445bad6bb1e5a6f902ad7f733e3f4619829b9c0e232361fbf50b978c8332029212ed895762e604d1df009fce58848cda33524a697df798233eae30a14b",
    "blake3-512:d57b54bffe698ed804a4a49486b73a1a8a3e7bd84fb12babaad01ce22d8b7bcb5a35f3476324063f8de9f8090846d0d4fbeb48d78475d07e16f7925b4f264de3",
    "blake3-512:343b1f9faa98218467d810e0a2bb1b1eebeaf921c71a1bc52141f885220afff482c631c52e2157a6067640f4830f928add53ef7aa0386c6a27ee3c8bab6dc353",
    "blake3-512:5e788f0d551081f4e709e4418e01017fa9ae1c04963e7be2862fadad8a8434fafa204629fbec53e2e44624c195ac2e32c0410df25cf8ff3a4be672582f89109f",
    "blake3-512:ad958847b50cf07ddbb92d85ae488a5f983d5619e108476b42e519174cfcce883ecd637544a372b946bb45a1c22893c710bc9b08ea0569ad0e035b3babb6a409",
)

# concept:literal CID (from #1282)
CONCEPT_LITERAL_CID = "blake3-512:02804a0bdbd2d5d541544451f41ee8d0d340baf28f70bd5abf5844e87a96aedd7b5ab3453962754a020679cc8c6b3d1f4cf0336a7ad8118128d42ac667abf2d6"

# Canonical sort CIDs (from #1282)
# Python admits: Int, Float, String, Bool, Bytes, Null (full primitive tier)
# Args must be sorted alphabetically by CID string value:
#   BOOL  (0ee1...) < INT (30ff...) < NULL (62f6...) < BYTES (7116...) < FLOAT (b979...) < STRING (be87...)
_SORT_BOOL_CID = "blake3-512:0ee13bf3fd6b7ecfbee72dfbfc18a7c0ea7f1663de6cca43cefb36f5b4c03665452646094a7c296e819e75d683c6ce4821f3d7db3c3c78ae97f2d4e3451d2074"
_SORT_INT_CID = "blake3-512:30ffc51350121a7172f3e4064a33c45bbd345756979fccff6875cd2ab33e4964d098a99df80cfbdf1ec1a0738c5ac3476f0ff8f75589ea511d1acd82c74ecd58"
_SORT_NULL_CID = "blake3-512:62f6040bd3f414c1e6c2b7bdf276669cd5613b33cb508a81170170064ca3ffba771a4b0002dc52e059fce5f9f63a1874ef71bd4ec89ae06e89c87a3e91aac3b5"
_SORT_BYTES_CID = "blake3-512:7116ef6e62e6739b213a8394f975a53c771b89f08c36d27143827acfcfebc0e39e5b82c530be668c3cfd5ec6966ccaa42930b37fdb1f4ac25652a970be10fb6b"
_SORT_FLOAT_CID = "blake3-512:b979e70c4d5e53d9bdf13d6f08330be3c5b0714b8c770d69bbd05946b86c36df5274be8145a2683cc29c278155c9c1ee65b6897913524eecb9e4c89c71862f57"
_SORT_STRING_CID = "blake3-512:be8721d24849feb74c4721520bdba02d352a94f49253a627cd509127472aa1c47cbe99cb705cac4159b5365abcce0c9aaa4901fe67630827deb6be1f9daeea10"

# admits_sorts formula for Python (full primitive tier, sorted by CID string)
_ADMITS_SORTS_FORMULA: Json = {
    "args": [
        {"kind": "const", "sort": {"kind": "primitive", "name": "cid"}, "value": _SORT_BOOL_CID},
        {"kind": "const", "sort": {"kind": "primitive", "name": "cid"}, "value": _SORT_INT_CID},
        {"kind": "const", "sort": {"kind": "primitive", "name": "cid"}, "value": _SORT_NULL_CID},
        {"kind": "const", "sort": {"kind": "primitive", "name": "cid"}, "value": _SORT_BYTES_CID},
        {"kind": "const", "sort": {"kind": "primitive", "name": "cid"}, "value": _SORT_FLOAT_CID},
        {"kind": "const", "sort": {"kind": "primitive", "name": "cid"}, "value": _SORT_STRING_CID},
    ],
    "kind": "atomic",
    "name": "admits_sorts",
}


def dimension_values() -> list[Json]:
    existing = [
        _with_cid(
            {
                "compare_to": _compare_atom(value_name),
                "dimension_name": dimension_name,
                "kind": "platform-dimension-value",
                "kit_cid": PYTHON_PLATFORM_KIT_CID,
                "schemaVersion": "1.0.0",
                "value_name": value_name,
            }
        )
        for dimension_name, value_name in PYTHON_PLATFORM_DIMENSIONS
    ]
    # SortAdmission dimension value for concept:literal.
    # value_name "FullPrimitiveTier" matches Java for cross-kit substrate uniformity.
    sort_admission = _with_cid(
        {
            "compare_to": _ADMITS_SORTS_FORMULA,
            "dimension_name": "SortAdmission",
            "kind": "platform-dimension-value",
            "kit_cid": PYTHON_PLATFORM_KIT_CID,
            "schemaVersion": "1.0.0",
            "value_name": "FullPrimitiveTier",
        }
    )
    return existing + [sort_admission]


def declaration() -> Json:
    dim_vals = dimension_values()
    # Existing five operator-semantic dimensions (shared across all arithmetic/shift/etc. tags)
    op_dimensions = {
        value["dimension_name"]: value["cid"]
        for value in dim_vals
        if value["dimension_name"] in {d for d, _ in PYTHON_PLATFORM_DIMENSIONS}
    }
    # SortAdmission dimension value (used only by concept:literal tag)
    sort_admission_dv = next(v for v in dim_vals if v["dimension_name"] == "SortAdmission")

    op_tags = [
        _with_cid(
            {
                "dimensions": op_dimensions,
                "kind": "platform-semantic-tag",
                "kit_cid": PYTHON_PLATFORM_KIT_CID,
                "op_cid": op_cid,
                "schemaVersion": "1.0.0",
            }
        )
        for op_cid in PYTHON_PLATFORM_CONCEPT_OP_CIDS
    ]

    # concept:literal tag: only carries the SortAdmission dimension
    literal_tag = _with_cid(
        {
            "dimensions": {"SortAdmission": sort_admission_dv["cid"]},
            "kind": "platform-semantic-tag",
            "kit_cid": PYTHON_PLATFORM_KIT_CID,
            "op_cid": CONCEPT_LITERAL_CID,
            "schemaVersion": "1.0.0",
        }
    )

    return {
        "tags": op_tags + [literal_tag],
        "dimension_values": dim_vals,
    }


def _compare_atom(value_name: str) -> Json:
    return {"args": [], "kind": "atomic", "name": f"python:{value_name}"}


def _with_cid(memento: Json) -> Json:
    # CID is computed over JCS(memento) with "cid" and "kit_cid" elided,
    # per substrate spec (DimensionValueMemento::recompute_cid +
    # PlatformSemanticTag::recompute_cid in provekit-ir-types).
    for_cid = {k: v for k, v in memento.items() if k not in ("cid", "kit_cid")}
    result = dict(memento)
    result["cid"] = _cid_of_json(for_cid)
    return result
