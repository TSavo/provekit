from __future__ import annotations

import json
import os
import re
from dataclasses import dataclass
from functools import lru_cache
from pathlib import Path
from typing import Any

BODY_TEMPLATE_REL = Path(
    "menagerie/python-language-signature/specs/body-templates/python-canonical-bodies-requests.json"
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
class ProofBindingCandidate:
    admission_tier: str
    body: str
    concept_name: str
    package_evidence: dict[str, Any]
    signature_shape_cid: str
    target_language: str
    target_library_tag: str


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
    *,
    signature_shape_cid: str | None = None,
    binding_candidates: list[dict[str, Any]] | None = None,
    strict_proof_bindings: bool = False,
) -> dict[str, Any]:
    proof_binding = proof_binding_for(
        concept_name=concept_name,
        signature_shape_cid=signature_shape_cid,
        binding_candidates=binding_candidates or [],
    )
    if proof_binding is not None:
        return {
            "source": _function_source(function, params, proof_binding.body),
            "is_stub": False,
            "extension": "py",
            "selection": {
                "admission_tier": proof_binding.admission_tier,
                "package_evidence": proof_binding.package_evidence,
                "signature_shape_cid": proof_binding.signature_shape_cid,
                "source": "proof-backed-library-binding",
            },
        }
    if strict_proof_bindings:
        raise _missing_template(function, params, param_types, concept_name)
    body = body_template_for(concept_name, params, param_types, return_type)
    if body is None:
        raise _missing_template(function, params, param_types, concept_name)
    return {
        "source": _function_source(function, params, body),
        "is_stub": False,
        "extension": "py",
    }


def proof_binding_for(
    *,
    concept_name: str,
    signature_shape_cid: str | None,
    binding_candidates: list[dict[str, Any]],
) -> ProofBindingCandidate | None:
    if not signature_shape_cid:
        return None
    matches: list[ProofBindingCandidate] = []
    candidate_names = (concept_name, concept_name.removeprefix("concept:"))
    for raw in binding_candidates:
        candidate = _parse_proof_binding_candidate(raw)
        if candidate is None:
            continue
        if candidate.target_language != "python":
            continue
        if candidate.target_library_tag != "requests":
            continue
        if candidate.concept_name not in candidate_names:
            continue
        if candidate.signature_shape_cid != signature_shape_cid:
            continue
        matches.append(candidate)
    matches.sort(key=_proof_binding_sort_key)
    return matches[0] if matches else None


def _parse_proof_binding_candidate(raw: dict[str, Any]) -> ProofBindingCandidate | None:
    body = raw.get("body")
    concept_name = raw.get("concept_name")
    signature_shape_cid = raw.get("signature_shape_cid")
    target_language = raw.get("target_language")
    target_library_tag = raw.get("target_library_tag")
    if not isinstance(body, str):
        return None
    if not isinstance(concept_name, str):
        return None
    if not isinstance(signature_shape_cid, str):
        return None
    if not isinstance(target_language, str):
        return None
    if not isinstance(target_library_tag, str):
        return None
    package_evidence = raw.get("package_evidence")
    if not isinstance(package_evidence, dict):
        package_evidence = {}
    admission_tier = raw.get("admission_tier")
    if not isinstance(admission_tier, str):
        admission_tier = "Third-party Inferred"
    return ProofBindingCandidate(
        admission_tier=admission_tier,
        body=body,
        concept_name=concept_name,
        package_evidence=package_evidence,
        signature_shape_cid=signature_shape_cid,
        target_language=target_language,
        target_library_tag=target_library_tag,
    )


def _proof_binding_sort_key(candidate: ProofBindingCandidate) -> tuple[int, str, str]:
    tier_rank = {
        "Self-Attested": 0,
        "Third-party Inferred": 1,
    }
    return (
        tier_rank.get(candidate.admission_tier, 2),
        candidate.signature_shape_cid,
        candidate.body,
    )


def _missing_template(
    function: str, params: list[str], param_types: list[str], concept_name: str
) -> MissingTemplateError:
    return MissingTemplateError(
        (
            MissingTemplateEntry(
                operation_kind=concept_name,
                args_shape=tuple(map_source_type(ty) for ty in param_types),
                function=function,
                term_position="body",
            ),
        )
    )


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


def _function_source(function: str, params: list[str], body: str) -> str:
    param_list = ", ".join(params)
    body_lines = body.splitlines() or ["pass"]
    indented = "\n".join(f"    {line}" if line else "" for line in body_lines)
    return f"def {function}({param_list}):\n{indented}\n"


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
