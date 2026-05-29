from __future__ import annotations

import ast
from pathlib import PurePosixPath
from typing import Any


def assemble_response(params: dict[str, Any]) -> dict[str, Any]:
    target_lang = params.get("target_lang")
    if target_lang not in (None, "", "python", "py"):
        raise ValueError(f"python assembler cannot assemble target_lang {target_lang!r}")

    fragments = params.get("fragments")
    if not isinstance(fragments, list):
        fragments = []

    content = _assemble_module(fragments)
    path = _python_file_path(params.get("file_basename"))
    return {
        "files": [{"path": path, "content": content}],
        "compile_classpath": [],
    }


def _assemble_module(fragments: list[Any]) -> str:
    imports = _collect_imports(fragments)
    helpers = _collect_blocks(fragments, "helpers")
    sources = _collect_blocks(fragments, "source")

    blocks: list[str] = []
    if imports:
        blocks.append("\n".join(imports))
    blocks.extend(helpers)
    blocks.extend(sources)
    content = "\n\n".join(blocks).rstrip() + "\n"
    ast.parse(content, filename="<provekit-python-assemble>")
    return content


def _collect_imports(fragments: list[Any]) -> list[str]:
    seen: set[str] = set()
    out: list[str] = []
    for fragment in fragments:
        if not isinstance(fragment, dict):
            continue
        imports = fragment.get("imports")
        if not isinstance(imports, list):
            continue
        for value in imports:
            import_stmt = _normalize_import(value)
            if not import_stmt or import_stmt in seen:
                continue
            seen.add(import_stmt)
            out.append(import_stmt)
    return sorted(out)


def _normalize_import(value: Any) -> str:
    raw = str(value).strip()
    if not raw:
        return ""
    if raw.startswith("import ") or raw.startswith("from "):
        return raw
    return f"import {raw}"


def _collect_blocks(fragments: list[Any], field: str) -> list[str]:
    seen: set[str] = set()
    out: list[str] = []
    for fragment in fragments:
        if not isinstance(fragment, dict):
            continue
        value = fragment.get(field)
        values = value if isinstance(value, list) else [value]
        for item in values:
            if not isinstance(item, str):
                continue
            block = item.strip()
            if not block or block in seen:
                continue
            seen.add(block)
            out.append(block)
    return out


def _python_file_path(value: Any) -> str:
    base = str(value or "").strip() or "materialized"
    base = base.replace("\\", "/")
    base = PurePosixPath(base).name
    if base.endswith(".py"):
        base = base[:-3]
    return _sanitize_file_stem(base) + ".py"


def _sanitize_file_stem(stem: str) -> str:
    chars: list[str] = []
    for ch in stem:
        if ch.isascii() and (ch.isalnum() or ch in {"_", "-"}):
            chars.append(ch)
        else:
            chars.append("_")
    out = "".join(chars).strip("_-.")
    return out or "materialized"
