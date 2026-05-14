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
    mapped_param_types = [map_source_type(ty) for ty in param_types]
    mapped_return_type = map_source_type(return_type)
    candidate_names = (concept_name, concept_name.removeprefix("concept:"))
    for entry in entries():
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


def map_source_type(src: str) -> str:
    match src:
        case "()":
            return "None"
        case "i64" | "u64" | "i32" | "u32" | "i16" | "u16" | "i8" | "u8" | "int":
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
