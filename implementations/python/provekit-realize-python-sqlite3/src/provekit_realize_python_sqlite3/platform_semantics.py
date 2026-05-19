from __future__ import annotations

import json
from typing import Any

import blake3 as _blake3

Json = dict[str, Any]

# CID for concept:insert-and-get-id, minted from its AlgorithmMemento via JCS+blake3-512.
CONCEPT_INSERT_AND_GET_ID_CID = (
    "blake3-512:"
    "0a4f0a8d36d8dee96b8d5b32a18bb390f35877ecef611771048c6e10cfc3d25a"
    "d8f59de89b00c7794f62cabaf91dbd779244338393a8bb6ef5e8309b0929b3ca"
)

KIT_ID = "provekit-binding-python-sqlite3@0.1.0"

# kit_cid is provenance metadata only (elided from CID computation per substrate spec).
SQLITE3_KIT_CID = (
    "blake3-512:"
    + _blake3.blake3(KIT_ID.encode("utf-8")).digest(length=64).hex()
)


def dimension_values() -> list[Json]:
    return [
        _with_cid(
            {
                # CursorLastRowid: row id is sourced from the cursor state after
                # executing the INSERT. Accessed as cursor.lastrowid.
                "compare_to": {
                    "kind": "atomic",
                    "name": "row_id_source",
                    "args": [
                        {
                            "kind": "ctor",
                            "name": "cursor_lastrowid",
                            "args": [
                                {
                                    "kind": "ctor",
                                    "name": "cursor_state_after_execute",
                                    "args": [],
                                }
                            ],
                        }
                    ],
                },
                "dimension_name": "RowIdMechanism",
                "kind": "platform-dimension-value",
                "kit_cid": SQLITE3_KIT_CID,
                "schemaVersion": "1.0.0",
                "value_name": "CursorLastRowid",
            }
        )
    ]


def declaration() -> Json:
    dim_vals = dimension_values()
    row_id_cid = next(v["cid"] for v in dim_vals if v["dimension_name"] == "RowIdMechanism")
    return {
        "tags": [
            _with_cid(
                {
                    "dimensions": {"RowIdMechanism": row_id_cid},
                    "kind": "platform-semantic-tag",
                    "kit_cid": SQLITE3_KIT_CID,
                    "op_cid": CONCEPT_INSERT_AND_GET_ID_CID,
                    "schemaVersion": "1.0.0",
                }
            )
        ],
        "dimension_values": dim_vals,
    }


def _with_cid(memento: Json) -> Json:
    # CID is computed over JCS(memento) with "cid" and "kit_cid" elided,
    # per substrate spec (DimensionValueMemento::recompute_cid +
    # PlatformSemanticTag::recompute_cid in provekit-ir-types).
    for_cid = {k: v for k, v in memento.items() if k not in ("cid", "kit_cid")}
    result = dict(memento)
    result["cid"] = _cid_of_json(for_cid)
    return result


def _cid_of_json(value: Any) -> str:
    return (
        "blake3-512:"
        + _blake3.blake3(_canonical_json_bytes(value)).digest(length=64).hex()
    )


def _canonical_json_bytes(value: Any) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")
