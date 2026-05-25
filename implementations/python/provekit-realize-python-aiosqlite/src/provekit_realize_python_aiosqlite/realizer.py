from __future__ import annotations

import contextvars
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

# `.proof`-load-via-RPC: per-request `bodyTemplates` JSON array lifted by
# cmd_materialize from the aiosqlite shim's signed .proof. When non-empty,
# `entries()` PREFERS these over the on-disk canonical-bodies-aiosqlite.json
# cache -- the @sugar.bind shim source is the authority. Empty -> disk
# fallback. ContextVar (not a global) so concurrent RPC invocations do not
# race; rpc.py sets/resets it per invoke. Mirrors the python-sqlite3 kit
# (#1463) + the core kit + Java #1458.
current_body_templates: contextvars.ContextVar[str] = contextvars.ContextVar(
    "current_body_templates", default=""
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
    template_params, lookup_param_types = _template_lookup_signature(
        params,
        param_types,
        named_term_tree,
    )
    mapped_return_type = map_source_type(return_type)
    candidate_names = (concept_name, concept_name.removeprefix("concept:"))
    for entry in entries():
        if entry.concept_name not in candidate_names:
            continue
        if entry.min_params is not None and len(lookup_param_types) < entry.min_params:
            continue
        if entry.max_params is not None and len(lookup_param_types) > entry.max_params:
            continue
        if entry.requires_param_types is not None:
            if tuple(lookup_param_types) != entry.requires_param_types:
                continue
        if entry.requires_return_type is not None:
            if mapped_return_type != entry.requires_return_type:
                continue
        if entry.template_kind != "verbatim":
            continue
        rendered = render_template(
            entry.template,
            template_params,
            lookup_param_types,
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
    ntt_args_shape = _ntt_args_shape(named_term_tree)
    if ntt_args_shape is not None:
        return ntt_args_shape
    return tuple(map_source_type(ty) for ty in param_types)


# NTT (named_term_tree) helpers. Kept in lock-step with the python-sqlite3
# kit's equivalents (#1253: latent inconsistency closed). Any change to NTT
# derivation MUST be applied to both kits in the same PR; the test suite
# guards this by pinning identical body_template_for outputs across the two
# plugins for shared catalog entries.


def _template_lookup_signature(
    params: list[str],
    param_types: list[str],
    named_term_tree: dict[str, Any] | None,
) -> tuple[list[str], list[str]]:
    ntt_args_shape = _ntt_args_shape(named_term_tree)
    if ntt_args_shape is None:
        return params, [map_source_type(ty) for ty in param_types]
    return (
        _ntt_template_params(params, named_term_tree, len(ntt_args_shape)),
        list(ntt_args_shape),
    )


def _ntt_args_shape(named_term_tree: dict[str, Any] | None) -> tuple[str, ...] | None:
    if not isinstance(named_term_tree, dict):
        return None
    args = named_term_tree.get("args")
    if not isinstance(args, list):
        return None
    out: list[str] = []
    for arg in args:
        if not isinstance(arg, dict):
            return None
        descriptor = _ntt_arg_descriptor(arg)
        if descriptor is None:
            return None
        out.append(_map_ntt_arg_descriptor(descriptor))
    return tuple(out)


def _ntt_arg_descriptor(arg: dict[str, Any]) -> str | None:
    for key in (
        "type",
        "typeName",
        "type_name",
        "sort",
        "sortName",
        "sort_name",
        "conceptName",
        "concept_name",
        "operationKind",
        "operation_kind",
    ):
        value = arg.get(key)
        if isinstance(value, str) and value.strip():
            return value.strip()
    return None


def _map_ntt_arg_descriptor(descriptor: str) -> str:
    match descriptor:
        case "Sql" | "sql" | "concept:sql":
            return "str"
        case "SqlArgs" | "sqlArgs" | "sql_args" | "sql-args" | "concept:sql-args":
            return "list[object]"
        case _:
            return map_source_type(descriptor)


def _ntt_template_params(
    params: list[str],
    named_term_tree: dict[str, Any] | None,
    arity: int,
) -> list[str]:
    if len(params) == arity:
        return params
    args = named_term_tree.get("args") if isinstance(named_term_tree, dict) else None
    if not isinstance(args, list):
        return [f"arg{index}" for index in range(arity)]
    names: list[str] = []
    for index, arg in enumerate(args):
        if isinstance(arg, dict):
            names.append(_ntt_arg_name(arg, index))
        else:
            names.append(f"arg{index}")
    return names


def _ntt_arg_name(arg: dict[str, Any], index: int) -> str:
    for key in ("name", "paramName", "param_name", "binding", "symbol"):
        value = arg.get(key)
        if isinstance(value, str) and value.strip():
            return _python_identifier_or_default(value.strip(), f"arg{index}")
    descriptor = _ntt_arg_descriptor(arg)
    if descriptor is None:
        return f"arg{index}"
    match descriptor:
        case "Sql" | "sql" | "concept:sql":
            return "sql"
        case "SqlArgs" | "sqlArgs" | "sql_args" | "sql-args" | "concept:sql-args":
            return "args"
        case _:
            return _python_identifier_or_default(
                descriptor.removeprefix("concept:"),
                f"arg{index}",
            )


def _python_identifier_or_default(value: str, default: str) -> str:
    identifier = value.replace("-", "_")
    if identifier.isidentifier():
        return identifier
    return default


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


def entries() -> tuple[BodyTemplateEntry, ...]:
    # `.proof`-load-via-RPC: when cmd_materialize fed `bodyTemplates` for this
    # request, those entries are the authority. Prepend them so the
    # first-match-wins matcher prefers them over the disk cache; they are NEVER
    # cached statically (per-request, library-specific). Empty -> fall through
    # to the disk-loaded cache. Mirrors python-sqlite3 (#1463) + Java #1458.
    rpc_templates = current_body_templates.get()
    if rpc_templates:
        rpc_entries = _parse_entries_from_rpc_array(rpc_templates)
        if rpc_entries:
            return rpc_entries + _disk_entries()
    return _disk_entries()


@lru_cache(maxsize=1)
def _disk_entries() -> tuple[BodyTemplateEntry, ...]:
    path = _find_repo_file(BODY_TEMPLATE_REL)
    if path is None:
        return ()
    raw = path.read_text(encoding="utf-8")
    root = json.loads(raw)
    content = root.get("header", {}).get("content", {})
    return _parse_entry_array(content.get("entries", []))


def _parse_entries_from_rpc_array(raw_array: str) -> tuple[BodyTemplateEntry, ...]:
    """`.proof`-load-via-RPC: parse a BARE `bodyTemplates` array (sent by
    cmd_materialize from the shim .proof) into BodyTemplateEntry records. Same
    per-entry shape as the disk projection's `content.entries`."""
    try:
        root = json.loads(raw_array)
    except (ValueError, TypeError):
        return ()
    if not isinstance(root, list):
        return ()
    return _parse_entry_array(root)


def _parse_entry_array(items: Any) -> tuple[BodyTemplateEntry, ...]:
    if not isinstance(items, list):
        return ()
    out: list[BodyTemplateEntry] = []
    for item in items:
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
