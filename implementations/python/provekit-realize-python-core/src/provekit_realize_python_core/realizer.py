from __future__ import annotations

import json
import os
import re
from dataclasses import dataclass
from functools import lru_cache
from pathlib import Path
from typing import Any

import blake3

BODY_TEMPLATE_REL = Path(
    "menagerie/python-language-signature/specs/body-templates/python-canonical-bodies.json"
)
LIBPROVEKIT_BODY_TEMPLATE_REL = Path(
    "menagerie/python-language-signature/specs/body-templates/python-canonical-bodies-libprovekit.json"
)
RUST_RUNTIME_BODY_TEMPLATE_REL = Path(
    "menagerie/python-language-signature/specs/body-templates/python-canonical-bodies-rust-runtime.json"
)
PLACEHOLDER_RE = re.compile(r"\$\{[^}]+\}")
DEFAULT_KIT_CID = "blake3-512:" + blake3.blake3(
    b"provekit-realize-python-core@0.1.0"
).digest(length=64).hex()
DEFAULT_POLICY_CID = "blake3-512:" + blake3.blake3(
    b"provekit-realize-python-core/default-contract-comment-policy"
).digest(length=64).hex()
DEFAULT_SUGAR_DICT_CID = "blake3-512:" + blake3.blake3(
    b"provekit-realize-python-core/contract-comment-sugar-v1"
).digest(length=64).hex()


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
class TermBody:
    body: str
    is_stub: bool


@dataclass(frozen=True)
class TermExpression:
    text: str | None = None
    stub_body: str | None = None


def emit_stub(
    function: str,
    params: list[str],
    param_types: list[str],
    return_type: str,
    concept_name: str,
    contract: dict[str, Any] | None = None,
    sugar_cids: list[str] | None = None,
    sugar_plugins: list[Any] | None = None,
) -> dict[str, Any]:
    term_body = term_body_for(concept_name, params, param_types, return_type)
    if term_body is not None:
        body = term_body.body
        is_stub = term_body.is_stub
    else:
        body = body_template_for(concept_name, params, param_types, return_type)
        is_stub = body is None
        if body is None:
            body = f'raise NotImplementedError("provekit-bind canonical: {concept_name}")'
    contract_lines = contract_comment_lines(contract, sugar_cids or [], sugar_plugins or [])
    result = {
        "source": _function_source(function, params, body, leading_lines=contract_lines),
        "is_stub": is_stub,
        "extension": "py",
    }
    if contract_lines:
        result["observed_loss_record"] = {}
        result["used_sugars"] = [sugar_cids[0]] if sugar_cids else []
    return result


def contract_comment_lines(
    contract: dict[str, Any] | None,
    sugar_cids: list[str],
    sugar_plugins: list[Any],
) -> list[str]:
    if not isinstance(contract, dict):
        return []
    witnesses = contract.get("witnesses")
    if not isinstance(witnesses, list):
        return []

    lines: list[str] = []
    for witness in witnesses:
        if not isinstance(witness, dict):
            continue
        payload = _contract_comment_payload(contract, witness, sugar_cids, sugar_plugins)
        if payload is None:
            continue
        payload_cid = _cid_of_json(payload)
        payload_json = json.dumps(
            payload,
            ensure_ascii=False,
            separators=(",", ":"),
            sort_keys=True,
        )
        lines.append(f"# provekit-contract: {payload_json}")
        lines.append(f"# provekit-contract-payload-cid: {payload_cid}")
    return lines


def _contract_comment_payload(
    contract: dict[str, Any],
    witness: dict[str, Any],
    sugar_cids: list[str],
    sugar_plugins: list[Any],
) -> dict[str, Any] | None:
    role = _payload_role(str(witness.get("role", "")))
    predicate = witness.get("predicate")
    if role is None or not isinstance(predicate, dict):
        return None

    concept_site_cid = contract.get("concept_site_cid")
    local_contract_cid = contract.get("local_contract_cid")
    if not isinstance(concept_site_cid, str) or not isinstance(local_contract_cid, str):
        return None

    extension_fields = witness.get("extension_fields")
    contract_cid = None
    if isinstance(extension_fields, dict):
        value = extension_fields.get("contract_cid")
        if isinstance(value, str):
            contract_cid = value
    if contract_cid is None:
        contract_cid = local_contract_cid

    fol_text = witness.get("predicate_text")
    if not isinstance(fol_text, str) or not fol_text:
        fol_text = _formula_debug_text(predicate)

    loss_record: dict[str, Any] = {}
    payload: dict[str, Any] = {
        "artifact_kind": "provekit-contract-comment-sugar",
        "concept_site_cid": concept_site_cid,
        "contract_cid": contract_cid,
        "emitted_by": {
            "kit_cid": DEFAULT_KIT_CID,
            "kit_kind": "realize",
            "target_language": "python",
        },
        "fol_text": fol_text,
        "ir_formula_jcs": predicate,
        "ir_formula_jcs_cid": _cid_of_json(predicate),
        "local_contract_cid": local_contract_cid,
        "loss_record_cid": _cid_of_json(loss_record),
        "policy_cid": _policy_cid(contract),
        "role": role,
        "schema_version": "1",
        "sugar_dict_cid": _sugar_dict_cid(sugar_cids, sugar_plugins),
    }
    return payload


def _payload_role(role: str) -> str | None:
    match role:
        case "pre" | "post" | "throws" | "observation":
            return role
        case "inv" | "invariant":
            return "invariant"
        case _:
            return None


def _policy_cid(contract: dict[str, Any]) -> str:
    value = contract.get("policy_cid")
    if isinstance(value, str) and value.startswith("blake3-512:"):
        return value
    return DEFAULT_POLICY_CID


def _sugar_dict_cid(sugar_cids: list[str], sugar_plugins: list[Any]) -> str:
    if sugar_cids:
        return sugar_cids[0]
    for plugin in sugar_plugins:
        if not isinstance(plugin, dict):
            continue
        header = plugin.get("header")
        cid = header.get("cid") if isinstance(header, dict) else None
        if isinstance(cid, str):
            return cid
    return DEFAULT_SUGAR_DICT_CID


def _cid_of_json(value: Any) -> str:
    return (
        "blake3-512:"
        + blake3.blake3(_canonical_json_bytes(value)).digest(length=64).hex()
    )


def _canonical_json_bytes(value: Any) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")


def _formula_debug_text(formula: dict[str, Any]) -> str:
    if formula.get("kind") == "atomic":
        name = formula.get("name")
        args = formula.get("args")
        if isinstance(name, str) and isinstance(args, list):
            rendered = ", ".join(_term_debug_text(arg) for arg in args)
            return f"{name}({rendered})"
    return "<formula>"


def _term_debug_text(term: Any) -> str:
    if isinstance(term, dict):
        if term.get("kind") == "var" and isinstance(term.get("name"), str):
            return term["name"]
        if term.get("kind") == "const":
            return repr(term.get("value"))
        if term.get("kind") == "ctor" and isinstance(term.get("name"), str):
            args = term.get("args")
            if isinstance(args, list):
                rendered = ", ".join(_term_debug_text(arg) for arg in args)
                return f"{term['name']}({rendered})"
    return "<?>"


def body_template_for(
    concept_name: str,
    params: list[str],
    param_types: list[str],
    return_type: str,
) -> str | None:
    return _body_template_for_entries(
        entries(),
        (concept_name, concept_name.removeprefix("concept:")),
        params,
        param_types,
        return_type,
    )


def term_body_for(
    term_surface: str,
    params: list[str],
    param_types: list[str],
    return_type: str,
) -> TermBody | None:
    surface = term_surface.strip()
    if not surface:
        return None

    return_arg = _single_call_arg(surface, "return")
    if return_arg is not None:
        expr = _lower_term_expression(return_arg, params, param_types, return_type)
        if expr is None:
            return None
        if expr.stub_body is not None:
            return TermBody(expr.stub_body, True)
        return TermBody(f"return {expr.text}", False)

    if surface.startswith("method:"):
        method_body = _lower_method_template_body(surface, params, param_types, return_type)
        if method_body is not None:
            return TermBody(method_body, False)
        expr = _lower_term_expression(surface, params, param_types, return_type)
        if expr is None:
            return None
        if expr.stub_body is not None:
            return TermBody(expr.stub_body, True)
        return TermBody(f"return {expr.text}", False)

    if surface.startswith("call:"):
        expr = _lower_term_expression(surface, params, param_types, return_type)
        if expr is None:
            return None
        if expr.stub_body is not None:
            return TermBody(expr.stub_body, True)
        return TermBody(f"return {expr.text}", False)

    if surface.startswith("let("):
        return _lower_let_body(surface, params, param_types, return_type)

    return None


def _body_template_for_entries(
    body_entries: tuple[BodyTemplateEntry, ...],
    candidate_names: tuple[str, ...],
    params: list[str],
    param_types: list[str],
    return_type: str,
) -> str | None:
    mapped_param_types = [map_source_type(ty) for ty in param_types]
    mapped_return_type = map_source_type(return_type)
    for entry in body_entries:
        if entry.concept_name not in candidate_names:
            continue
        if entry.min_params is not None and len(params) < entry.min_params:
            continue
        if entry.max_params is not None and len(params) > entry.max_params:
            continue
        if entry.requires_param_types is not None:
            if tuple(mapped_param_types) != entry.requires_param_types:
                continue
        if entry.requires_return_type is not None:
            if mapped_return_type != entry.requires_return_type:
                continue
        if entry.template_kind != "verbatim":
            continue
        rendered = render_template(entry.template, params, mapped_param_types, mapped_return_type)
        if rendered is None:
            continue
        return rendered
    return None


def _lower_term_expression(
    term_surface: str,
    params: list[str],
    param_types: list[str],
    return_type: str,
) -> TermExpression | None:
    surface = term_surface.strip()
    return_arg = _single_call_arg(surface, "return")
    if return_arg is not None:
        return _lower_term_expression(return_arg, params, param_types, return_type)
    if surface.startswith("method:"):
        return _lower_method_expression(surface, params, param_types, return_type)
    if surface.startswith("call:"):
        return _lower_call_expression(surface, params, param_types, return_type)
    if surface.startswith("let("):
        return None
    return TermExpression(text=surface)


def _lower_method_template_body(
    surface: str,
    params: list[str],
    param_types: list[str],
    return_type: str,
) -> str | None:
    method = _parse_method_surface(surface)
    if method is None:
        return None
    method_name, receiver, args = method
    return _libprovekit_method_template(method_name, receiver, args, params, param_types, return_type)


def _lower_method_expression(
    surface: str,
    params: list[str],
    param_types: list[str],
    return_type: str,
) -> TermExpression | None:
    method = _parse_method_surface(surface)
    if method is None:
        return None
    method_name, receiver, args = method
    template = _libprovekit_method_template(
        method_name,
        receiver,
        args,
        params,
        param_types,
        return_type,
    )
    if template is not None:
        expression = _single_return_expression(template)
        if expression is not None:
            return TermExpression(text=expression)
    receiver_expr = _lower_argument_expression(receiver, params, param_types, return_type)
    if receiver_expr.stub_body is not None:
        return receiver_expr
    arg_exprs = _lower_argument_list(args, params, param_types, return_type)
    if arg_exprs is None:
        return None
    return TermExpression(text=f"{receiver_expr.text}.{method_name}({', '.join(arg_exprs)})")


def _lower_call_expression(
    surface: str,
    params: list[str],
    param_types: list[str],
    return_type: str,
) -> TermExpression | None:
    parsed = _parse_call_surface(surface)
    if parsed is None:
        return None
    path, args = parsed
    arg_exprs = _lower_argument_list(args, params, param_types, return_type)
    if arg_exprs is None:
        return None
    runtime_concept = _rust_runtime_call_concept(path)
    if runtime_concept is not None:
        arg_types = [_type_for_argument(arg, params, param_types) for arg in args]
        template = _body_template_expression_for(
            runtime_concept,
            arg_exprs,
            arg_types,
            return_type,
        )
        if template is not None:
            return TermExpression(text=template)
    if "::" in path:
        return TermExpression(stub_body=_unsupported_call_stub(path, surface))
    return TermExpression(text=f"{path}({', '.join(arg_exprs)})")


def _lower_let_body(
    surface: str,
    params: list[str],
    param_types: list[str],
    return_type: str,
) -> TermBody | None:
    inner = _single_call_arg(surface, "let")
    if inner is None:
        return None
    args = _split_top_level(inner)
    if len(args) not in {2, 3}:
        return None
    pattern = _lower_pattern(args[0])
    rhs = _lower_term_expression(args[1], params, param_types, return_type)
    if rhs is None:
        return None
    if rhs.stub_body is not None:
        return TermBody(rhs.stub_body, True)
    head = f"{pattern} = {rhs.text}"
    if len(args) == 2:
        return TermBody(head, False)
    continuation = _lower_term_body(args[2], params, param_types, return_type)
    if continuation is None:
        return None
    if continuation.is_stub:
        return continuation
    if not continuation.body:
        return TermBody(head, False)
    return TermBody(f"{head}\n{continuation.body}", False)


def _lower_term_body(
    surface: str,
    params: list[str],
    param_types: list[str],
    return_type: str,
) -> TermBody | None:
    stripped = surface.strip()
    if stripped == "skip":
        return TermBody("", False)
    return_arg = _single_call_arg(stripped, "return")
    if return_arg is not None:
        expr = _lower_term_expression(return_arg, params, param_types, return_type)
        if expr is None:
            return None
        if expr.stub_body is not None:
            return TermBody(expr.stub_body, True)
        return TermBody(f"return {expr.text}", False)
    if stripped.startswith("let("):
        return _lower_let_body(stripped, params, param_types, return_type)
    expr = _lower_term_expression(stripped, params, param_types, return_type)
    if expr is None:
        return None
    if expr.stub_body is not None:
        return TermBody(expr.stub_body, True)
    return TermBody(expr.text or "", False)


def _lower_pattern(pattern: str) -> str:
    raw = pattern.strip()
    bind_arg = _single_call_arg(raw, "pattern_bind")
    if bind_arg is not None:
        raw = bind_arg.strip()
    ascription = _split_type_ascription(raw)
    if ascription is None:
        return raw
    name, type_name = ascription
    return f"{name}: {map_source_type(type_name)}"


def _lower_argument_list(
    args: list[str],
    params: list[str],
    param_types: list[str],
    return_type: str,
) -> list[str] | None:
    lowered: list[str] = []
    for arg in args:
        expr = _lower_argument_expression(arg, params, param_types, return_type)
        if expr.stub_body is not None or expr.text is None:
            return None
        lowered.append(expr.text)
    return lowered


def _lower_argument_expression(
    arg: str,
    params: list[str],
    param_types: list[str],
    return_type: str,
) -> TermExpression:
    expr = _lower_term_expression(arg, params, param_types, return_type)
    if expr is not None:
        return expr
    return TermExpression(text=arg.strip())


def _libprovekit_method_template(
    method_name: str,
    receiver: str,
    args: list[str],
    params: list[str],
    param_types: list[str],
    return_type: str,
) -> str | None:
    if not _simple_receiver_name(receiver):
        return None
    receiver_key = _concept_key(receiver)
    method_key = _concept_key(method_name)
    candidates = (
        f"concept:{receiver_key}-{method_key}",
        f"{receiver_key}-{method_key}",
    )
    arg_types = [_type_for_argument(arg, params, param_types) for arg in args]
    return _body_template_for_entries(
        libprovekit_entries(),
        candidates,
        [arg.strip() for arg in args],
        arg_types,
        return_type,
    )


def _body_template_expression_for(
    concept_name: str,
    params: list[str],
    param_types: list[str],
    return_type: str,
) -> str | None:
    body = _body_template_for_entries(
        entries(),
        (concept_name, concept_name.removeprefix("concept:")),
        params,
        param_types,
        return_type,
    )
    if body is None:
        return None
    return _single_return_expression(body)


def _rust_runtime_call_concept(path: str) -> str | None:
    match path:
        case "String::with_capacity":
            return "concept:string-with-capacity"
    if path.endswith("::new"):
        return "concept:new"
    return None


def _parse_method_surface(surface: str) -> tuple[str, str, list[str]] | None:
    head_args = _head_and_args(surface.removeprefix("method:"))
    if head_args is None:
        return None
    method_name, inner = head_args
    args = _split_top_level(inner)
    if len(args) != 2:
        return None
    method_args = _parse_bracket_list(args[1])
    if method_args is None:
        return None
    return method_name.strip(), args[0].strip(), method_args


def _parse_call_surface(surface: str) -> tuple[str, list[str]] | None:
    head_args = _head_and_args(surface.removeprefix("call:"))
    if head_args is None:
        return None
    path, inner = head_args
    args = _split_top_level(inner)
    if len(args) == 1 and args[0] == "":
        return path.strip(), []
    if len(args) == 2:
        legacy_args = _parse_bracket_list(args[1])
        if legacy_args is not None:
            return args[0].strip(), legacy_args
    return path.strip(), args


def _single_call_arg(surface: str, head: str) -> str | None:
    head_args = _head_and_args(surface)
    if head_args is None:
        return None
    got_head, inner = head_args
    if got_head != head:
        return None
    return inner


def _head_and_args(surface: str) -> tuple[str, str] | None:
    stripped = surface.strip()
    if not stripped.endswith(")"):
        return None
    index = stripped.find("(")
    if index <= 0:
        return None
    return stripped[:index], stripped[index + 1 : -1]


def _parse_bracket_list(surface: str) -> list[str] | None:
    stripped = surface.strip()
    if not stripped.startswith("[") or not stripped.endswith("]"):
        return None
    inner = stripped[1:-1].strip()
    if not inner:
        return []
    return _split_top_level(inner)


def _split_top_level(text: str) -> list[str]:
    parts: list[str] = []
    start = 0
    depth = 0
    quote: str | None = None
    escaped = False
    for index, ch in enumerate(text):
        if quote is not None:
            if escaped:
                escaped = False
            elif ch == "\\":
                escaped = True
            elif ch == quote:
                quote = None
            continue
        if ch in {"'", '"'}:
            quote = ch
            continue
        if ch in "([{":
            depth += 1
            continue
        if ch in ")]}":
            depth -= 1
            continue
        if ch == "," and depth == 0:
            parts.append(text[start:index].strip())
            start = index + 1
    parts.append(text[start:].strip())
    return parts


def _split_type_ascription(pattern: str) -> tuple[str, str] | None:
    depth = 0
    for index, ch in enumerate(pattern):
        if ch in "([{":
            depth += 1
            continue
        if ch in ")]}":
            depth -= 1
            continue
        if ch != ":" or depth != 0:
            continue
        previous_ch = pattern[index - 1] if index > 0 else ""
        next_ch = pattern[index + 1] if index + 1 < len(pattern) else ""
        if previous_ch == ":" or next_ch == ":":
            continue
        name = pattern[:index].strip()
        type_name = pattern[index + 1 :].strip()
        if name and type_name:
            return name, type_name
    return None


def _single_return_expression(body: str) -> str | None:
    stripped = body.strip()
    if "\n" in stripped:
        return None
    if not stripped.startswith("return "):
        return None
    return stripped.removeprefix("return ").strip()


def _unsupported_call_stub(path: str, surface: str) -> str:
    message = _double_quoted(f"provekit-bind canonical: call:{path}")
    return (
        f"# provekit-realize-python: unsupported canonical call `{path}`; "
        f"no Python shim matched `{surface}`\n"
        f"raise NotImplementedError({message})"
    )


def _double_quoted(value: str) -> str:
    escaped = value.replace("\\", "\\\\").replace('"', '\\"')
    return f'"{escaped}"'


def _simple_receiver_name(receiver: str) -> bool:
    return re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", receiver.strip()) is not None


def _concept_key(value: str) -> str:
    return value.strip().replace("_", "-").lower()


def _type_for_argument(arg: str, params: list[str], param_types: list[str]) -> str:
    stripped = arg.strip()
    for index, param in enumerate(params):
        if stripped == param and index < len(param_types):
            return param_types[index]
    return ""


def map_source_type(src: str) -> str:
    match src:
        case "()":
            return "None"
        case (
            "i128"
            | "u128"
            | "i64"
            | "u64"
            | "i32"
            | "u32"
            | "i16"
            | "u16"
            | "i8"
            | "u8"
            | "isize"
            | "usize"
            | "int"
        ):
            return "int"
        case "f64" | "f32" | "float":
            return "float"
        case "bool":
            return "bool"
        case "String" | "&str" | "&String" | "str":
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
    return _entries_from_files((BODY_TEMPLATE_REL, RUST_RUNTIME_BODY_TEMPLATE_REL))


@lru_cache(maxsize=1)
def libprovekit_entries() -> tuple[BodyTemplateEntry, ...]:
    return _entries_from_file(LIBPROVEKIT_BODY_TEMPLATE_REL)


def _entries_from_files(relatives: tuple[Path, ...]) -> tuple[BodyTemplateEntry, ...]:
    out: list[BodyTemplateEntry] = []
    for relative in relatives:
        out.extend(_entries_from_file(relative))
    return tuple(out)


def _entries_from_file(relative: Path) -> tuple[BodyTemplateEntry, ...]:
    path = _find_repo_file(relative)
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


def _function_source(
    function: str,
    params: list[str],
    body: str,
    *,
    leading_lines: list[str] | None = None,
) -> str:
    param_list = ", ".join(params)
    body_lines = body.splitlines() or ["pass"]
    indented = "\n".join(f"    {line}" if line else "" for line in body_lines)
    prefix = ""
    if leading_lines:
        prefix = "\n".join(leading_lines) + "\n"
    return f"{prefix}def {function}({param_list}):\n{indented}\n"


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
