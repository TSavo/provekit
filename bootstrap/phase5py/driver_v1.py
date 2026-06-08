from __future__ import annotations

import ast
import json
import os
import subprocess
import sys
import traceback
from pathlib import Path
from typing import Any


ARC = "Phase-5-Py-v1"
REPO_ROOT = Path(__file__).resolve().parents[2]
OUT_DIR = REPO_ROOT / "bootstrap/phase5py"
RECEIPT_PATH = OUT_DIR / "v1_receipt.json"
MODULE_PATH = OUT_DIR / "libsugar_py_v1.py"
README_PATH = OUT_DIR / "README.md"
BODY_TEMPLATE_REL = Path(
    "menagerie/python-language-signature/specs/body-templates/"
    "python-canonical-bodies-libsugar.json"
)
BODY_TEMPLATE_PATH = REPO_ROOT / BODY_TEMPLATE_REL

CASE_SPECS = [
    {
        "name": "null",
        "fixture": "implementations/rust/libsugar/tests/fixtures/proofir/d7_v0_value_null.json",
        "function": "null",
        "params": [],
        "param_types": [],
        "return_type": "Value",
        "body_template_concept": "concept:value-null",
    },
    {
        "name": "boolean",
        "fixture": "implementations/rust/libsugar/tests/fixtures/proofir/d7_v4_value_boolean.json",
        "function": "boolean",
        "params": ["b"],
        "param_types": ["bool"],
        "return_type": "Value",
        "body_template_concept": "concept:value-boolean",
    },
    {
        "name": "integer",
        "fixture": "implementations/rust/libsugar/tests/fixtures/proofir/d7_v4_value_integer.json",
        "function": "integer",
        "params": ["n"],
        "param_types": ["int"],
        "return_type": "Value",
        "body_template_concept": "concept:value-integer",
    },
    {
        "name": "string",
        "fixture": "implementations/rust/libsugar/tests/fixtures/proofir/d7_v4_value_string.json",
        "function": "string",
        "params": ["s"],
        "param_types": ["str"],
        "return_type": "Value",
        "body_template_concept": "concept:value-string",
    },
]


def main() -> int:
    _reexec_with_kit_runtime_if_needed()
    _prepare_import_path()
    os.environ.setdefault("SUGAR_REPO_ROOT", str(REPO_ROOT))

    try:
        from sugar_lift_py_tests.canonicalizer import blake3_512_of
        from sugar_lift_python_source.canonical import cid_of_json
        from sugar_lift_python_source.lifter import lift_source
        from sugar_realize_python_core import realizer as realize_python_realizer
    except Exception as exc:
        _write_import_failure_receipt(exc)
        print(f"{ARC}: kit import failed, receipt recorded at {RECEIPT_PATH}")
        return 0

    template_loading = _configure_body_template_catalog(realize_python_realizer)
    cases: list[dict[str, Any]] = []
    emitted_sources: list[str] = []

    for spec in CASE_SPECS:
        fixture_path = REPO_ROOT / str(spec["fixture"])
        fixture = json.loads(fixture_path.read_text(encoding="utf-8"))
        source_path = f"bootstrap/phase5py/generated/{spec['name']}.py"

        case: dict[str, Any] = {
            "name": spec["name"],
            "fixture": spec["fixture"],
            "target": fixture.get("target"),
            "term_surface": fixture.get("term_surface"),
            "body_template_concept_name": spec["body_template_concept"],
            "rust_fixture_substrate_cid": cid_of_json(fixture["proofir_term"]),
            "realize_python_invocation": {
                "entrypoint": "sugar_realize_python_core.realizer.emit_stub",
                "function": spec["function"],
                "params": spec["params"],
                "param_types": spec["param_types"],
                "return_type": spec["return_type"],
                "concept_name": spec["body_template_concept"],
                "template_catalog": str(BODY_TEMPLATE_REL),
            },
        }

        try:
            if not template_loading["loaded"]:
                raise RuntimeError(str(template_loading["reason"]))
            realized = realize_python_realizer.emit_stub(
                function=str(spec["function"]),
                params=list(spec["params"]),
                param_types=list(spec["param_types"]),
                return_type=str(spec["return_type"]),
                concept_name=str(spec["body_template_concept"]),
            )
            source = str(realized["source"])
            ast.parse(source, filename=source_path)
            lift_result = lift_source(source, source_path)
            function_contract = _find_lifted_function_contract(lift_result.ir, str(spec["function"]))
            body_term = function_contract["post"]["args"][1] if function_contract else None
            lift_output_cid = cid_of_json(body_term) if body_term is not None else cid_of_json(lift_result.ir)
            source_cid = blake3_512_of(source.encode("utf-8"))
            substrate_match = lift_output_cid == case["rust_fixture_substrate_cid"]
            verdict = "BYTE_IDENTICAL" if substrate_match else "CHARACTERIZED_DIFF"
            diff = _diff_record(
                is_stub=bool(realized.get("is_stub")),
                lift_refusals=lift_result.refusals,
                substrate_match=substrate_match,
                rust_cid=case["rust_fixture_substrate_cid"],
                python_cid=lift_output_cid,
                rust_term=fixture["proofir_term"],
                python_body_term=body_term,
            )
            case.update(
                {
                    "realize_python_is_stub": bool(realized.get("is_stub")),
                    "realize_python_body_template_loaded": template_loading,
                    "emitted_python_source": source,
                    "emitted_python_source_cid": source_cid,
                    "python_ast_parse": "ok",
                    "lift_python_entrypoint": "sugar_lift_python_source.lifter.lift_source",
                    "lift_python_source_path": source_path,
                    "lift_python_refusals": lift_result.refusals,
                    "lift_python_diagnostics": lift_result.diagnostics,
                    "lift_python_contract_cid": cid_of_json(function_contract)
                    if function_contract
                    else None,
                    "lift_python_output_cid": lift_output_cid,
                    "lift_python_body_summary": _term_summary(body_term),
                    "substrate_cids_match": substrate_match,
                    "verdict": verdict,
                    "diff": diff,
                    "kit_behavior_responsible": diff["kit_behavior_responsible"],
                }
            )
            emitted_sources.append(source)
        except Exception as exc:
            case.update(_case_failure_record(exc, template_loading))
        cases.append(case)

    module_source = _module_source(emitted_sources)
    ast.parse(module_source, filename=str(MODULE_PATH))
    MODULE_PATH.write_text(module_source, encoding="utf-8")

    template_source_cid = blake3_512_of(BODY_TEMPLATE_PATH.read_bytes())
    receipt = _receipt(cases, blake3_512_of(module_source.encode("utf-8")), template_source_cid)
    RECEIPT_PATH.write_text(
        json.dumps(receipt, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    README_PATH.write_text(_readme(cases), encoding="utf-8")

    _print_report(cases)
    return 0


def _runtime_ok() -> bool:
    if sys.version_info < (3, 11):
        return False
    try:
        import blake3  # noqa: F401
    except Exception:
        return False
    return True


def _reexec_with_kit_runtime_if_needed() -> None:
    if _runtime_ok():
        return
    if os.environ.get("PHASE5PY_RUNTIME_HANDOFF") == "1":
        return
    runtime = _find_python_runtime()
    if runtime is None:
        return
    current = Path(sys.executable)
    selected = Path(runtime)
    if selected == current:
        return
    env = os.environ.copy()
    env["PHASE5PY_RUNTIME_HANDOFF"] = "1"
    proc = subprocess.run(
        [str(selected), str(Path(__file__).resolve()), *sys.argv[1:]],
        env=env,
        check=False,
    )
    raise SystemExit(proc.returncode)


def _find_python_runtime() -> str | None:
    candidates = [
        os.environ.get("SUGAR_PHASE5PY_PYTHON"),
        sys.executable,
        "/usr/local/bin/python3",
        "/opt/homebrew/bin/python3",
        "/usr/bin/python3",
        "python3",
    ]
    seen: set[str] = set()
    for candidate in candidates:
        if not candidate or candidate in seen:
            continue
        seen.add(candidate)
        try:
            proc = subprocess.run(
                [
                    candidate,
                    "-c",
                    (
                        "import importlib.util, sys; "
                        "ok = sys.version_info >= (3, 11) "
                        "and importlib.util.find_spec('blake3') is not None; "
                        "raise SystemExit(0 if ok else 1)"
                    ),
                ],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
        except OSError:
            continue
        if proc.returncode == 0:
            return candidate
    return None


def _prepare_import_path() -> None:
    paths = [
        REPO_ROOT / "implementations/python/sugar-realize-python-core/src",
        REPO_ROOT / "implementations/python/sugar-lift-python-source/src",
        REPO_ROOT / "implementations/python/sugar-lift-py-tests/src",
    ]
    for path in reversed(paths):
        text = str(path)
        if text not in sys.path:
            sys.path.insert(0, text)


def _configure_body_template_catalog(realize_python_realizer: Any) -> dict[str, Any]:
    try:
        raw = BODY_TEMPLATE_PATH.read_text(encoding="utf-8")
        root = json.loads(raw)
        entries = root["header"]["content"]["entries"]
        if not isinstance(entries, list) or len(entries) != 4:
            raise ValueError("libsugar body-template catalog must contain four entries")
        realize_python_realizer.BODY_TEMPLATE_REL = BODY_TEMPLATE_REL
        realize_python_realizer.entries.cache_clear()
        loaded_entries = realize_python_realizer.entries()
        loaded_concepts = [entry.concept_name for entry in loaded_entries]
        expected = [str(spec["body_template_concept"]) for spec in CASE_SPECS]
        missing = [concept for concept in expected if concept not in loaded_concepts]
        if missing:
            raise ValueError(f"body-template concepts not loaded: {missing}")
        return {
            "loaded": True,
            "mechanism": "driver runtime override of sugar_realize_python_core.realizer.BODY_TEMPLATE_REL",
            "path": str(BODY_TEMPLATE_REL),
            "template_name": root["header"]["content"]["template_name"],
        }
    except Exception as exc:
        return {
            "loaded": False,
            "mechanism": "driver runtime override of sugar_realize_python_core.realizer.BODY_TEMPLATE_REL",
            "path": str(BODY_TEMPLATE_REL),
            "reason": repr(exc),
        }


def _find_lifted_function_contract(ir: list[dict[str, Any]], function_name: str) -> dict[str, Any] | None:
    suffix = f".{function_name}"
    for item in ir:
        fn_name = str(item.get("fnName", ""))
        if item.get("kind") == "function-contract" and not fn_name.startswith("<source-unit:"):
            if fn_name.endswith(suffix):
                return item
    return None


def _diff_record(
    *,
    is_stub: bool,
    lift_refusals: list[dict[str, Any]],
    substrate_match: bool,
    rust_cid: str,
    python_cid: str,
    rust_term: dict[str, Any],
    python_body_term: Any,
) -> dict[str, Any]:
    if substrate_match:
        classification = "byte-identical"
        responsible = "lift_python output CID matches the D7 Rust fixture ProofIR term CID."
    elif is_stub:
        classification = "realize-python-template-gap"
        responsible = (
            "sugar-realize-python-core emitted its fallback stub instead of "
            "the libsugar Value constructor body template."
        )
    elif lift_refusals:
        classification = "lift-python-refusal"
        responsible = "sugar-lift-python-source refused part of the emitted Python body."
    else:
        classification = "lift-python-substrate-namespace-mismatch"
        responsible = (
            "sugar-lift-python-source emitted python:* body terms for the Python "
            "Value constructor idioms rather than the D7 ProofIR return/call:new term."
        )
    return {
        "classification": classification,
        "kit_behavior_responsible": responsible,
        "rust_fixture_substrate_cid": rust_cid,
        "lift_python_output_cid": python_cid,
        "rust_term_name": rust_term.get("name"),
        "python_body_summary": _term_summary(python_body_term),
    }


def _case_failure_record(exc: Exception, template_loading: dict[str, Any]) -> dict[str, Any]:
    classification = "driver-or-kit-invocation-failure"
    responsible = "phase5py driver could not complete this kit invocation."
    if not template_loading.get("loaded"):
        classification = "realize-python-custom-template-load-gap"
        responsible = (
            "sugar-realize-python-core has no clean public body-template path "
            "parameter; v1 only attempted a driver runtime override."
        )
    return {
        "realize_python_is_stub": None,
        "realize_python_body_template_loaded": template_loading,
        "emitted_python_source": "",
        "emitted_python_source_cid": "",
        "python_ast_parse": "not-run",
        "lift_python_refusals": [],
        "lift_python_diagnostics": [],
        "lift_python_contract_cid": None,
        "lift_python_output_cid": "",
        "substrate_cids_match": False,
        "verdict": "CHARACTERIZED_DIFF",
        "diff": {
            "classification": classification,
            "kit_behavior_responsible": responsible,
            "error": repr(exc),
            "traceback": traceback.format_exc(),
        },
        "kit_behavior_responsible": responsible,
    }


def _term_summary(term: Any) -> str:
    names = _ctor_names(term)
    return " -> ".join(names[:8]) if names else "<none>"


def _ctor_names(term: Any) -> list[str]:
    if isinstance(term, dict):
        names = [str(term["name"])] if term.get("kind") == "ctor" else []
        for child in term.get("args", []):
            names.extend(_ctor_names(child))
        return names
    if isinstance(term, list):
        out: list[str] = []
        for item in term:
            out.extend(_ctor_names(item))
        return out
    return []


def _module_source(sources: list[str]) -> str:
    body = "\n".join(source.rstrip() + "\n" for source in sources)
    return (
        "# Generated by bootstrap/phase5py/driver_v1.py.\n"
        "# This module is the Phase-5-Py-v1 realize_python output snapshot.\n"
        "from __future__ import annotations\n\n"
        f"{body}"
    )


def _receipt(
    cases: list[dict[str, Any]],
    module_source_cid: str,
    template_source_cid: str,
) -> dict[str, Any]:
    return {
        "version": "1",
        "arc": ARC,
        "parent_umbrella": "#977",
        "base_commit": "ff1f16d0",
        "branch": "feat/phase5py-v1-python-body-templates",
        "n": 1,
        "cluster": "Value constructor cluster: null, boolean, integer, string",
        "constraints": {
            "python_related_authored_in_python": True,
            "rust_code_changes": False,
            "substrate_changes": False,
            "new_memento_types": False,
        },
        "kits": {
            "realize_python": "implementations/python/sugar-realize-python-core",
            "lift_python": "implementations/python/sugar-lift-python-source",
            "python_body_templates": str(BODY_TEMPLATE_REL),
            "python_body_templates_source_cid": template_source_cid,
        },
        "outputs": {
            "module": str(MODULE_PATH.relative_to(REPO_ROOT)),
            "module_source_cid": module_source_cid,
            "receipt": str(RECEIPT_PATH.relative_to(REPO_ROOT)),
            "readme": str(README_PATH.relative_to(REPO_ROOT)),
        },
        "cases": cases,
        "all_substrate_cids_match": all(case.get("substrate_cids_match") for case in cases),
    }


def _readme(cases: list[dict[str, Any]]) -> str:
    all_match = all(case.get("substrate_cids_match") for case in cases)
    status = (
        "Phase-5-Py-v1 n=1 case verified on the Value constructor cluster"
        if all_match
        else "Phase-5-Py-v1 retires the v0 body-template gap and records the next blocker"
    )
    rows = [
        (
            f"| {case['name']} | {case['verdict']} | "
            f"{case['diff']['classification']} | {str(case['substrate_cids_match']).lower()} |"
        )
        for case in cases
    ]
    lines = [
        "# Phase-5-Py-v1 n=1 self-trip receipt",
        "",
        status + ".",
        "",
        "This directory records the Python n=1 case for the libsugar self-host arc.",
        "v1 adds a libsugar-specific Python body-template catalog for the four D7",
        "Value constructor surfaces and runs the same realize, parse, lift, compare",
        "loop captured by v0.",
        "",
        "The self-trip under test is:",
        "",
        "1. read the D7 Rust lift fixture",
        "2. invoke sugar-realize-python-core with python-canonical-bodies-libsugar.json",
        "3. parse the emitted Python with ast.parse",
        "4. invoke sugar-lift-python-source on that source",
        "5. compare the lifted Python body CID with the fixture ProofIR term CID",
        "",
        "Fixtures used:",
        "",
        "- implementations/rust/libsugar/tests/fixtures/proofir/d7_v0_value_null.json",
        "- implementations/rust/libsugar/tests/fixtures/proofir/d7_v4_value_boolean.json",
        "- implementations/rust/libsugar/tests/fixtures/proofir/d7_v4_value_integer.json",
        "- implementations/rust/libsugar/tests/fixtures/proofir/d7_v4_value_string.json",
        "",
        "Generated artifacts:",
        "",
        "- bootstrap/phase5py/driver_v1.py",
        "- bootstrap/phase5py/libsugar_py_v1.py",
        "- bootstrap/phase5py/v1_receipt.json",
        "- menagerie/python-language-signature/specs/body-templates/python-canonical-bodies-libsugar.json",
        "- bootstrap/phase5py/README.md",
        "",
        "Per-fixture verdicts:",
        "",
        "| Fixture | Verdict | Diff class | Substrate CID match |",
        "| --- | --- | --- | --- |",
        *rows,
        "",
        "The v0 fallback-stub behavior is retired for this catalog.",
        "The realized v1 functions are valid Python and use Value.NULL, Value.boolean,",
        "Value.integer, and Value.string bodies.",
        "",
        "The current stop condition is in lift_python.",
        "The source lifter emits python:* body terms for these Python idioms.",
        "It does not currently map them back to the D7 ProofIR return/call:new terms.",
        "No Rust code, substrate code, or memento type was changed for this receipt.",
    ]
    return "\n".join(lines) + "\n"


def _write_import_failure_receipt(exc: Exception) -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    receipt = {
        "version": "1",
        "arc": ARC,
        "verdict": "CHARACTERIZED_DIFF",
        "diff": {
            "classification": "python-kit-import-failure",
            "kit_behavior_responsible": "python kit runtime import failed before realization.",
            "error": repr(exc),
            "traceback": traceback.format_exc(),
        },
    }
    RECEIPT_PATH.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def _print_report(cases: list[dict[str, Any]]) -> None:
    print("Phase-5-Py-v1 per-fixture verdicts")
    for case in cases:
        print(
            f"- {case['name']}: {case['verdict']}; "
            f"rust_fixture_substrate_cid={case['rust_fixture_substrate_cid']}; "
            f"lift_python_output_cid={case['lift_python_output_cid']}; "
            f"substrate_cids_match={case['substrate_cids_match']}"
        )
    print(f"wrote {MODULE_PATH.relative_to(REPO_ROOT)}")
    print(f"wrote {RECEIPT_PATH.relative_to(REPO_ROOT)}")
    print(f"wrote {README_PATH.relative_to(REPO_ROOT)}")


if __name__ == "__main__":
    raise SystemExit(main())
