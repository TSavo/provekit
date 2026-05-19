from __future__ import annotations

import json
import os
import re
from dataclasses import dataclass
from functools import lru_cache
from pathlib import Path
from typing import Any

BODY_TEMPLATE_REL = Path(
    "menagerie/python-language-signature/specs/body-templates/python-canonical-bodies-aiosqlite.json"
)
PLACEHOLDER_RE = re.compile(r"\$\{[^}]+\}")


@dataclass(frozen=True)
class BodyTemplateEntry:
    concept_name: str
    template_kind: str
    template: str
    min_params: int | None
    max_params: int | None
    requires_param_types: tuple[str, ...] | None
    requires_return_type: str | None


@dataclass(frozen=True)
class MissingTemplateEntry:
    operation_kind: str
    args_shape: tuple[str, ...]
    function: str
    term_position: str

    def to_json(self) -> dict[str, Any]:
        return {
            "operation_kind": self.operation_kind,
            "args_shape": list(self.args_shape),
            "function": self.function,
            "term_position": self.term_position,
        }


class MissingTemplateError(Exception):
    def __init__(self, entries: tuple[MissingTemplateEntry, ...]):
        super().__init__("missing body-template entry")
        self.entries = entries


def emit_stub(
    function: str,
    params: list[str],
    param_types: list[str],
    return_type: str,
    concept_name: str,
    named_term_tree: dict[str, Any] | None = None,
) -> dict[str, Any]:
    body = body_template_for(
        concept_name,
        params,
        param_types,
        return_type,
        named_term_tree=named_term_tree,
    )
    if body is None:
        raise MissingTemplateError(
            (
                MissingTemplateEntry(
                    operation_kind=concept_name,
                    args_shape=args_shape_for(param_types, named_term_tree),
                    function=function,
                    term_position="body",
                ),
            )
        )
    return {
        "source": _function_source(function, params, body),
        "is_stub": False,
        "extension": "py",
    }


def body_template_for(
    concept_name: str,
    params: list[str],
    param_types: list[str],
    return_type: str,
    named_term_tree: dict[str, Any] | None = None,
) -> str | None:
    tree_args_shape = _args_shape_from_named_term_tree(named_term_tree)
    if tree_args_shape is None:
        args_shape = tuple(map_source_type(ty) for ty in param_types)
        template_params = params
    else:
        args_shape = tree_args_shape
        template_params = _tree_template_params(named_term_tree, params, args_shape)
    mapped_return_type = map_source_type(return_type)
    candidate_names = (concept_name, concept_name.removeprefix("concept:"))
    for entry in entries():
        if entry.concept_name not in candidate_names:
            continue
        if entry.min_params is not None and len(args_shape) < entry.min_params:
            continue
        if entry.max_params is not None and len(args_shape) > entry.max_params:
            continue
        if entry.requires_param_types is not None:
            if args_shape != entry.requires_param_types:
                continue
        if entry.requires_return_type is not None:
            if mapped_return_type != entry.requires_return_type:
                continue
        if entry.template_kind != "verbatim":
            continue
        rendered = render_template(
            entry.template,
            template_params,
            list(args_shape),
            mapped_return_type,
        )
        if rendered is None:
            continue
        return rendered
    return None


def args_shape_for(
    param_types: list[str],
    named_term_tree: dict[str, Any] | None = None,
) -> tuple[str, ...]:
    tree_args_shape = _args_shape_from_named_term_tree(named_term_tree)
    if tree_args_shape is not None:
        return tree_args_shape
    return tuple(map_source_type(ty) for ty in param_types)


def _args_shape_from_named_term_tree(
    named_term_tree: dict[str, Any] | None,
) -> tuple[str, ...] | None:
    if not isinstance(named_term_tree, dict):
        return None
    args = named_term_tree.get("args")
    if not isinstance(args, list):
        return ()
    return tuple(_tree_arg_shape(arg) for arg in args)


def _tree_arg_shape(arg: Any) -> str:
    if not isinstance(arg, dict):
        return "arg"
    for key in (
        "sort",
        "sortName",
        "conceptName",
        "concept_name",
        "operationKind",
        "operation_kind",
        "kind",
    ):
        value = arg.get(key)
        if isinstance(value, str) and value.strip():
            return value
    return "arg"


def _tree_template_params(
    named_term_tree: dict[str, Any] | None,
    params: list[str],
    args_shape: tuple[str, ...],
) -> list[str]:
    if len(params) == len(args_shape):
        return params
    if not isinstance(named_term_tree, dict):
        return params
    args = named_term_tree.get("args")
    if not isinstance(args, list):
        return []
    out: list[str] = []
    for index, arg in enumerate(args):
        out.append(_tree_arg_name(arg, args_shape[index], index))
    return out


def _tree_arg_name(arg: Any, args_shape: str, index: int) -> str:
    if isinstance(arg, dict):
        for key in ("paramName", "param_name", "symbol", "name"):
            value = arg.get(key)
            if isinstance(value, str) and value.strip():
                return _python_identifier(value, index)
    return _python_identifier(args_shape, index)


def _python_identifier(value: str, index: int) -> str:
    stripped = value.strip()
    if stripped in ("Sql", "concept:sql", "sql"):
        return "sql"
    if stripped in ("SqlArgs", "concept:sql-args", "sql-args", "sql_args"):
        return "args"
    if stripped.startswith("concept:"):
        stripped = stripped.removeprefix("concept:")
    candidate = re.sub(r"\W+", "_", stripped).strip("_").lower()
    if not candidate:
        return f"arg{index}"
    if not (candidate[0].isalpha() or candidate[0] == "_"):
        return f"arg{index}"
    return candidate


def map_source_type(src: str) -> str:
    match src:
        case "()":
            return "None"
        case "i64" | "u64" | "i32" | "u32" | "i16" | "u16" | "i8" | "u8" | "int" | "number":
            return "int"
        case "f64" | "f32" | "float":
            return "float"
        case "bool" | "boolean":
            return "bool"
        case "String" | "&str" | "&String" | "str" | "string":
            return "str"
        case _:
            return src


def render_template(
    template: str,
    params: list[str],
    param_types: list[str],
    return_type: str,
) -> str | None:
    rendered = template
    for index, name in enumerate(params):
        rendered = rendered.replace(f"${{param{index}}}", name)
    for index, type_name in enumerate(param_types):
        rendered = rendered.replace(f"${{param_type_{index}}}", type_name)
    rendered = rendered.replace("${param_count}", str(len(params)))
    rendered = rendered.replace("${return_type}", return_type)
    if PLACEHOLDER_RE.search(rendered):
        return None
    return rendered


@lru_cache(maxsize=1)
def entries() -> tuple[BodyTemplateEntry, ...]:
    path = _find_repo_file(BODY_TEMPLATE_REL)
    if path is None:
        return ()
    raw = path.read_text(encoding="utf-8")
    root = json.loads(raw)
    content = root.get("header", {}).get("content", {})
    out: list[BodyTemplateEntry] = []
    for item in content.get("entries", []):
        if not isinstance(item, dict):
            continue
        template = item.get("emission_template", {})
        if not isinstance(template, dict):
            continue
        guard = item.get("signature_guard", {})
        if not isinstance(guard, dict):
            guard = {}
        concept_name = item.get("concept_name")
        template_kind = template.get("kind")
        template_text = template.get("template")
        if not isinstance(concept_name, str):
            continue
        if not isinstance(template_kind, str) or not isinstance(template_text, str):
            continue
        requires_param_types = guard.get("requires_param_types")
        out.append(
            BodyTemplateEntry(
                concept_name=concept_name,
                template_kind=template_kind,
                template=template_text,
                min_params=_int_or_none(guard.get("min_params")),
                max_params=_int_or_none(guard.get("max_params")),
                requires_param_types=tuple(requires_param_types)
                if isinstance(requires_param_types, list)
                else None,
                requires_return_type=guard.get("requires_return_type")
                if isinstance(guard.get("requires_return_type"), str)
                else None,
            )
        )
    return tuple(out)


def _function_source(function: str, params: list[str], body: str) -> str:
    param_list = ", ".join(params)
    body_lines = body.splitlines() or ["pass"]
    indented = "\n".join(f"    {line}" if line else "" for line in body_lines)
    return f"async def {function}({param_list}):\n{indented}\n"


def _int_or_none(value: Any) -> int | None:
    if isinstance(value, int):
        return value
    return None


def _find_repo_file(relative: Path) -> Path | None:
    seen: set[Path] = set()
    for base in _candidate_bases():
        candidate = base / relative
        if candidate in seen:
            continue
        seen.add(candidate)
        if candidate.exists():
            return candidate
    return None


def _candidate_bases() -> list[Path]:
    bases: list[Path] = []
    env_root = os.environ.get("PROVEKIT_REPO_ROOT")
    if env_root:
        bases.append(Path(env_root))
    cwd = Path.cwd().resolve()
    bases.extend([cwd, *cwd.parents])
    here = Path(__file__).resolve()
    bases.extend(here.parents)
    return bases
