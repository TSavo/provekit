from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import subprocess
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from types import ModuleType
from typing import Any

import pytest

ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/provekit-realize-python-core/src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))

from provekit_realize_python_core.rpc import dispatch

FIXTURES_DIR = ROOT / "implementations/python/conformance/fixtures"
EXPECTED_FIXTURES = {
    "arithmetic_add",
    "control_flow_if",
    "hello_world",
    "recursive_factorial",
    "transported_op_drop",
}
COMPILE_FAILURE = "target-compile-failure"
BEHAVIOR_DIVERGENCE = "target-behavior-divergence"


@dataclass(frozen=True)
class RunResult:
    stdout: str
    return_value: Any
    exit_code: int


def _load_fixtures() -> list[dict[str, Any]]:
    files = sorted(FIXTURES_DIR.glob("*.json"))
    assert {path.stem for path in files} == EXPECTED_FIXTURES

    fixtures = []
    for path in files:
        fixture = json.loads(path.read_text(encoding="utf-8"))
        assert fixture["name"] == path.stem
        assert set(("original_source", "declared_test_inputs", "expected_output")).issubset(
            fixture
        )
        assert isinstance(fixture["realize_request"], dict)
        fixtures.append(fixture)
    return fixtures


def _composition_refusal(
    fixture_name: str,
    failure_kind: str,
    failure_detail: str,
) -> dict[str, Any]:
    return {
        "kind": "CompositionRefusalMemento",
        "header": {
            "kind": "composition-refusal",
            "schema_version": "1",
            "fixture": fixture_name,
            "failure_kind": failure_kind,
            "failure_detail": failure_detail,
        },
    }


@pytest.mark.parametrize("fixture", _load_fixtures(), ids=lambda fixture: fixture["name"])
def test_python_carrier_fixtures_emit_compile_and_match_behavior(
    fixture: dict[str, Any],
    tmp_path: Path,
) -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": fixture["name"],
            "method": "provekit.plugin.invoke",
            "params": fixture["realize_request"],
        }
    )
    assert "error" not in response, response
    emitted_source = response["result"]["source"]
    assert response["result"]["extension"] == "py"

    emitted_file = tmp_path / f"{fixture['name']}_emitted.py"
    emitted_file.write_text(emitted_source, encoding="utf-8")
    refusal = _compile_emitted_python(fixture["name"], emitted_file)
    assert refusal is None, json.dumps(refusal, sort_keys=True)

    if fixture.get("behavior_comparison") == "carrier_comment":
        expected = fixture["expected_output"]
        assert isinstance(expected, dict)
        needle = expected["carrier_comment_contains"]
        if needle not in emitted_source:
            pytest.fail(
                json.dumps(
                    _composition_refusal(
                        fixture["name"],
                        BEHAVIOR_DIVERGENCE,
                        f"emitted source did not preserve carrier comment {needle!r}",
                    ),
                    sort_keys=True,
                )
            )
        return

    original_file = tmp_path / f"{fixture['name']}_original.py"
    original_file.write_text(fixture["original_source"], encoding="utf-8")
    original_results = _run_declared_inputs(original_file, fixture)
    emitted_results = _run_declared_inputs(emitted_file, fixture)
    expected_results = fixture["expected_output"]

    assert [asdict(result) for result in original_results] == expected_results
    if original_results != emitted_results:
        pytest.fail(
            json.dumps(
                _composition_refusal(
                    fixture["name"],
                    BEHAVIOR_DIVERGENCE,
                    (
                        f"original output {[_jsonable(result) for result in original_results]!r} "
                        f"did not match emitted output "
                        f"{[_jsonable(result) for result in emitted_results]!r}"
                    ),
                ),
                sort_keys=True,
            )
        )


def test_compile_failure_refusal_uses_target_compile_failure(tmp_path: Path) -> None:
    emitted_file = tmp_path / "broken.py"
    emitted_file.write_text("def broken(:\n    pass\n", encoding="utf-8")

    refusal = _compile_emitted_python("broken", emitted_file)

    assert refusal is not None
    assert refusal["header"]["failure_kind"] == COMPILE_FAILURE


def test_behavior_divergence_refusal_uses_target_behavior_divergence() -> None:
    refusal = _composition_refusal(
        "mismatch",
        BEHAVIOR_DIVERGENCE,
        "original output did not match emitted output",
    )

    assert refusal["header"]["failure_kind"] == BEHAVIOR_DIVERGENCE


def _compile_emitted_python(fixture_name: str, emitted_file: Path) -> dict[str, Any] | None:
    proc = subprocess.run(
        ["python3", "-m", "py_compile", str(emitted_file)],
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode == 0:
        return None
    return _composition_refusal(
        fixture_name,
        COMPILE_FAILURE,
        (proc.stderr or proc.stdout).strip(),
    )


def _run_declared_inputs(path: Path, fixture: dict[str, Any]) -> list[RunResult]:
    module = _load_module(path)
    calls = fixture["declared_test_inputs"]
    assert isinstance(calls, list)
    results = []
    for call in calls:
        entrypoint = call["entrypoint"]
        args = call.get("args", [])
        stdout = io.StringIO()
        try:
            with contextlib.redirect_stdout(stdout):
                return_value = getattr(module, entrypoint)(*args)
        except SystemExit as exc:
            results.append(
                RunResult(
                    stdout=stdout.getvalue(),
                    return_value=None,
                    exit_code=int(exc.code) if isinstance(exc.code, int) else 1,
                )
            )
        except Exception as exc:
            results.append(
                RunResult(
                    stdout=stdout.getvalue(),
                    return_value=repr(exc),
                    exit_code=1,
                )
            )
        else:
            results.append(
                RunResult(
                    stdout=stdout.getvalue(),
                    return_value=return_value,
                    exit_code=0,
                )
            )
    return results


def _load_module(path: Path) -> ModuleType:
    module_name = f"provekit_conformance_{path.stem}"
    spec = importlib.util.spec_from_file_location(module_name, path)
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _jsonable(result: RunResult) -> dict[str, Any]:
    return asdict(result)
