from __future__ import annotations

import json
import shutil
import subprocess
from typing import Any

BLAKE3_512_PREFIX = "blake3-512:"


def canonical_json_bytes(value: Any) -> bytes:
    encoded = json.dumps(
        value,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=False,
    )
    return encoded.encode("utf-8")


def blake3_512_of(data: bytes) -> str:
    if not isinstance(data, bytes):
        raise TypeError("blake3_512_of requires bytes")

    try:
        from provekit_lift_py_tests.canonicalizer import (  # type: ignore[import-not-found]
            blake3_512_of as existing_blake3_512_of,
        )

        return existing_blake3_512_of(data)
    except Exception:
        pass

    try:
        import blake3  # type: ignore[import-not-found]

        digest = blake3.blake3(data).digest(length=64)
        return BLAKE3_512_PREFIX + digest.hex()
    except ModuleNotFoundError:
        return _blake3_512_with_b3sum(data)


def cid_of_json(value: Any) -> str:
    return blake3_512_of(canonical_json_bytes(value))


def _blake3_512_with_b3sum(data: bytes) -> str:
    exe = shutil.which("b3sum")
    if exe is None:
        raise RuntimeError(
            "BLAKE3 support requires the blake3 Python package or b3sum on PATH"
        )
    proc = subprocess.run(
        [exe, "--length", "64"],
        input=data,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=True,
    )
    digest = proc.stdout.decode("ascii").split()[0]
    return BLAKE3_512_PREFIX + digest
