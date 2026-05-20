from __future__ import annotations

from typing import Any

from .realizer import _cid_of_json

Json = dict[str, Any]

PYTHON_PLATFORM_KIT_CID = (
    "blake3-512:"
    "bc36b43fec1a80efcecb05f8c4de725f961295466530aec452763c6c479b67c"
    "590c2e8062a3f46979383086ae80e6c0a917c443625d3474a7a89705e0a56ab8c"
)

# Canonical sort CIDs (from #1282)
_SORT_INT_CID = "blake3-512:30ffc51350121a7172f3e4064a33c45bbd345756979fccff6875cd2ab33e4964d098a99df80cfbdf1ec1a0738c5ac3476f0ff8f75589ea511d1acd82c74ecd58"
_SORT_FLOAT_CID = "blake3-512:b979e70c4d5e53d9bdf13d6f08330be3c5b0714b8c770d69bbd05946b86c36df5274be8145a2683cc29c278155c9c1ee65b6897913524eecb9e4c89c71862f57"
_SORT_STRING_CID = "blake3-512:be8721d24849feb74c4721520bdba02d352a94f49253a627cd509127472aa1c47cbe99cb705cac4159b5365abcce0c9aaa4901fe67630827deb6be1f9daeea10"
_SORT_BOOL_CID = "blake3-512:0ee13bf3fd6b7ecfbee72dfbfc18a7c0ea7f1663de6cca43cefb36f5b4c03665452646094a7c296e819e75d683c6ce4821f3d7db3c3c78ae97f2d4e3451d2074"
_SORT_BYTES_CID = "blake3-512:7116ef6e62e6739b213a8394f975a53c771b89f08c36d27143827acfcfebc0e39e5b82c530be668c3cfd5ec6966ccaa42930b37fdb1f4ac25652a970be10fb6b"
_SORT_NULL_CID = "blake3-512:62f6040bd3f414c1e6c2b7bdf276669cd5613b33cb508a81170170064ca3ffba771a4b0002dc52e059fce5f9f63a1874ef71bd4ec89ae06e89c87a3e91aac3b5"
# Python admits: Int, Float, String, Bool, Bytes, Null (full primitive tier)

_CONCEPT_LITERAL_NAME = "concept:literal"


def answers() -> list[Json]:
    """Returns one LiteralEncodingMemento per sort Python admits at literal positions.

    Python admits: Int, Float, String, Bool, Bytes, Null.
    """
    return [
        _memento(_SORT_INT_CID, "42", 42),
        # Python float literals are bit-preserved as {"__float_bits__": <u64>} (IEEE 754 raw bits).
        # 4614253070214989087 == 0x40091EB851EB851F == bits of 3.14 as f64.
        _memento(_SORT_FLOAT_CID, "3.14", {"__float_bits__": 4614253070214989087}),
        _memento(_SORT_STRING_CID, '"hello"', "hello"),
        _memento(_SORT_BOOL_CID, "True", True),
        # Python bytes literals b"abc" are represented as the UTF-8 string "abc" at
        # the concept:literal value layer (bytes decoded to str).
        _memento(_SORT_BYTES_CID, 'b"abc"', "abc"),
        _memento(_SORT_NULL_CID, "None", None),
    ]


def _memento(sort_cid: str, source_example: str, decoded_value: Any) -> Json:
    expected_term_shape_node = {
        "concept_name": _CONCEPT_LITERAL_NAME,
        "sort": sort_cid,
        "value": decoded_value,
    }
    memento_without_cid = {
        "expected_term_shape_node": expected_term_shape_node,
        "kind": "literal-encoding-memento",
        "kit_cid": PYTHON_PLATFORM_KIT_CID,
        "language": "python",
        "schemaVersion": "1.0.0",
        "sort_cid": sort_cid,
        "source_example": source_example,
    }
    # CID is computed with "cid" and "kit_cid" elided, per substrate spec.
    for_cid = {k: v for k, v in memento_without_cid.items() if k not in ("cid", "kit_cid")}
    cid = _cid_of_json(for_cid)
    return {"cid": cid, **memento_without_cid}
