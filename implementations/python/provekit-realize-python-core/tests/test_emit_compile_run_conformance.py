from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[4]
LIFT_SRC = ROOT / "implementations/python/provekit-lift-python-source/src"
PY_TESTS_SRC = ROOT / "implementations/python/provekit-lift-py-tests/src"
REALIZER_SRC = ROOT / "implementations/python/provekit-realize-python-core/src"
for path in (PY_TESTS_SRC, LIFT_SRC, REALIZER_SRC):
    if str(path) not in sys.path:
        sys.path.insert(0, str(path))

from provekit_lift_python_source.bind_lifter import lift_source
from provekit_realize_python_core.realizer import emit_stub


FIXTURE_ROOT = ROOT / "implementations/python/conformance/fixtures"
REQUIRED_FIXTURE_TYPES = {
    "hello_world",
    "recursive_function",
    "arithmetic",
    "control_flow",
    "transported_op_via_concept_citation_comment",
}


def _fixture_dirs() -> list[Path]:
    return sorted(path for path in FIXTURE_ROOT.iterdir() if path.is_dir())


def _load_expected(fixture_dir: Path) -> dict[str, Any]:
    return json.loads((fixture_dir / "expected.json").read_text(encoding="utf-8"))


def _execute_cases(source: str, function: str, cases: list[dict[str, Any]]) -> list[Any]:
    namespace: dict[str, Any] = {}
    exec(source, namespace)
    target = namespace[function]
    return [target(*case["args"]) for case in cases]


def _render_from_lift(source: str, fixture_name: str) -> str:
    result = lift_source(source, f"{fixture_name}/original.py")
    assert result.diagnostics == []
    assert len(result.ir) == 1
    entry = result.ir[0]
    emitted = emit_stub(
        function="",
        params=entry["param_names"],
        param_types=entry.get("param_types", []) or ["int"] * len(entry["param_names"]),
        return_type=entry.get("return_type", "") or "int",
        concept_name="UNNAMED-CONCEPT-1",
        term_shape=entry["term_shape"],
        operand_bindings=entry.get("operand_bindings"),
        source_function_name=entry["source_function_name"],
    )
    return emitted["source"]


def _write_executable_module(path: Path, source: str, function: str, cases: list[dict[str, Any]]) -> None:
    harness = {
        "function": function,
        "cases": cases,
    }
    path.write_text(
        source
        + "\nif __name__ == \"__main__\":\n"
        + "    import json\n"
        + f"    _fixture = {json.dumps(harness, separators=(',', ':'))!r}\n"
        + "    _fixture = json.loads(_fixture)\n"
        + "    _fn = globals()[_fixture[\"function\"]]\n"
        + "    _out = [_fn(*case[\"args\"]) for case in _fixture[\"cases\"]]\n"
        + "    print(json.dumps(_out, separators=(\",\", \":\"), sort_keys=True))\n",
        encoding="utf-8",
    )


def _compile_or_refusal(path: Path) -> dict[str, Any] | None:
    completed = subprocess.run(
        [sys.executable, "-m", "py_compile", str(path)],
        capture_output=True,
        text=True,
    )
    if completed.returncode == 0:
        return None
    return _composition_refusal("target-compile-failure", completed.stderr)


def _run_or_refusal(path: Path, expected: list[Any]) -> dict[str, Any] | None:
    completed = subprocess.run(
        [sys.executable, str(path)],
        capture_output=True,
        text=True,
    )
    observed = None
    if completed.returncode == 0 and completed.stdout.strip():
        observed = json.loads(completed.stdout)
    if completed.returncode == 0 and observed == expected:
        return None
    detail = json.dumps(
        {
            "expected": expected,
            "observed": observed,
            "stderr": completed.stderr,
        },
        separators=(",", ":"),
        sort_keys=True,
    )
    return _composition_refusal("target-behavior-divergence", detail)


def _composition_refusal(failure_kind: str, detail: str) -> dict[str, Any]:
    return {
        "envelope": {
            "declaredAt": "1970-01-01T00:00:00Z",
            "signature": "test-signature",
            "signer": "substrate:test",
        },
        "header": {
            "atoms_cids": [],
            "ccp_version": "ccp/1.0",
            "cid": "test-cid",
            "compose_input_cid": "test-compose-input-cid",
            "effect_set_cids": [],
            "failure_detail": detail,
            "failure_kind": failure_kind,
            "kind": "CompositionRefusal",
            "schemaVersion": "1",
        },
        "metadata": {
            "note": "python emit compile run conformance",
        },
    }


def test_python_fixture_registry_has_required_types() -> None:
    observed = {fixture_dir.name for fixture_dir in _fixture_dirs()}
    assert REQUIRED_FIXTURE_TYPES <= observed
    for fixture_dir in _fixture_dirs():
        expected = _load_expected(fixture_dir)
        assert expected["fixture_type"] == fixture_dir.name
        assert (fixture_dir / "original.py").is_file()
        assert expected["cases"]


def test_python_emit_compile_run_fixtures_match_original_behavior() -> None:
    for fixture_dir in _fixture_dirs():
        expected = _load_expected(fixture_dir)
        original_source = (fixture_dir / "original.py").read_text(encoding="utf-8")
        cases = expected["cases"]
        function = expected["function"]
        expected_outputs = [case["expected"] for case in cases]
        assert _execute_cases(original_source, function, cases) == expected_outputs

        emitted_source = _render_from_lift(original_source, fixture_dir.name)
        with tempfile.TemporaryDirectory() as tmp:
            emitted_path = Path(tmp) / "emitted.py"
            _write_executable_module(emitted_path, emitted_source, function, cases)
            compile_refusal = _compile_or_refusal(emitted_path)
            assert compile_refusal is None, compile_refusal
            behavior_refusal = _run_or_refusal(emitted_path, expected_outputs)
            assert behavior_refusal is None, behavior_refusal


def test_refusal_memento_flow_reports_compile_and_behavior_failure_kinds(tmp_path: Path) -> None:
    bad_compile = tmp_path / "bad_compile.py"
    bad_compile.write_text("def nope(:\n", encoding="utf-8")
    compile_refusal = _compile_or_refusal(bad_compile)
    assert compile_refusal is not None
    assert compile_refusal["header"]["failure_kind"] == "target-compile-failure"

    divergent = tmp_path / "divergent.py"
    _write_executable_module(divergent, "def value():\n    return 2\n", "value", [{"args": []}])
    behavior_refusal = _run_or_refusal(divergent, [1])
    assert behavior_refusal is not None
    assert behavior_refusal["header"]["failure_kind"] == "target-behavior-divergence"


def test_transported_op_fixture_can_emit_concept_citation_comment() -> None:
    source = (
        FIXTURE_ROOT
        / "transported_op_via_concept_citation_comment"
        / "original.py"
    ).read_text(encoding="utf-8")
    payload_line = next(
        line.strip()
        for line in source.splitlines()
        if line.strip().startswith("# provekit-concept: ")
    )
    transported_op = json.loads(payload_line.removeprefix("# provekit-concept: "))
    emitted = emit_stub(
        function="transported_skip",
        params=["x"],
        param_types=["int"],
        return_type="None",
        concept_name="missing-python-skip-carrier",
        transported_op=transported_op,
    )
    assert "# provekit-concept:" in emitted["source"]
    assert "# provekit-concept-payload-cid:" in emitted["source"]
    assert "pass" in emitted["source"]
