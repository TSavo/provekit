"""Shared JCS and Markdown helpers for catalog audit scripts."""

from __future__ import annotations

import shutil
import subprocess
from typing import Any, Iterable


def encode_jcs(value: Any) -> str:
    if value is None:
        return "null"
    if value is True:
        return "true"
    if value is False:
        return "false"
    if isinstance(value, int) and not isinstance(value, bool):
        return str(value)
    if isinstance(value, str):
        return encode_jcs_string(value)
    if isinstance(value, list):
        return "[" + ",".join(encode_jcs(item) for item in value) + "]"
    if isinstance(value, dict):
        items = []
        for key in sorted(value):
            if not isinstance(key, str):
                raise TypeError(f"JCS object key must be a string, got {key!r}")
            items.append(f"{encode_jcs_string(key)}:{encode_jcs(value[key])}")
        return "{" + ",".join(items) + "}"
    raise TypeError(f"JCS cannot encode value {value!r}")


def encode_jcs_string(value: str) -> str:
    out = ['"']
    for char in value:
        code = ord(char)
        if char == '"':
            out.append('\\"')
        elif char == "\\":
            out.append("\\\\")
        elif code < 0x20:
            out.append(f"\\u{code:04x}")
        else:
            out.append(char)
    out.append('"')
    return "".join(out)


def blake3_512(data: bytes) -> str:
    try:
        import blake3  # type: ignore

        digest = blake3.blake3(data).digest(length=64).hex()
        return f"blake3-512:{digest}"
    except ModuleNotFoundError:
        pass

    b3sum = shutil.which("b3sum")
    if b3sum is None:
        raise SystemExit("BLAKE3 unavailable: install python blake3 or provide b3sum")
    proc = subprocess.run(
        [b3sum, "--length", "64"],
        input=data,
        check=True,
        capture_output=True,
    )
    digest = proc.stdout.decode("utf-8").split()[0]
    return f"blake3-512:{digest}"


def md_cell(value: object) -> str:
    text = str(value)
    text = text.replace("\n", "<br>")
    text = text.replace("|", "\\|")
    return text


def table(lines: list[str], headers: list[str], rows: Iterable[Iterable[object]]) -> None:
    lines.append("| " + " | ".join(md_cell(header) for header in headers) + " |")
    lines.append("|" + "|".join(" --- " for _ in headers) + "|")
    for row in rows:
        lines.append("| " + " | ".join(md_cell(col) for col in row) + " |")
    lines.append("")
