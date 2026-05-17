from __future__ import annotations

from typing import Any

from .canonical import cid_of_json

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


def dimension_values() -> list[Json]:
    return [
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


def declaration() -> Json:
    dimensions = {
        value["dimension_name"]: value["cid"]
        for value in dimension_values()
        if isinstance(value["dimension_name"], str)
    }
    return {
        "tags": [
            _with_cid(
                {
                    "dimensions": dimensions,
                    "kind": "platform-semantic-tag",
                    "kit_cid": PYTHON_PLATFORM_KIT_CID,
                    "op_cid": op_cid,
                    "schemaVersion": "1.0.0",
                }
            )
            for op_cid in PYTHON_PLATFORM_CONCEPT_OP_CIDS
        ]
    }


def _compare_atom(value_name: str) -> Json:
    return {"kind": "atomic", "name": f"python:{value_name}", "args": []}


def _with_cid(memento: Json) -> Json:
    result = dict(memento)
    result["cid"] = cid_of_json(result)
    return result
