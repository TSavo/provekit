from __future__ import annotations

import argparse
import ast
import json
from pathlib import Path
import sys
import traceback
from typing import Any

from .ast_template import function_body_template, function_param_names
from .bind_lifter import lift_paths
from .canonical import template_cid_of_json

VERSION = "0.1.0"


def initialize_result() -> dict[str, Any]:
    return {
        "name": "provekit-lift-python-bind",
        "version": VERSION,
        "protocol_version": "pep/1.7.0",
        "capabilities": {
            "authoring_surfaces": ["python", "python-bind"],
            "ir_version": "bind-ir/1.0.0",
            "emits_signed_mementos": False,
        },
    }


def run_rpc() -> None:
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
            response = dispatch(request)
        except json.JSONDecodeError as exc:
            response = _error(None, -32700, f"PARSE_ERROR: {exc}")
        except Exception as exc:
            response = _error(None, -32603, f"{exc}\n{traceback.format_exc()}")
        _send(response)


def dispatch(request: dict[str, Any]) -> dict[str, Any]:
    msg_id = request.get("id")
    method = request.get("method", "")
    params = request.get("params") or {}

    if method == "initialize":
        return {"jsonrpc": "2.0", "id": msg_id, "result": initialize_result()}
    if method == "lift":
        return _lift(msg_id, params)
    if method == "provekit.plugin.recognize":
        return {
            "jsonrpc": "2.0",
            "id": msg_id,
            "result": recognize_impl(params),
        }
    if method == "shutdown":
        return {"jsonrpc": "2.0", "id": msg_id, "result": None}
    return _error(msg_id, -32601, f"METHOD_NOT_FOUND: {method}")


def _lift(msg_id: Any, params: dict[str, Any]) -> dict[str, Any]:
    source_paths = params.get("source_paths")
    paths: list[str]
    if isinstance(source_paths, list):
        paths = [str(path) for path in source_paths if str(path)]
    else:
        paths = ["."]
    if not paths:
        paths = ["."]

    options_value = params.get("options")
    options = options_value if isinstance(options_value, dict) else {}
    layer = str(options.get("layer") or "all")
    result = lift_paths(str(params.get("workspace_root", ".")), paths, layer=layer)
    return {
        "jsonrpc": "2.0",
        "id": msg_id,
        "result": {
            "kind": "ir-document",
            "ir": result.ir,
            "diagnostics": result.diagnostics,
        },
    }


def recognize_impl(params: dict[str, Any]) -> dict[str, Any]:
    project_root = params.get("project_root")
    if not isinstance(project_root, str) or not project_root:
        raise ValueError("missing `project_root`")
    source_paths = params.get("source_paths")
    if not isinstance(source_paths, list):
        raise ValueError("missing `source_paths` array")
    root = Path(project_root).resolve()

    binding_templates, sugar_template_files = _self_resolved_binding_templates(
        root, source_paths
    )

    bindings_by_cid: dict[str, dict[str, Any]] = {}
    for binding in binding_templates:
        if not isinstance(binding, dict):
            continue
        cid = binding.get("template_cid")
        if isinstance(cid, str) and cid:
            bindings_by_cid[cid] = binding

    tags: list[dict[str, Any]] = []
    for rel_path, full_path in _iter_requested_python_files(root, source_paths):
        if rel_path in sugar_template_files:
            continue
        try:
            source = full_path.read_text(encoding="utf-8")
        except OSError:
            continue
        try:
            tree = ast.parse(source, filename=rel_path)
        except SyntaxError:
            continue
        for node in _iter_candidate_functions(tree):
            tag = _recognize_function(rel_path, node, bindings_by_cid)
            if tag is not None:
                tags.append(tag)
    return {"tags": tags}


def _self_resolved_binding_templates(
    root: Path,
    source_paths: list[Any],
) -> tuple[list[dict[str, Any]], set[str]]:
    result = lift_paths(
        str(root),
        [str(path) for path in source_paths],
        layer="library-bindings",
    )
    templates: list[dict[str, Any]] = []
    sugar_template_files: set[str] = set()
    for entry in result.ir:
        if (
            not isinstance(entry, dict)
            or entry.get("kind") != "library-sugar-binding-entry"
        ):
            continue
        template = _binding_template_from_sugar_entry(entry)
        if template is not None:
            templates.append(template)
        body_source = entry.get("body_source")
        file = body_source.get("file") if isinstance(body_source, dict) else None
        if isinstance(file, str) and file:
            sugar_template_files.add(file)
    return templates, sugar_template_files


def _binding_template_from_sugar_entry(entry: dict[str, Any]) -> dict[str, Any] | None:
    body_source = entry.get("body_source")
    if not isinstance(body_source, dict):
        return None
    ast_template = body_source.get("ast_template")
    template_cid = body_source.get("template_cid")
    if ast_template is None or not isinstance(template_cid, str) or not template_cid:
        return None
    return {
        "concept_name": entry.get("concept_name"),
        "library_tag": entry.get("target_library_tag"),
        "family": entry.get("family"),
        "ast_template": ast_template,
        "template_cid": template_cid,
        "param_names": body_source.get("param_names"),
        "contract_cid": entry.get("contract_cid"),
    }


def _recognize_function(
    rel_path: str,
    node: ast.FunctionDef | ast.AsyncFunctionDef,
    bindings_by_cid: dict[str, dict[str, Any]],
) -> dict[str, Any] | None:
    candidate_template = function_body_template(node)
    candidate_cid = template_cid_of_json(candidate_template)
    binding = bindings_by_cid.get(candidate_cid)
    if binding is None:
        return None

    param_names = function_param_names(node)
    return {
        "file": rel_path,
        "span": {
            "start_line": node.lineno,
            "start_col": node.col_offset,
            "end_line": node.end_lineno or node.lineno,
            "end_col": node.end_col_offset or 0,
        },
        "function_name": node.name,
        "concept_name": binding.get("concept_name"),
        "library_tag": binding.get("library_tag"),
        "family": binding.get("family"),
        "template_cid": candidate_cid,
        "contract_cid": binding.get("contract_cid"),
        "match_tier": "exact",
        "param_bindings": [
            {"index": index + 1, "source_text": name}
            for index, name in enumerate(param_names)
        ],
    }


def _iter_candidate_functions(
    tree: ast.AST,
) -> list[ast.FunctionDef | ast.AsyncFunctionDef]:
    candidates: list[ast.FunctionDef | ast.AsyncFunctionDef] = []

    class Visitor(ast.NodeVisitor):
        def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
            candidates.append(node)
            self.generic_visit(node)

        def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
            candidates.append(node)
            self.generic_visit(node)

    Visitor().visit(tree)
    return candidates


def _iter_requested_python_files(
    root: Path,
    source_paths: list[Any],
) -> list[tuple[str, Path]]:
    files: list[tuple[str, Path]] = []
    seen: set[Path] = set()
    for item in source_paths:
        rel = str(item)
        if not rel:
            continue
        matches = _expand_source_path(root, rel)
        for full_path in matches:
            try:
                resolved = full_path.resolve()
            except OSError:
                continue
            if resolved in seen or not _is_relative_to(resolved, root):
                continue
            if not resolved.is_file() or resolved.suffix != ".py":
                continue
            seen.add(resolved)
            display = resolved.relative_to(root).as_posix()
            files.append((display, resolved))
    return files


def _expand_source_path(root: Path, rel: str) -> list[Path]:
    if any(ch in rel for ch in "*?[]"):
        return sorted(root.glob(rel))
    full = Path(rel)
    if not full.is_absolute():
        full = root / full
    if full.is_dir():
        return sorted(full.rglob("*.py"))
    return [full]


def _is_relative_to(path: Path, root: Path) -> bool:
    try:
        path.relative_to(root)
        return True
    except ValueError:
        return False


def _send(obj: dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(obj, separators=(",", ":"), ensure_ascii=False) + "\n")
    sys.stdout.flush()


def _error(msg_id: Any, code: int, message: str) -> dict[str, Any]:
    return {
        "jsonrpc": "2.0",
        "id": msg_id,
        "error": {"code": code, "message": message},
    }


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--rpc", action="store_true", help="run bind JSON-RPC over stdio")
    parser.add_argument("--bind-rpc", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args(argv)
    if args.rpc or args.bind_rpc:
        run_rpc()
    else:
        parser.print_help()


if __name__ == "__main__":
    main()
