from __future__ import annotations

import json
import keyword
import os
import re
import subprocess
from dataclasses import dataclass
from functools import lru_cache
from pathlib import Path
from typing import Any

try:
    import blake3
except ModuleNotFoundError:  # pragma: no cover - depends on host Python env
    blake3 = None


def _blake3_digest(data: bytes, length: int = 64) -> bytes:
    if blake3 is not None:
        return blake3.blake3(data).digest(length=length)
    completed = subprocess.run(
        ["b3sum", "--length", str(length), "--no-names"],
        input=data,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=True,
    )
    return bytes.fromhex(completed.stdout.decode("ascii").strip().split()[0])


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
DEFAULT_KIT_CID = "blake3-512:" + _blake3_digest(
    b"provekit-realize-python-core@0.1.0"
).hex()
DEFAULT_POLICY_CID = "blake3-512:" + _blake3_digest(
    b"provekit-realize-python-core/default-contract-comment-policy"
).hex()
DEFAULT_SUGAR_DICT_CID = "blake3-512:" + _blake3_digest(
    b"provekit-realize-python-core/contract-comment-sugar-v1"
).hex()


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
    type_name: str = ""


@dataclass(frozen=True)
class PythonFunctionName:
    kind: str
    function: str
    class_name: str | None = None
    method_name: str | None = None


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
    parsed_name = _parse_python_function_name(function)
    if parsed_name is None or parsed_name.kind != "free":
        return _unrepresentable_function_result(function)
    return _emit_function(
        function=function,
        params=params,
        param_types=param_types,
        return_type=return_type,
        concept_name=concept_name,
        contract=contract,
        sugar_cids=sugar_cids,
        sugar_plugins=sugar_plugins,
    )


def emit_module(document: dict[str, Any]) -> dict[str, Any]:
    terms = document.get("terms")
    if not isinstance(terms, list):
        terms = []
    canonical_runtime = any(
        isinstance(item, dict)
        and str(item.get("function", "")) in {"encode_jcs", "encode_value", "encode_string"}
        for item in terms
    )

    class_methods: dict[str, list[str]] = {}
    free_functions: list[str] = []
    receipts: list[dict[str, str]] = []
    is_stub = False

    for item in terms:
        if not isinstance(item, dict):
            continue
        function = str(item.get("function", ""))
        parsed_name = _parse_python_function_name(function)
        if parsed_name is None:
            receipts.append(_unrepresentable_function_receipt(function))
            is_stub = True
            continue

        params = _string_list(item.get("params"))
        param_types = _string_list(item.get("paramTypes", item.get("param_types")))
        return_type = str(item.get("returnType", item.get("return_type", "")))
        concept_name = _entry_term_surface(item) or str(
            item.get("conceptName", item.get("concept_name", ""))
        )

        if parsed_name.kind == "free":
            if canonical_runtime:
                canonical_source = _canonical_free_function_source(parsed_name.function)
                if canonical_source is not None:
                    free_functions.append(canonical_source.rstrip())
                    continue
            emitted = _emit_function(
                function=parsed_name.function,
                params=params,
                param_types=param_types,
                return_type=return_type,
                concept_name=concept_name,
            )
            free_functions.append(emitted["source"].rstrip())
            is_stub = is_stub or bool(emitted.get("is_stub"))
            continue

        assert parsed_name.class_name is not None
        assert parsed_name.method_name is not None
        if canonical_runtime and parsed_name.class_name == "Value":
            _ensure_value_runtime_init(class_methods)
            canonical_source = _canonical_value_method_source(parsed_name.method_name)
            if canonical_source is not None:
                class_methods.setdefault("Value", []).append(
                    _indent_block(canonical_source.rstrip(), "    ")
                )
                continue

        method_params, method_types, method_concept, is_static = _method_signature(
            params, param_types, concept_name
        )
        emitted = _emit_function(
            function=parsed_name.method_name,
            params=method_params,
            param_types=method_types,
            return_type=return_type,
            concept_name=method_concept,
        )
        method_source = emitted["source"].rstrip()
        if is_static:
            method_source = "@staticmethod\n" + method_source
        class_methods.setdefault(parsed_name.class_name, []).append(
            _indent_block(method_source, "    ")
        )
        is_stub = is_stub or bool(emitted.get("is_stub"))

    if canonical_runtime:
        helper_source = _canonical_runtime_helper_source("_value_from_python")
        if helper_source is not None:
            free_functions.append(helper_source.rstrip())

    parts: list[str] = []
    for class_name, methods in class_methods.items():
        if methods:
            parts.append(f"class {class_name}:\n" + "\n\n".join(methods))
        else:
            parts.append(f"class {class_name}:\n    pass")
    parts.extend(free_functions)

    source = "\n\n".join(part for part in parts if part)
    if source:
        source += "\n"
    if receipts and not source:
        source = "".join(f"# provekit-realize-python: {r['message']}\n" for r in receipts)
    source = _module_prelude(source) + source
    return {
        "source": source,
        "is_stub": is_stub,
        "extension": "py",
        "receipts": receipts,
    }


def _ensure_value_runtime_init(class_methods: dict[str, list[str]]) -> None:
    methods = class_methods.setdefault("Value", [])
    if methods:
        return
    methods.append(
        _indent_block(
            (
                "def __init__(self, kind, value=None):\n"
                "    self._kind = kind\n"
                "    self.value = value"
            ),
            "    ",
        )
    )


def _canonical_value_method_source(method_name: str) -> str | None:
    match method_name:
        case "kind":
            return "def kind(self):\n    return self._kind\n"
        case "null":
            return '@staticmethod\ndef null():\n    return Value("Null")\n'
        case "boolean":
            return '@staticmethod\ndef boolean(b):\n    return Value("Bool", bool(b))\n'
        case "integer":
            return '@staticmethod\ndef integer(n):\n    return Value("Integer", int(n))\n'
        case "string":
            return '@staticmethod\ndef string(s):\n    return Value("String", str(s))\n'
        case "array":
            return '@staticmethod\ndef array(items):\n    return Value("Array", list(items))\n'
        case "object":
            return (
                "@staticmethod\n"
                "def object(entries):\n"
                "    return Value(\"Object\", [(str(k), v) for k, v in entries])\n"
            )
    return None


def _canonical_free_function_source(function: str) -> str | None:
    match function:
        case "blake3_512_hex":
            return (
                "def blake3_512_hex(s):\n"
                "    data = bytes(s) if isinstance(s, bytearray) else s\n"
                "    if isinstance(data, str):\n"
                "        data = data.encode()\n"
                "    return blake3_512_of(data)\n"
            )
        case "encode_jcs":
            return "def encode_jcs(v):\n    return encode_value(v)\n"
        case "encode_value":
            return (
                "def encode_value(v, out=None):\n"
                "    if not isinstance(v, Value):\n"
                "        v = _value_from_python(v)\n"
                "    if v.kind() == \"Null\":\n"
                "        rendered = \"null\"\n"
                "    elif v.kind() == \"Bool\":\n"
                "        rendered = \"true\" if v.value else \"false\"\n"
                "    elif v.kind() == \"Integer\":\n"
                "        rendered = str(int(v.value))\n"
                "    elif v.kind() == \"String\":\n"
                "        rendered = encode_string(v.value)\n"
                "    elif v.kind() == \"Array\":\n"
                "        rendered = \"[\" + \",\".join(encode_value(item) for item in v.value) + \"]\"\n"
                "    elif v.kind() == \"Object\":\n"
                "        items = sorted(v.value, key=lambda item: item[0])\n"
                "        rendered = \"{\" + \",\".join(encode_string(k) + \":\" + encode_value(val) for k, val in items) + \"}\"\n"
                "    else:\n"
                "        raise TypeError(f\"unknown Value kind: {v.kind()}\")\n"
                "    return rendered if out is None else out + rendered\n"
            )
        case "encode_string":
            return (
                "def encode_string(s, out=None):\n"
                "    pieces = [\"\\\"\"]\n"
                "    for c in str(s):\n"
                "        code = ord(c)\n"
                "        if c == \"\\\"\":\n"
                "            pieces.append(\"\\\\\\\"\")\n"
                "        elif c == \"\\\\\":\n"
                "            pieces.append(\"\\\\\\\\\")\n"
                "        elif code < 0x20:\n"
                "            pieces.append(f\"\\\\u{code:04x}\")\n"
                "        else:\n"
                "            pieces.append(c)\n"
                "    pieces.append(\"\\\"\")\n"
                "    rendered = \"\".join(pieces)\n"
                "    return rendered if out is None else out + rendered\n"
            )
    return _canonical_runtime_helper_source(function)


def _canonical_runtime_helper_source(function: str) -> str | None:
    if function != "_value_from_python":
        return None
    return (
        "def _value_from_python(value):\n"
        "    if isinstance(value, Value):\n"
        "        return value\n"
        "    if value is None:\n"
        "        return Value.null()\n"
        "    if isinstance(value, bool):\n"
        "        return Value.boolean(value)\n"
        "    if isinstance(value, int):\n"
        "        return Value.integer(value)\n"
        "    if isinstance(value, str):\n"
        "        return Value.string(value)\n"
        "    if isinstance(value, list):\n"
        "        return Value.array([_value_from_python(item) for item in value])\n"
        "    if isinstance(value, dict):\n"
        "        return Value.object((k, _value_from_python(v)) for k, v in value.items())\n"
        "    raise TypeError(f\"cannot convert to Value: {type(value).__name__}\")\n"
    )


def _emit_function(
    *,
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
            body = (
                "raise NotImplementedError("
                f"{_double_quoted(f'provekit-bind canonical: {concept_name}')})"
            )
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


def _parse_python_function_name(function: str) -> PythonFunctionName | None:
    if _is_python_identifier(function):
        return PythonFunctionName(kind="free", function=function)
    if function.count("::") == 1:
        class_name, method_name = function.split("::", 1)
        if _is_python_identifier(class_name) and _is_python_identifier(method_name):
            return PythonFunctionName(
                kind="method",
                function=function,
                class_name=class_name,
                method_name=method_name,
            )
    return None


def _is_python_identifier(value: str) -> bool:
    return (
        re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", value) is not None
        and not keyword.iskeyword(value)
    )


def _unrepresentable_function_result(function: str) -> dict[str, Any]:
    receipt = _unrepresentable_function_receipt(function)
    return {
        "source": f"# provekit-realize-python: {receipt['message']}\n",
        "is_stub": True,
        "extension": "py",
        "receipts": [receipt],
    }


def _unrepresentable_function_receipt(function: str) -> dict[str, str]:
    return {
        "status": "refused",
        "message": f"function name unrepresentable in python: {function}",
    }


def _string_list(value: Any) -> list[str]:
    if not isinstance(value, list):
        return []
    return [str(item) for item in value]


def _entry_term_surface(item: dict[str, Any]) -> str | None:
    term_shape = item.get("termShape", item.get("term_shape"))
    if isinstance(term_shape, str) and term_shape.strip():
        return term_shape
    if isinstance(term_shape, dict):
        for key in ("term_surface", "termSurface"):
            value = term_shape.get(key)
            if isinstance(value, str) and value.strip():
                return value
    return None


def _method_signature(
    params: list[str],
    param_types: list[str],
    concept_name: str,
) -> tuple[list[str], list[str], str, bool]:
    if not params:
        return [], [], concept_name, True
    first_name = params[0]
    first_type = param_types[0] if param_types else ""
    is_receiver = first_name in {"self", "__self"} or first_type.replace(" ", "") in {
        "Self",
        "&Self",
        "&mutSelf",
    }
    if not is_receiver:
        return params, param_types, concept_name, True
    method_params = ["self", *params[1:]]
    method_types = [first_type, *param_types[1:]] if param_types else []
    method_concept = _replace_identifier(concept_name, first_name, "self")
    return method_params, method_types, method_concept, False


def _replace_identifier(text: str, old: str, new: str) -> str:
    if old == new or not old:
        return text
    return re.sub(rf"\b{re.escape(old)}\b", new, text)


def _indent_block(source: str, prefix: str) -> str:
    return "\n".join(f"{prefix}{line}" if line else "" for line in source.splitlines())


def _module_prelude(source: str) -> str:
    lines: list[str] = []
    if "blake3." in source:
        lines.extend(
            [
                "try:",
                "    import blake3",
                "except ModuleNotFoundError:",
                "    import subprocess",
                "",
                "    class _ProvekitBlake3Hasher:",
                "        def __init__(self):",
                "            self._chunks = []",
                "",
                "        def update(self, data):",
                "            self._chunks.append(bytes(data))",
                "",
                "        def digest(self, length=64):",
                "            completed = subprocess.run(",
                "                [\"b3sum\", \"--length\", str(length), \"--no-names\"],",
                "                input=b\"\".join(self._chunks),",
                "                stdout=subprocess.PIPE,",
                "                stderr=subprocess.PIPE,",
                "                check=True,",
                "            )",
                "            return bytes.fromhex(completed.stdout.decode(\"ascii\").strip().split()[0])",
                "",
                "    class _ProvekitBlake3Module:",
                "        @staticmethod",
                "        def blake3():",
                "            return _ProvekitBlake3Hasher()",
                "",
                "    blake3 = _ProvekitBlake3Module()",
            ]
        )
    if "BLAKE3_512_PREFIX" in source:
        lines.append('BLAKE3_512_PREFIX = "blake3-512:"')
    if not lines:
        return ""
    return "\n".join(lines) + "\n\n"


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
    return "blake3-512:" + _blake3_digest(_canonical_json_bytes(value)).hex()


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
    operation = _lower_operation_expression(surface, params, param_types, return_type)
    if operation is not None:
        return operation
    if _safe_python_atom(surface):
        return TermExpression(text=surface, type_name=_surface_type(surface, params, param_types))
    return TermExpression(stub_body=_unsupported_term_stub(surface))


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
        if expr.stub_body is not None or expr.text is None:
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
    if receiver_expr.stub_body is not None or receiver_expr.text is None:
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
        case "borrow":
            return ("concept:borrow",)
    return ()


def _rust_runtime_operation_return_type(
    op_name: str, arg_terms: list[TermExpression]
) -> str:
    match op_name.strip():
        case "add":
            types = [term.type_name for term in arg_terms]
            if types and all(type_name == "int" for type_name in types):
                return "int"
            if types and all(type_name == "str" for type_name in types):
                return "str"
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


def _unsupported_term_stub(surface: str) -> str:
    message = _double_quoted(f"provekit-bind canonical: {surface}")
    return (
        f"# provekit-realize-python: unsupported canonical term `{surface}`\n"
        f"raise NotImplementedError({message})"
    )


def _safe_python_atom(surface: str) -> bool:
    stripped = surface.strip()
    if _simple_receiver_name(stripped):
        return True
    if stripped in {"True", "False", "None"}:
        return True
    if re.fullmatch(r"-?\d+", stripped):
        return True
    if (
        len(stripped) >= 2
        and stripped[0] == stripped[-1]
        and stripped[0] in {"'", '"'}
    ):
        return True
    return False


def _double_quoted(value: str) -> str:
    escaped = value.replace("\\", "\\\\").replace('"', '\\"')
    return f'"{escaped}"'


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
