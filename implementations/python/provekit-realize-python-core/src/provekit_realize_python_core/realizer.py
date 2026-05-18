from __future__ import annotations

import json
import os
import re
from dataclasses import dataclass, field
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
BLAKE3_BODY_TEMPLATE_REL = Path(
    "menagerie/python-language-signature/specs/body-templates/python-canonical-bodies-blake3.json"
)
PLACEHOLDER_RE = re.compile(r"\$\{[^}]+\}")
CID_RE = re.compile(r"^blake3-512:[0-9a-f]{128}$")
KIT_ID = "provekit-realize-python-core@0.1.0"
DEFAULT_KIT_CID = "blake3-512:" + blake3.blake3(KIT_ID.encode("utf-8")).digest(
    length=64
).hex()
DEFAULT_POLICY_CID = "blake3-512:" + blake3.blake3(
    b"provekit-realize-python-core/default-contract-comment-policy"
).digest(length=64).hex()
DEFAULT_SUGAR_DICT_CID = "blake3-512:" + blake3.blake3(
    b"provekit-realize-python-core/contract-comment-sugar-v1"
).digest(length=64).hex()
DEFAULT_CONCEPT_POLICY_CID = "blake3-512:" + blake3.blake3(
    b"provekit-realize-python-core/default-concept-citation-policy"
).digest(length=64).hex()
DEFAULT_CONCEPT_SUGAR_DICT_CID = "blake3-512:" + blake3.blake3(
    b"provekit-realize-python-core/concept-citation-comment-sugar-v1"
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
    loss_record_contribution: dict[str, Any] | None = None


@dataclass(frozen=True)
class TermBody:
    body: str


@dataclass(frozen=True)
class TermExpression:
    text: str | None = None
    type_name: str = ""


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


class OperandBindingMisalignmentError(Exception):
    def __init__(self, missing_positions: list[list[int]], extra_positions: list[list[int]]):
        self.missing_positions = missing_positions
        self.extra_positions = extra_positions
        super().__init__(
            "operand binding misalignment: "
            f"missing_positions={missing_positions} "
            f"extra_positions={extra_positions}"
        )


def emit_stub(
    function: str,
    params: list[str],
    param_types: list[str],
    return_type: str,
    concept_name: str,
    contract: dict[str, Any] | None = None,
    transported_op: dict[str, Any] | None = None,
    sugar_cids: list[str] | None = None,
    sugar_plugins: list[Any] | None = None,
    named_term_tree: dict[str, Any] | None = None,
    term_shape: dict[str, Any] | None = None,
    operand_bindings: list[dict[str, Any]] | None = None,
    source_function_name: str | None = None,
    annotate: bool = False,
) -> dict[str, Any]:
    sugar_cids_value = sugar_cids or []
    sugar_plugins_value = sugar_plugins or []
    observed_loss_record: dict[str, Any] | None = None
    if transported_op is None and named_term_tree is None and term_shape is None:
        carrier_entry = sugar_carrier_entry_for(
            concept_name,
            params,
            param_types,
            return_type,
        )
        if carrier_entry is not None:
            observed_loss_record = _loss_record_from_contribution(
                carrier_entry.loss_record_contribution
            )
            transported_op = _sugar_carrier_transported_op(
                function,
                params,
                concept_name,
                observed_loss_record,
            )
    concept_lines = concept_citation_comment_lines(
        transported_op,
        sugar_cids_value,
        sugar_plugins_value,
    )
    if concept_lines:
        body = "\n".join([*concept_lines, "pass"])
    else:
        if named_term_tree is None:
            if term_shape is None:
                missing = missing_templates_for(
                    function,
                    params,
                    param_types,
                    return_type,
                    concept_name,
                )
            else:
                missing = missing_templates_for_term_shape(
                    function,
                    params,
                    param_types,
                    return_type,
                    term_shape,
                    operand_bindings=operand_bindings,
                )
        else:
            missing = missing_templates_for_tree(
                function,
                params,
                param_types,
                return_type,
                named_term_tree,
            )
        if missing:
            raise MissingTemplateError(missing)

        if named_term_tree is None:
            if term_shape is None:
                term_body = term_body_for(concept_name, params, param_types, return_type)
            else:
                term_body = term_body_for_term_shape(
                    term_shape,
                    params,
                    param_types,
                    return_type,
                    operand_bindings=operand_bindings,
                )
        else:
            term_body = term_body_for_tree(
                named_term_tree,
                params,
                param_types,
                return_type,
                annotate=annotate,
            )
        if term_body is not None:
            body = term_body.body
        else:
            body = body_template_for(concept_name, params, param_types, return_type)
            if body is None:
                raise MissingTemplateError(
                    (
                        MissingTemplateEntry(
                            operation_kind=concept_name,
                            args_shape=tuple(map_source_type(ty) for ty in param_types),
                            function=function,
                            term_position="body",
                        ),
                    )
                )
    contract_lines = contract_comment_lines(contract, sugar_cids_value, sugar_plugins_value)
    emitted_function = source_function_name or function
    result = {
        "source": _function_source(emitted_function, params, body, leading_lines=contract_lines),
        "is_stub": False,
        "extension": "py",
    }
    if contract_lines or concept_lines:
        result["observed_loss_record"] = observed_loss_record or {}
        result["used_sugars"] = [sugar_cids_value[0]] if sugar_cids_value else []
    return result


def missing_templates_for(
    function: str,
    params: list[str],
    param_types: list[str],
    return_type: str,
    concept_name: str,
) -> tuple[MissingTemplateEntry, ...]:
    collector = _MissingTemplateCollector(function, params, param_types, return_type)
    return collector.collect(concept_name)


def missing_templates_for_tree(
    function: str,
    params: list[str],
    param_types: list[str],
    return_type: str,
    named_term_tree: dict[str, Any],
) -> tuple[MissingTemplateEntry, ...]:
    collector = _MissingTemplateCollector(function, params, param_types, return_type)
    return collector.collect_tree(named_term_tree)


def missing_templates_for_term_shape(
    function: str,
    params: list[str],
    param_types: list[str],
    return_type: str,
    term_shape: dict[str, Any],
    *,
    operand_bindings: list[dict[str, Any]] | None = None,
) -> tuple[MissingTemplateEntry, ...]:
    if (
        term_body_for_term_shape(
            term_shape,
            params,
            param_types,
            return_type,
            operand_bindings=operand_bindings,
        )
        is not None
    ):
        return ()
    concept_name = _shape_concept_name(term_shape) or "termShape"
    return (
        MissingTemplateEntry(
            operation_kind=concept_name,
            args_shape=tuple(map_source_type(ty) for ty in param_types),
            function=function,
            term_position="body.termShape",
        ),
    )


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


def concept_citation_comment_lines(
    transported_op: dict[str, Any] | None,
    sugar_cids: list[str],
    sugar_plugins: list[Any],
) -> list[str]:
    if not isinstance(transported_op, dict):
        return []
    payload = _concept_citation_payload(transported_op, sugar_cids, sugar_plugins)
    if payload is None:
        return []
    payload_cid = _cid_of_json(payload)
    payload_json = json.dumps(
        payload,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    )
    return [
        f"# provekit-concept: {payload_json}",
        f"# provekit-concept-payload-cid: {payload_cid}",
    ]


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


def _concept_citation_payload(
    transported_op: dict[str, Any],
    sugar_cids: list[str],
    sugar_plugins: list[Any],
) -> dict[str, Any] | None:
    concept_cid = _transported_cid(transported_op, "concept_cid", "conceptCid")
    concept_site_cid = _transported_cid(
        transported_op,
        "concept_site_cid",
        "conceptSiteCid",
    )
    loss_record_cid = _transported_cid(
        transported_op,
        "loss_record_cid",
        "lossRecordCid",
    )
    shape_cid = _transported_cid(transported_op, "shape_cid", "shapeCid")
    operation_kind = _transported_str(
        transported_op,
        "operation_kind",
        "operationKind",
    )
    term_position = _transported_term_position(
        _transported_value(transported_op, "term_position", "termPosition")
    )
    if (
        concept_cid is None
        or concept_site_cid is None
        or loss_record_cid is None
        or shape_cid is None
        or operation_kind is None
        or term_position is None
    ):
        return None

    args_jcs = _transported_value(transported_op, "args_jcs", "argsJcs")
    payload: dict[str, Any] = {
        "artifact_kind": "provekit-concept-citation-comment-sugar",
        "concept_cid": concept_cid,
        "concept_site_cid": concept_site_cid,
        "emitted_by": {
            "kit_cid": DEFAULT_KIT_CID,
            "kit_id": KIT_ID,
            "kit_kind": "realize",
            "target_language": "python",
            "target_library_tag": _transported_str(
                transported_op,
                "target_library_tag",
                "targetLibraryTag",
            )
            or "python",
        },
        "loss_record_cid": loss_record_cid,
        "operation_kind": operation_kind,
        "policy_cid": _concept_policy_cid(transported_op),
        "schema_version": "1",
        "shape_cid": shape_cid,
        "sugar_dict_cid": _concept_sugar_dict_cid(
            transported_op,
            sugar_cids,
            sugar_plugins,
        ),
        "term_position": term_position,
    }

    if args_jcs is not None:
        if not isinstance(args_jcs, list):
            return None
        payload["args_jcs"] = args_jcs
        payload["args_jcs_cid"] = _cid_of_json(args_jcs)
    else:
        args_jcs_cid = _transported_cid(transported_op, "args_jcs_cid", "argsJcsCid")
        if args_jcs_cid is None:
            return None
        payload["args_jcs_cid"] = args_jcs_cid

    callsite_cid = _transported_value(transported_op, "callsite_cid", "callsiteCid")
    if callsite_cid is not None:
        if not isinstance(callsite_cid, str) or not CID_RE.fullmatch(callsite_cid):
            return None
        payload["callsite_cid"] = callsite_cid
    concept_name = _transported_str(transported_op, "concept_name", "conceptName")
    if concept_name is not None:
        payload["concept_name"] = concept_name
    return payload


def _sugar_carrier_transported_op(
    function: str,
    params: list[str],
    concept_name: str,
    loss_record: dict[str, Any],
) -> dict[str, Any] | None:
    concept_cid = _catalog_concept_cid(concept_name)
    if concept_cid is None:
        return None
    args_jcs = [{"kind": "var", "name": param} for param in params]
    return {
        "args_jcs": args_jcs,
        "concept_cid": concept_cid,
        "concept_name": concept_name,
        "concept_site_cid": _cid_of_json(
            {
                "concept_name": concept_name,
                "function": function,
                "surface": "python-concept-citation-sugar-carrier",
            }
        ),
        "loss_record_cid": _cid_of_json(loss_record),
        "operation_kind": concept_name.removeprefix("concept:"),
        "policy_cid": DEFAULT_CONCEPT_POLICY_CID,
        "shape_cid": concept_cid,
        "sugar_dict_cid": DEFAULT_CONCEPT_SUGAR_DICT_CID,
        "term_position": [0],
    }


def _loss_record_from_contribution(
    contribution: dict[str, Any] | None,
) -> dict[str, Any]:
    if contribution is None:
        contribution = {"form": "literal", "value": {}}
    return {"loss_record_contribution": contribution}


@lru_cache(maxsize=1)
def _concept_catalog_index() -> dict[str, Any]:
    path = _find_repo_file(Path("menagerie/concept-shapes/catalog/index.json"))
    if path is None:
        return {}
    return json.loads(path.read_text(encoding="utf-8"))


def _catalog_concept_cid(concept_name: str) -> str | None:
    index = _concept_catalog_index()
    entries = index.get("entries")
    if not isinstance(entries, dict):
        return None
    for entry in entries.values():
        if not isinstance(entry, dict):
            continue
        if entry.get("kind") != "algorithm" or entry.get("name") != concept_name:
            continue
        cid = entry.get("cid")
        if isinstance(cid, str) and CID_RE.fullmatch(cid):
            return cid
    return None


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


def _concept_policy_cid(transported_op: dict[str, Any]) -> str:
    value = _transported_cid(transported_op, "policy_cid", "policyCid")
    if value is not None:
        return value
    return DEFAULT_CONCEPT_POLICY_CID


def _concept_sugar_dict_cid(
    transported_op: dict[str, Any],
    sugar_cids: list[str],
    sugar_plugins: list[Any],
) -> str:
    value = _transported_cid(transported_op, "sugar_dict_cid", "sugarDictCid")
    if value is not None:
        return value
    for cid in sugar_cids:
        if CID_RE.fullmatch(cid):
            return cid
    for plugin in sugar_plugins:
        if not isinstance(plugin, dict):
            continue
        header = plugin.get("header")
        cid = header.get("cid") if isinstance(header, dict) else None
        if isinstance(cid, str) and CID_RE.fullmatch(cid):
            return cid
    return DEFAULT_CONCEPT_SUGAR_DICT_CID


def _transported_cid(transported_op: dict[str, Any], *keys: str) -> str | None:
    value = _transported_value(transported_op, *keys)
    if isinstance(value, str) and CID_RE.fullmatch(value):
        return value
    return None


def _transported_str(transported_op: dict[str, Any], *keys: str) -> str | None:
    value = _transported_value(transported_op, *keys)
    if isinstance(value, str) and value:
        return value
    return None


def _transported_value(transported_op: dict[str, Any], *keys: str) -> Any:
    for key in keys:
        if key in transported_op:
            return transported_op[key]
    return None


def _transported_term_position(value: Any) -> list[int] | None:
    if not isinstance(value, list):
        return None
    out: list[int] = []
    for item in value:
        if isinstance(item, bool) or not isinstance(item, int) or item < 0:
            return None
        out.append(item)
    return out


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


def sugar_carrier_entry_for(
    concept_name: str,
    params: list[str],
    param_types: list[str],
    return_type: str,
) -> BodyTemplateEntry | None:
    mapped_param_types = [map_source_type(ty) for ty in param_types]
    mapped_return_type = map_source_type(return_type)
    candidate_names = (concept_name, concept_name.removeprefix("concept:"))
    for entry in entries():
        if entry.concept_name not in candidate_names:
            continue
        if entry.template_kind != "concept-citation-comment":
            continue
        if not _entry_signature_matches(
            entry,
            len(params),
            mapped_param_types,
            mapped_return_type,
        ):
            continue
        return entry
    return None


def _entry_signature_matches(
    entry: BodyTemplateEntry,
    param_count: int,
    mapped_param_types: list[str],
    mapped_return_type: str,
) -> bool:
    if entry.min_params is not None and param_count < entry.min_params:
        return False
    if entry.max_params is not None and param_count > entry.max_params:
        return False
    if entry.requires_param_types is not None:
        if tuple(mapped_param_types) != entry.requires_param_types:
            return False
    if entry.requires_return_type is not None:
        if mapped_return_type != entry.requires_return_type:
            return False
    return True


class _MissingTemplateCollector:
    def __init__(
        self,
        function: str,
        params: list[str],
        param_types: list[str],
        return_type: str,
    ) -> None:
        self.function = function
        self.params = params
        self.param_types = param_types
        self.return_type = return_type
        self.entries: list[MissingTemplateEntry] = []

    def collect(self, surface: str) -> tuple[MissingTemplateEntry, ...]:
        stripped = surface.strip()
        if self._is_term_surface(stripped):
            self._collect_body(stripped, "body")
        elif body_template_for(stripped, self.params, self.param_types, self.return_type) is None:
            self._add(
                operation_kind=stripped,
                args_shape=tuple(map_source_type(ty) for ty in self.param_types),
                term_position="body",
            )
        return tuple(self.entries)

    def collect_tree(self, tree: dict[str, Any]) -> tuple[MissingTemplateEntry, ...]:
        self._collect_tree(tree, "body.namedTermTree")
        return tuple(self.entries)

    def _collect_tree(self, tree: Any, position: str) -> None:
        if not isinstance(tree, dict):
            self._add("namedTermTree", (), position)
            return
        concept_name = _tree_concept_name(tree)
        if not concept_name:
            self._add("namedTermTree", (), position)
            return
        operation_kind = _tree_operation_kind(tree)
        args = _tree_args(tree)
        if not _is_tree_composer(concept_name, operation_kind):
            template_params, template_types = self._tree_template_signature(args)
            if (
                body_template_for(
                    concept_name,
                    template_params,
                    template_types,
                    self.return_type,
                )
                is None
            ):
                self._add(
                    concept_name,
                    tuple(map_source_type(ty) for ty in template_types),
                    position,
                )
        for index, child in enumerate(args):
            self._collect_tree(child, f"{position}.args[{index}]")

    def _tree_template_signature(
        self,
        args: list[dict[str, Any]],
    ) -> tuple[list[str], list[str]]:
        if not args:
            return self.params, self.param_types
        names = [f"arg{index}" for index, _child in enumerate(args)]
        types = [
            _tree_concept_name(child) or _tree_operation_kind(child) or "expr"
            for child in args
        ]
        return names, types

    def _is_term_surface(self, surface: str) -> bool:
        if surface == "skip":
            return True
        if surface.startswith(("method:", "call:", "let(")):
            return True
        head_args = _head_and_args(surface)
        if head_args is None:
            return False
        head, _inner = head_args
        return bool(head)

    def _collect_body(self, surface: str, position: str) -> None:
        stripped = surface.strip()
        if stripped == "skip":
            return
        return_arg = _single_call_arg(stripped, "return")
        if return_arg is not None:
            self._collect_expression(return_arg, f"{position}.return")
            return
        if stripped.startswith("let("):
            self._collect_let(stripped, position)
            return
        self._collect_expression(stripped, position)

    def _collect_let(self, surface: str, position: str) -> None:
        inner = _single_call_arg(surface, "let")
        if inner is None:
            self._add("let", self._args_shape([surface]), position)
            return
        args = _split_top_level(inner)
        if len(args) not in {2, 3}:
            self._add("let", self._args_shape(args), position)
            return
        self._collect_expression(args[1], f"{position}.let.rhs")
        if len(args) == 3:
            self._collect_body(args[2], f"{position}.let.cont")

    def _collect_expression(self, surface: str, position: str) -> None:
        stripped = surface.strip()
        return_arg = _single_call_arg(stripped, "return")
        if return_arg is not None:
            self._collect_expression(return_arg, f"{position}.return")
            return
        if stripped.startswith("method:"):
            self._collect_method(stripped, position)
            return
        if stripped.startswith("call:"):
            self._collect_call(stripped, position)
            return
        if stripped.startswith("let("):
            self._collect_let(stripped, position)
            return
        head_args = _head_and_args(stripped)
        if head_args is None:
            return
        op_name, inner = head_args
        raw_args = _split_top_level(inner)
        candidates = _rust_runtime_operation_concepts(op_name)
        op_position = f"{position}.{op_name}"
        if not candidates:
            self._add(op_name, self._args_shape(raw_args), op_position)
            for index, arg in enumerate(raw_args):
                self._collect_expression(arg, f"{op_position}.args[{index}]")
            return
        for index, arg in enumerate(raw_args):
            self._collect_expression(arg, f"{op_position}.args[{index}]")
        if not self._operation_template_matches(candidates, raw_args):
            self._add(candidates[0], self._args_shape(raw_args), op_position)

    def _collect_call(self, surface: str, position: str) -> None:
        parsed = _parse_call_surface(surface)
        if parsed is None:
            self._add("call", self._args_shape([surface]), position)
            return
        path, args = parsed
        op_position = f"{position}.call:{path}"
        for index, arg in enumerate(args):
            self._collect_expression(arg, f"{op_position}.args[{index}]")
        candidates = _rust_runtime_call_concepts(path)
        if candidates:
            if not self._operation_template_matches(candidates, args):
                self._add(candidates[0], self._args_shape(args), op_position)
            return
        if "::" in path:
            self._add(f"call:{path}", self._args_shape(args), op_position)

    def _collect_method(self, surface: str, position: str) -> None:
        parsed = _parse_method_surface(surface)
        if parsed is None:
            self._add("method", self._args_shape([surface]), position)
            return
        method_name, receiver, args = parsed
        op_position = f"{position}.method:{method_name}"
        self._collect_expression(receiver, f"{op_position}.receiver")
        for index, arg in enumerate(args):
            self._collect_expression(arg, f"{op_position}.args[{index}]")
        if _libprovekit_method_template(
            method_name,
            receiver,
            args,
            self.params,
            self.param_types,
            self.return_type,
        ) is not None:
            return
        rust_candidates = _rust_runtime_method_concepts(method_name, receiver)
        if _rust_runtime_method_template(
            method_name,
            receiver,
            args,
            self.params,
            self.param_types,
            self.return_type,
        ) is not None:
            return
        if rust_candidates and _requires_rust_method_template(method_name):
            self._add(
                rust_candidates[0],
                self._args_shape([receiver, *args]),
                op_position,
            )

    def _operation_template_matches(
        self,
        candidates: tuple[str, ...],
        raw_args: list[str],
    ) -> bool:
        arg_terms = _lower_argument_terms(
            raw_args,
            self.params,
            self.param_types,
            self.return_type,
        )
        if arg_terms is None:
            return True
        arg_exprs = [term.text or "" for term in arg_terms]
        arg_types = [
            _expression_type(term, arg, self.params, self.param_types)
            for term, arg in zip(arg_terms, raw_args, strict=True)
        ]
        return (
            _body_template_expression_for_candidates(
                candidates,
                arg_exprs,
                arg_types,
                self.return_type,
            )
            is not None
        )

    def _args_shape(self, args: list[str]) -> tuple[str, ...]:
        return tuple(
            _arg_shape(arg, self.params, self.param_types)
            for arg in args
            if arg.strip()
        )

    def _add(
        self,
        operation_kind: str,
        args_shape: tuple[str, ...],
        term_position: str,
    ) -> None:
        self.entries.append(
            MissingTemplateEntry(
                operation_kind=operation_kind,
                args_shape=args_shape,
                function=self.function,
                term_position=term_position,
            )
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
        return TermBody(f"return {expr.text}")

    if surface.startswith("method:"):
        method_body = _lower_method_template_body(surface, params, param_types, return_type)
        if method_body is not None:
            return TermBody(method_body)
        expr = _lower_term_expression(surface, params, param_types, return_type)
        if expr is None:
            return None
        return TermBody(f"return {expr.text}")

    if surface.startswith("call:"):
        expr = _lower_term_expression(surface, params, param_types, return_type)
        if expr is None:
            return None
        return TermBody(f"return {expr.text}")

    if surface.startswith("let("):
        return _lower_let_body(surface, params, param_types, return_type)

    return None


def term_body_for_tree(
    named_term_tree: dict[str, Any],
    params: list[str],
    param_types: list[str],
    return_type: str,
    *,
    annotate: bool = False,
) -> TermBody | None:
    return _lower_tree_body(
        named_term_tree,
        params,
        param_types,
        return_type,
        annotate=annotate,
    )


def _operand_binding_map(
    term_shape: dict[str, Any],
    operand_bindings: list[dict[str, Any]] | None,
) -> dict[tuple[int, ...], str] | None:
    if operand_bindings is None:
        return None
    mapping: dict[tuple[int, ...], str] = {}
    for item in operand_bindings:
        if not isinstance(item, dict):
            raise ValueError("operand_bindings entries must be objects")
        position = item.get("position")
        symbol = item.get("symbol")
        if (
            not isinstance(position, list)
            or not all(isinstance(part, int) and part >= 0 for part in position)
        ):
            raise ValueError("operand_bindings position must be an integer array")
        if not isinstance(symbol, str) or symbol == "":
            raise ValueError("operand_bindings symbol must be a non-empty string")
        key = tuple(position)
        if key in mapping:
            raise ValueError(f"duplicate operand_bindings position {position}")
        mapping[key] = symbol

    leaf_positions = sorted(_term_shape_leaf_positions(term_shape, ()))
    leaf_set = set(leaf_positions)
    missing = [list(position) for position in leaf_positions if position not in mapping]
    extra = [list(position) for position in sorted(mapping) if position not in leaf_set]
    if missing or extra:
        raise OperandBindingMisalignmentError(missing, extra)
    return mapping


def _term_shape_leaf_positions(shape: Any, position: tuple[int, ...]) -> list[tuple[int, ...]]:
    if not isinstance(shape, dict):
        return []
    kind = shape.get("kind")
    if kind in {"literal", "const"} or "value" in shape:
        return []
    if not _shape_concept_name(shape):
        return [position]
    out: list[tuple[int, ...]] = []
    for index, child in enumerate(_shape_args(shape)):
        out.extend(_term_shape_leaf_positions(child, (*position, index)))
    return out


def _is_identifier_symbol(symbol: str) -> bool:
    return re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", symbol) is not None


def _symbol_term(symbol: str, context: _ShapeLoweringContext) -> TermExpression:
    if symbol in {"true", "True"}:
        return TermExpression(text="True", type_name="bool")
    if symbol in {"false", "False"}:
        return TermExpression(text="False", type_name="bool")
    if symbol == "None":
        return TermExpression(text="None", type_name="None")
    if re.fullmatch(r"-?[0-9]+", symbol):
        return TermExpression(text=symbol, type_name="int")
    if len(symbol) >= 2 and symbol[0] == symbol[-1] == '"':
        return TermExpression(text=symbol, type_name="str")
    return TermExpression(
        text=symbol,
        type_name=map_source_type(_type_for_argument(symbol, context.params, context.param_types))
        or "int",
    )


@dataclass
class _ShapeLoweringContext:
    params: list[str]
    param_types: list[str]
    return_type: str
    operand_bindings: dict[tuple[int, ...], str] | None = None
    next_leaf: int = 0
    next_temp: int = 0
    defined_symbols: set[str] = field(default_factory=set)

    def __post_init__(self) -> None:
        self.defined_symbols.update(self.params)

    def fallback_leaf(self) -> TermExpression:
        if self.params:
            index = min(self.next_leaf, len(self.params) - 1)
            self.next_leaf += 1
            return TermExpression(
                text=self.params[index],
                type_name=map_source_type(self.param_types[index])
                if index < len(self.param_types)
                else "",
            )
        self.next_leaf += 1
        return TermExpression(text="0", type_name="int")

    def temp_name(self) -> str:
        name = f"_provekit_v{self.next_temp}"
        self.next_temp += 1
        return name

    def temp_name_for_seq_child(
        self,
        remaining: list[tuple[Any, tuple[int, ...]]],
    ) -> str:
        if self.operand_bindings is not None:
            for shape, position in remaining:
                for symbol in self.symbols_under(shape, position):
                    if (
                        _is_identifier_symbol(symbol)
                        and symbol not in self.defined_symbols
                    ):
                        self.defined_symbols.add(symbol)
                        return symbol
        return self.temp_name()

    def symbols_under(self, shape: Any, position: tuple[int, ...]) -> list[str]:
        if self.operand_bindings is None:
            return []
        return [
            self.operand_bindings[pos]
            for pos in _term_shape_leaf_positions(shape, position)
            if pos in self.operand_bindings
        ]


def term_body_for_term_shape(
    term_shape: dict[str, Any],
    params: list[str],
    param_types: list[str],
    return_type: str,
    *,
    operand_bindings: list[dict[str, Any]] | None = None,
) -> TermBody | None:
    binding_map = _operand_binding_map(term_shape, operand_bindings)
    context = _ShapeLoweringContext(params, param_types, return_type, binding_map)
    body = _lower_shape_body(term_shape, context, ())
    if body is not None:
        return body
    expression = _lower_shape_expression(term_shape, context, ())
    if expression is None or expression.text is None:
        return None
    return TermBody(f"return {expression.text}")


def _lower_shape_body(
    shape: Any,
    context: _ShapeLoweringContext,
    position: tuple[int, ...],
) -> TermBody | None:
    if not isinstance(shape, dict):
        return None
    concept_name = _shape_concept_name(shape)
    args = _shape_args(shape)
    if concept_name in {"concept:comment", "comment"}:
        surface = _comment_surface_from_shape(shape)
        if surface is None:
            return None
        return TermBody(_python_comment_body(surface))
    if concept_name in {"concept:skip", "skip"} and not args:
        return TermBody("")
    if concept_name in {"concept:seq", "seq"}:
        lines: list[str] = []
        for index, child in enumerate(args[:-1]):
            child_position = (*position, index)
            child_body = _lower_shape_body(child, context, child_position)
            if child_body is not None:
                if child_body.body:
                    lines.append(child_body.body)
                continue
            expression = _lower_shape_expression(child, context, child_position)
            if expression is None or expression.text is None:
                return None
            remaining = [
                (sibling, (*position, sibling_index))
                for sibling_index, sibling in enumerate(args[index + 1 :], start=index + 1)
            ]
            lines.append(f"{context.temp_name_for_seq_child(remaining)} = {expression.text}")
        if not args:
            return TermBody("")
        tail = _lower_shape_body(args[-1], context, (*position, len(args) - 1))
        if tail is not None:
            if tail.body:
                lines.append(tail.body)
            return TermBody("\n".join(lines))
        expression = _lower_shape_expression(args[-1], context, (*position, len(args) - 1))
        if expression is None or expression.text is None:
            return None
        lines.append(f"return {expression.text}")
        return TermBody("\n".join(lines))
    if concept_name in {"concept:conditional", "conditional"} and len(args) == 3:
        condition = _lower_shape_expression(args[0], context, (*position, 0))
        if condition is None or condition.text is None:
            return None
        then_body = _lower_shape_branch_body(args[1], context, (*position, 1))
        else_body = _lower_shape_branch_body(args[2], context, (*position, 2))
        if then_body is None or else_body is None:
            return None
        return TermBody(
            "\n".join(
                [
                    f"if {condition.text}:",
                    _indent_shape_block(then_body.body),
                    "else:",
                    _indent_shape_block(else_body.body),
                ]
            )
        )
    return None


def _lower_shape_branch_body(
    shape: Any,
    context: _ShapeLoweringContext,
    position: tuple[int, ...],
) -> TermBody | None:
    body = _lower_shape_body(shape, context, position)
    if body is not None:
        return body
    expression = _lower_shape_expression(shape, context, position)
    if expression is None or expression.text is None:
        return None
    return TermBody(f"return {expression.text}")


def _indent_shape_block(body: str) -> str:
    lines = body.splitlines() or ["pass"]
    return "\n".join(f"    {line}" if line else "" for line in lines)


def _lower_shape_expression(
    shape: Any,
    context: _ShapeLoweringContext,
    position: tuple[int, ...],
) -> TermExpression | None:
    if not isinstance(shape, dict):
        return None
    concept_name = _shape_concept_name(shape)
    if not concept_name:
        return _shape_leaf_expression(shape, context, position)
    args = _shape_args(shape)
    if concept_name in {"concept:seq", "seq"}:
        for index, child in enumerate(args[:-1]):
            _lower_shape_expression(child, context, (*position, index))
        if not args:
            return TermExpression(text="None", type_name="None")
        return _lower_shape_expression(args[-1], context, (*position, len(args) - 1))
    arg_terms = []
    for index, child in enumerate(args):
        term = _lower_shape_expression(child, context, (*position, index))
        if term is None or term.text is None:
            return None
        arg_terms.append(term)
    arg_exprs = [term.text or "" for term in arg_terms]
    arg_types = [term.type_name for term in arg_terms]
    if concept_name in {"concept:call", "call"} and arg_exprs:
        callee = arg_exprs[0]
        if not _is_identifier_symbol(callee):
            return None
        return TermExpression(
            text=f"{callee}({', '.join(arg_exprs[1:])})",
            type_name=map_source_type(context.return_type),
        )
    if concept_name in {"concept:skip", "skip"} and not arg_exprs:
        return TermExpression(text="None", type_name="None")
    template = _body_template_expression_for(concept_name, arg_exprs, arg_types, context.return_type)
    if template is None:
        return None
    return TermExpression(
        text=template,
        type_name=_shape_operation_return_type(concept_name, arg_terms, context.return_type),
    )


def _shape_leaf_expression(
    shape: dict[str, Any],
    context: _ShapeLoweringContext,
    position: tuple[int, ...],
) -> TermExpression:
    if context.operand_bindings is not None:
        symbol = context.operand_bindings[position]
        return _symbol_term(symbol, context)
    kind = str(shape.get("kind", ""))
    if kind == "var":
        name = shape.get("name")
        if isinstance(name, str) and name:
            return TermExpression(
                text=name,
                type_name=map_source_type(_type_for_argument(name, context.params, context.param_types)),
            )
    if kind == "const" or "value" in shape:
        return _literal_term(shape.get("value"))
    return context.fallback_leaf()


def _comment_surface_from_shape(shape: dict[str, Any]) -> str | None:
    args = _shape_args(shape)
    if not args:
        return ""
    value = args[0].get("value")
    return value if isinstance(value, str) else None


def _python_comment_body(surface: str) -> str:
    lines = surface.splitlines() or [""]
    return "\n".join(_python_comment_line(line) for line in lines)


def _python_comment_line(surface: str) -> str:
    if surface.startswith("#"):
        return surface
    if surface:
        return f"# {surface}"
    return "#"


def _literal_term(value: Any) -> TermExpression:
    if isinstance(value, bool):
        return TermExpression(text="True" if value else "False", type_name="bool")
    if isinstance(value, int):
        return TermExpression(text=str(value), type_name="int")
    if isinstance(value, float):
        return TermExpression(text=repr(value), type_name="float")
    if isinstance(value, str):
        return TermExpression(text=json.dumps(value), type_name="str")
    if value is None:
        return TermExpression(text="None", type_name="None")
    return TermExpression(text=json.dumps(value, sort_keys=True), type_name="")


def _shape_concept_name(shape: dict[str, Any]) -> str:
    value = shape.get("concept_name", shape.get("conceptName"))
    return value.strip() if isinstance(value, str) else ""


def _shape_args(shape: dict[str, Any]) -> list[dict[str, Any]]:
    args = shape.get("args")
    if not isinstance(args, list):
        return []
    return [arg for arg in args if isinstance(arg, dict)]


def _shape_operation_return_type(
    concept_name: str,
    arg_terms: list[TermExpression],
    return_type: str,
) -> str:
    op_name = concept_name.removeprefix("concept:")
    if op_name == "conditional" and len(arg_terms) >= 3:
        branch_types = [arg_terms[1].type_name, arg_terms[2].type_name]
        if branch_types[0] and branch_types[0] == branch_types[1]:
            return branch_types[0]
        return map_source_type(return_type)
    if op_name == "call":
        return map_source_type(return_type)
    if op_name == "skip":
        return "None"
    return _rust_runtime_operation_return_type(op_name, arg_terms)


def _lower_tree_body(
    tree: Any,
    params: list[str],
    param_types: list[str],
    return_type: str,
    *,
    annotate: bool,
) -> TermBody | None:
    if not isinstance(tree, dict):
        return None
    concept_name = _tree_concept_name(tree)
    if not concept_name:
        return None
    operation_kind = _tree_operation_kind(tree)
    args = _tree_args(tree)
    if _is_tree_composer(concept_name, operation_kind):
        bodies: list[str] = []
        for child in args:
            child_body = _lower_tree_body(
                child,
                params,
                param_types,
                return_type,
                annotate=annotate,
            )
            if child_body is None:
                return None
            if child_body.body:
                bodies.append(child_body.body)
        body = "\n".join(bodies)
        return TermBody(_annotated_body(body, concept_name, annotate))

    child_bodies: list[str] = []
    for child in args:
        child_body = _lower_tree_body(
            child,
            params,
            param_types,
            return_type,
            annotate=annotate,
        )
        if child_body is None:
            return None
        if child_body.body:
            child_bodies.append(child_body.body)
    body = body_template_for(concept_name, params, param_types, return_type)
    if body is None:
        return None
    if child_bodies:
        body = "\n".join([*child_bodies, body])
    return TermBody(_annotated_body(body, concept_name, annotate))


def _tree_concept_name(tree: dict[str, Any]) -> str:
    value = tree.get("conceptName", tree.get("concept_name"))
    return value.strip() if isinstance(value, str) else ""


def _tree_operation_kind(tree: dict[str, Any]) -> str:
    value = tree.get("operationKind", tree.get("operation_kind"))
    return value.strip() if isinstance(value, str) else ""


def _tree_args(tree: dict[str, Any]) -> list[dict[str, Any]]:
    args = tree.get("args")
    if not isinstance(args, list):
        return []
    return [arg for arg in args if isinstance(arg, dict)]


def _is_tree_composer(concept_name: str, operation_kind: str) -> bool:
    return operation_kind == "seq" or concept_name in {"concept:seq", "python:seq"}


def _annotated_body(body: str, concept_name: str, annotate: bool) -> str:
    if not annotate:
        return body
    if not body:
        return f"# concept: {concept_name}"
    return f"# concept: {concept_name}\n{body}"


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
        if not _entry_signature_matches(
            entry,
            len(params),
            mapped_param_types,
            mapped_return_type,
        ):
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
    operation = _lower_operation_expression(surface, params, param_types, return_type)
    if operation is not None:
        return operation
    return TermExpression(text=surface, type_name=_surface_type(surface, params, param_types))


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
            return TermExpression(text=expression, type_name=map_source_type(return_type))
    template = _rust_runtime_method_template(
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
            return TermExpression(
                text=expression,
                type_name=_rust_runtime_method_return_type(method_name),
            )
    receiver_expr = _lower_argument_expression(receiver, params, param_types, return_type)
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
    arg_terms = _lower_argument_terms(args, params, param_types, return_type)
    if arg_terms is None:
        return None
    arg_exprs = [term.text or "" for term in arg_terms]
    arg_types = [
        _expression_type(term, arg, params, param_types)
        for term, arg in zip(arg_terms, args, strict=True)
    ]
    runtime_concepts = _rust_runtime_call_concepts(path)
    if runtime_concepts:
        template = _body_template_expression_for_candidates(
            runtime_concepts,
            arg_exprs,
            arg_types,
            return_type,
        )
        if template is not None:
            return TermExpression(
                text=template,
                type_name=_rust_runtime_call_return_type(path),
            )
    if "::" in path:
        return None
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
    head = f"{pattern} = {rhs.text}"
    if len(args) == 2:
        return TermBody(head)
    continuation = _lower_term_body(args[2], params, param_types, return_type)
    if continuation is None:
        return None
    if not continuation.body:
        return TermBody(head)
    return TermBody(f"{head}\n{continuation.body}")


def _lower_term_body(
    surface: str,
    params: list[str],
    param_types: list[str],
    return_type: str,
) -> TermBody | None:
    stripped = surface.strip()
    if stripped == "skip":
        return TermBody("")
    return_arg = _single_call_arg(stripped, "return")
    if return_arg is not None:
        expr = _lower_term_expression(return_arg, params, param_types, return_type)
        if expr is None:
            return None
        return TermBody(f"return {expr.text}")
    if stripped.startswith("let("):
        return _lower_let_body(stripped, params, param_types, return_type)
    expr = _lower_term_expression(stripped, params, param_types, return_type)
    if expr is None:
        return None
    return TermBody(expr.text or "")


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
    terms = _lower_argument_terms(args, params, param_types, return_type)
    if terms is None:
        return None
    return [term.text or "" for term in terms]


def _lower_argument_terms(
    args: list[str],
    params: list[str],
    param_types: list[str],
    return_type: str,
) -> list[TermExpression] | None:
    terms: list[TermExpression] = []
    for arg in args:
        expr = _lower_argument_expression(arg, params, param_types, return_type)
        if expr.text is None:
            return None
        terms.append(expr)
    return terms


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


def _rust_runtime_method_template(
    method_name: str,
    receiver: str,
    args: list[str],
    params: list[str],
    param_types: list[str],
    return_type: str,
) -> str | None:
    candidates = _rust_runtime_method_concepts(method_name, receiver)
    if not candidates:
        return None
    receiver_expr = _lower_argument_expression(receiver, params, param_types, return_type)
    if receiver_expr.text is None:
        return None
    arg_terms = _lower_argument_terms(args, params, param_types, return_type)
    if arg_terms is None:
        return None
    template_params = [receiver_expr.text, *(term.text or "" for term in arg_terms)]
    template_types = _rust_runtime_method_param_types(
        method_name,
        receiver_expr,
        arg_terms,
        receiver,
        args,
        params,
        param_types,
    )
    return _body_template_for_entries(
        entries(),
        candidates,
        template_params,
        template_types,
        return_type,
    )


def _body_template_expression_for(
    concept_name: str,
    params: list[str],
    param_types: list[str],
    return_type: str,
) -> str | None:
    return _body_template_expression_for_candidates(
        (concept_name, concept_name.removeprefix("concept:")),
        params,
        param_types,
        return_type,
    )


def _body_template_expression_for_candidates(
    candidate_names: tuple[str, ...],
    params: list[str],
    param_types: list[str],
    return_type: str,
) -> str | None:
    body = _body_template_for_entries(
        entries(),
        candidate_names,
        params,
        param_types,
        return_type,
    )
    if body is None:
        return None
    return _single_return_expression(body)


def _rust_runtime_call_concepts(path: str) -> tuple[str, ...]:
    match path:
        case "blake3::Hasher::new":
            return ("rust-call:blake3::Hasher::new",)
        case "hex::encode":
            return ("rust-call:hex::encode",)
        case "String::with_capacity":
            return ("concept:string-with-capacity",)
    if path.endswith("::new"):
        return ("concept:new",)
    return ()


def _rust_runtime_call_return_type(path: str) -> str:
    match path:
        case "hex::encode" | "String::with_capacity":
            return "str"
    if path == "blake3::Hasher::new":
        return "blake3.Hasher"
    return ""


def _lower_operation_expression(
    surface: str,
    params: list[str],
    param_types: list[str],
    return_type: str,
) -> TermExpression | None:
    head_args = _head_and_args(surface)
    if head_args is None:
        return None
    op_name, inner = head_args
    candidates = _rust_runtime_operation_concepts(op_name)
    if not candidates:
        return None
    raw_args = _split_top_level(inner)
    arg_terms = _lower_argument_terms(raw_args, params, param_types, return_type)
    if arg_terms is None:
        return None
    arg_exprs = [term.text or "" for term in arg_terms]
    arg_types = [
        _expression_type(term, arg, params, param_types)
        for term, arg in zip(arg_terms, raw_args, strict=True)
    ]
    template = _body_template_expression_for_candidates(
        candidates,
        arg_exprs,
        arg_types,
        return_type,
    )
    if template is None:
        return None
    return TermExpression(
        text=template,
        type_name=_rust_runtime_operation_return_type(op_name, arg_terms),
    )


def _rust_runtime_operation_concepts(op_name: str) -> tuple[str, ...]:
    match op_name.strip():
        case "array_repeat":
            return ("concept:array-repeat",)
        case "add":
            return ("concept:add",)
        case "sub":
            return ("concept:sub",)
        case "conditional":
            return ("concept:conditional",)
        case "decl":
            return ("concept:decl",)
        case "eq":
            return ("concept:eq",)
        case "ne":
            return ("concept:ne",)
        case "lt":
            return ("concept:lt",)
        case "le":
            return ("concept:le",)
        case "gt":
            return ("concept:gt",)
        case "ge":
            return ("concept:ge",)
        case "mul":
            return ("concept:mul",)
        case "div":
            return ("concept:div",)
        case "and":
            return ("concept:and",)
        case "or":
            return ("concept:or",)
        case "not":
            return ("concept:not",)
        case "mod":
            return ("concept:mod",)
        case "shl":
            return ("concept:shl",)
        case "shr":
            return ("concept:shr",)
        case "bitand":
            return ("concept:bitand",)
        case "bitor":
            return ("concept:bitor",)
        case "bitxor":
            return ("concept:bitxor",)
        case "neg":
            return ("concept:neg",)
        case "bitnot":
            return ("concept:bitnot",)
        case "borrow":
            return ("concept:borrow",)
    return ()


def _rust_runtime_operation_return_type(
    op_name: str, arg_terms: list[TermExpression]
) -> str:
    match op_name.strip():
        case (
            "add"
            | "sub"
            | "mul"
            | "div"
            | "mod"
            | "shl"
            | "shr"
            | "bitand"
            | "bitor"
            | "bitxor"
        ):
            types = [term.type_name for term in arg_terms]
            if types and all(type_name == "int" for type_name in types):
                return "int"
            if types and all(type_name == "str" for type_name in types):
                return "str"
        case "neg" | "bitnot":
            types = [term.type_name for term in arg_terms]
            if types and all(type_name == "int" for type_name in types):
                return "int"
        case "eq" | "ne" | "lt" | "le" | "gt" | "ge":
            types = [term.type_name for term in arg_terms]
            if types and all(type_name == "int" for type_name in types):
                return "bool"
        case "and" | "or":
            types = [term.type_name for term in arg_terms]
            if types and all(type_name == "bool" for type_name in types):
                return "bool"
        case "not":
            types = [term.type_name for term in arg_terms]
            if types and all(type_name == "bool" for type_name in types):
                return "bool"
        case "borrow":
            return arg_terms[0].type_name if arg_terms else ""
    return ""


def _rust_runtime_method_concepts(
    method_name: str, receiver: str
) -> tuple[str, ...]:
    method_key = _concept_key(method_name)
    candidates: list[str] = []
    match method_name:
        case "len":
            candidates.append("concept:method-len")
        case "push_str":
            candidates.append("concept:string-push-str")
    receiver_key = _receiver_chain_key(receiver)
    if receiver_key:
        candidates.append(f"rust-method:{receiver_key}-{method_key}")
    return tuple(candidates)


def _rust_runtime_method_param_types(
    method_name: str,
    receiver_expr: TermExpression,
    arg_terms: list[TermExpression],
    receiver: str,
    args: list[str],
    params: list[str],
    param_types: list[str],
) -> list[str]:
    if method_name == "push_str":
        return ["str", "str"]
    return [
        _expression_type(receiver_expr, receiver, params, param_types),
        *(
            _expression_type(term, arg, params, param_types)
            for term, arg in zip(arg_terms, args, strict=True)
        ),
    ]


def _rust_runtime_method_return_type(method_name: str) -> str:
    match method_name:
        case "len":
            return "int"
        case "push_str":
            return "str"
        case "update" | "finalize_xof":
            return "blake3.Hasher"
        case "fill":
            return "bytes"
    return ""


def _requires_rust_method_template(method_name: str) -> bool:
    return method_name in {"len", "push_str", "update", "finalize_xof", "fill"}


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


def _simple_receiver_name(receiver: str) -> bool:
    return re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", receiver.strip()) is not None


def _concept_key(value: str) -> str:
    return value.strip().replace("_", "-").lower()


def _receiver_chain_key(receiver: str) -> str | None:
    stripped = receiver.strip()
    if _simple_receiver_name(stripped):
        return _concept_key(_strip_ssa_suffix(stripped))
    parsed = _parse_method_surface(stripped) if stripped.startswith("method:") else None
    if parsed is None:
        return None
    method_name, inner_receiver, _args = parsed
    inner_key = _receiver_chain_key(inner_receiver)
    if inner_key is None:
        return None
    return f"{inner_key}-{_concept_key(method_name)}"


def _strip_ssa_suffix(value: str) -> str:
    return re.sub(r"_v\d+$", "", value.strip())


def _expression_type(
    expr: TermExpression,
    raw_arg: str,
    params: list[str],
    param_types: list[str],
) -> str:
    if expr.type_name:
        return expr.type_name
    return _surface_type(raw_arg, params, param_types)


def _surface_type(surface: str, params: list[str], param_types: list[str]) -> str:
    from_param = _type_for_argument(surface, params, param_types)
    if from_param:
        return map_source_type(from_param)
    stripped = surface.strip()
    if re.fullmatch(r"-?\d+", stripped):
        return "int"
    if stripped in {"true", "false", "True", "False"}:
        return "bool"
    if (
        len(stripped) >= 2
        and stripped[0] == stripped[-1]
        and stripped[0] in {"'", '"'}
    ):
        return "str"
    if stripped == "BLAKE3_512_PREFIX":
        return "str"
    return ""


def _type_for_argument(arg: str, params: list[str], param_types: list[str]) -> str:
    stripped = arg.strip()
    for index, param in enumerate(params):
        if stripped == param and index < len(param_types):
            return param_types[index]
    return ""


def _arg_shape(arg: str, params: list[str], param_types: list[str]) -> str:
    stripped = arg.strip()
    parsed_call = _parse_call_surface(stripped) if stripped.startswith("call:") else None
    if parsed_call is not None:
        path, _args = parsed_call
        return f"call:{path}"
    parsed_method = _parse_method_surface(stripped) if stripped.startswith("method:") else None
    if parsed_method is not None:
        method_name, _receiver, _args = parsed_method
        return f"method:{method_name}"
    head_args = _head_and_args(stripped)
    if head_args is not None:
        head, _inner = head_args
        return head
    type_name = _surface_type(stripped, params, param_types)
    return type_name or "expr"


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
    return _entries_from_files(
        (BODY_TEMPLATE_REL, RUST_RUNTIME_BODY_TEMPLATE_REL, BLAKE3_BODY_TEMPLATE_REL)
    )


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
        loss_record_contribution = item.get("loss_record_contribution")
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
                loss_record_contribution=loss_record_contribution
                if isinstance(loss_record_contribution, dict)
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
    # Per the canonical-form ruling, function names are not bind-CID-relevant
    # (audit/provenance only). When the lift kit stripped fn_name from the
    # bind payload envelope (A19), this field arrives empty. Fall back to a
    # synthetic placeholder so the emitted source parses; relift recovers
    # the algebra from term_shape composition, not from the function name.
    function = function or "_provekit_synth"
    param_list = ", ".join(params)
    body_lines = body.splitlines() or ["pass"]
    if all(not line.strip() or line.lstrip().startswith("#") for line in body_lines):
        body_lines.append("pass")
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
