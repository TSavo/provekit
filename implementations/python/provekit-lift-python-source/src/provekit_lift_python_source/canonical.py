from __future__ import annotations

import json
from typing import Any

from provekit_lift_py_tests.canonicalizer import (
    BLAKE3_512_PREFIX,
    Value,
    blake3_512_of,
    encode_jcs,
    varr,
    vbool,
    vint,
    vnull,
    vobj,
    vstr,
)


def canonical_json_bytes(value: Any) -> bytes:
    return encode_jcs(_json_to_value(value)).encode("utf-8")


def cid_of_json(value: Any) -> str:
    return blake3_512_of(canonical_json_bytes(value))


def template_json_bytes(value: Any) -> bytes:
    """Compact serde_json::Value::to_string style bytes for recognize templates."""
    return json.dumps(value, separators=(",", ":"), sort_keys=False).encode("utf-8")


def template_cid_of_json(value: Any) -> str:
    return blake3_512_of(template_json_bytes(value))


def _json_to_value(value: Any) -> Value:
    if value is None:
        return vnull()
    if isinstance(value, bool):
        return vbool(value)
    if isinstance(value, int):
        return vint(value)
    if isinstance(value, str):
        return vstr(value)
    if isinstance(value, list):
        return varr([_json_to_value(item) for item in value])
    if isinstance(value, dict):
        pairs: list[tuple[str, Value]] = []
        for key, item in value.items():
            if not isinstance(key, str):
                raise TypeError("canonical JSON object keys must be str")
            pairs.append((key, _json_to_value(item)))
        return vobj(pairs)
    raise TypeError(f"unsupported canonical JSON value: {type(value).__name__}")
