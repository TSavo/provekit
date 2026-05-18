"""
Body-template emission tests for the 7 declared-but-not-wired Python boundary
realizations added in #1158 (pk-1158-python-bodytemplates).

Three tests per template-emission variant (per feedback_discrimination_tests_per_variant):
  1. Positive  -- correct concept name + arity emits the expected body
  2. Structural -- emitted source compiles and is structurally well-formed
  3. Discrimination -- wrong arity or sibling concept refuses / does not match

Concepts covered:
  - concept:closure           (verbatim)
  - concept:exception         (verbatim)
  - concept:iterator          (verbatim)
  - concept:reference         (concept-citation-comment)
  - concept:dynamic-dispatch  (concept-citation-comment)
  - concept:double-dispatch   (concept-citation-comment)
  - concept:generic-instantiation (concept-citation-comment)
"""
from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/provekit-realize-python-core/src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))

from provekit_realize_python_core.realizer import (
    MissingTemplateError,
    body_template_for,
    emit_stub,
    sugar_carrier_entry_for,
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _compiled_namespace(source: str) -> dict[str, object]:
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "rendered.py"
        path.write_text(source, encoding="utf-8")
        subprocess.run(
            [sys.executable, "-m", "py_compile", str(path)],
            check=True,
            capture_output=True,
            text=True,
        )
    namespace: dict[str, object] = {}
    exec(source, namespace)
    return namespace


def _concept_citation_payloads(source: str) -> list[dict]:
    payloads: list[dict] = []
    for line in source.splitlines():
        stripped = line.strip()
        if stripped.startswith("# provekit-concept: "):
            payloads.append(json.loads(stripped.removeprefix("# provekit-concept: ")))
    return payloads


# ---------------------------------------------------------------------------
# concept:closure  (verbatim, min_params=1, max_params=1)
# ---------------------------------------------------------------------------

def test_closure_body_template_positive_emission() -> None:
    result = emit_stub(
        function="make_fn",
        params=["x"],
        param_types=["int"],
        return_type="object",
        concept_name="concept:closure",
    )
    assert result["source"] == "def make_fn(x):\n    return lambda: x\n"
    assert result["is_stub"] is False
    assert result["extension"] == "py"


def test_closure_body_template_structural_compiles() -> None:
    result = emit_stub(
        function="wrap_val",
        params=["v"],
        param_types=["str"],
        return_type="object",
        concept_name="concept:closure",
    )
    ns = _compiled_namespace(result["source"])
    wrap_val = ns["wrap_val"]
    fn = wrap_val("hello")
    assert callable(fn)
    assert fn() == "hello"


def test_closure_body_template_discrimination_wrong_arity_refuses() -> None:
    # min_params=1, max_params=1 -- two params must be refused
    body = body_template_for("concept:closure", ["a", "b"], ["int", "int"], "object")
    assert body is None


# ---------------------------------------------------------------------------
# concept:exception  (verbatim, min_params=2, max_params=2)
# ---------------------------------------------------------------------------

def test_exception_body_template_positive_emission() -> None:
    result = emit_stub(
        function="safe_call",
        params=["action", "fallback"],
        param_types=["int", "int"],
        return_type="int",
        concept_name="concept:exception",
    )
    assert result["source"] == (
        "def safe_call(action, fallback):\n"
        "    try:\n"
        "        return action\n"
        "    except Exception:\n"
        "        return fallback\n"
    )
    assert result["is_stub"] is False
    assert result["extension"] == "py"


def test_exception_body_template_structural_compiles() -> None:
    result = emit_stub(
        function="guarded",
        params=["risky", "safe"],
        param_types=["int", "int"],
        return_type="int",
        concept_name="concept:exception",
    )
    ns = _compiled_namespace(result["source"])
    guarded = ns["guarded"]
    assert guarded(1, 0) == 1
    assert guarded(0, 99) == 0


def test_exception_body_template_discrimination_wrong_arity_refuses() -> None:
    # requires exactly 2 params
    body = body_template_for("concept:exception", ["x"], ["int"], "int")
    assert body is None
    body_zero = body_template_for("concept:exception", [], [], "int")
    assert body_zero is None


# ---------------------------------------------------------------------------
# concept:iterator  (verbatim, min_params=1, max_params=1)
# ---------------------------------------------------------------------------

def test_iterator_body_template_positive_emission() -> None:
    result = emit_stub(
        function="get_iter",
        params=["col"],
        param_types=["list[int]"],
        return_type="object",
        concept_name="concept:iterator",
    )
    assert result["source"] == "def get_iter(col):\n    return iter(col)\n"
    assert result["is_stub"] is False
    assert result["extension"] == "py"


def test_iterator_body_template_structural_compiles() -> None:
    result = emit_stub(
        function="make_iter",
        params=["items"],
        param_types=["list[int]"],
        return_type="object",
        concept_name="concept:iterator",
    )
    ns = _compiled_namespace(result["source"])
    make_iter = ns["make_iter"]
    it = make_iter([1, 2, 3])
    assert next(it) == 1
    assert next(it) == 2


def test_iterator_body_template_discrimination_wrong_arity_refuses() -> None:
    # max_params=1 -- two params must be refused
    body = body_template_for("concept:iterator", ["a", "b"], ["int", "int"], "object")
    assert body is None


# ---------------------------------------------------------------------------
# concept:reference  (concept-citation-comment)
#
# The concept is not yet in the concept-shapes catalog, so emit_stub cannot
# build a full transported_op payload. Tests validate the body-template entry
# directly via sugar_carrier_entry_for, matching the audit's gap definition:
# "no template entry" -- we verify the entry exists with the correct shape.
# ---------------------------------------------------------------------------

def test_reference_citation_comment_positive_entry_found() -> None:
    entry = sugar_carrier_entry_for("concept:reference", ["x"], ["object"], "object")
    assert entry is not None
    assert entry.concept_name == "concept:reference"
    assert entry.template_kind == "concept-citation-comment"
    assert entry.template == "pass"


def test_reference_citation_comment_structural_loss_record() -> None:
    entry = sugar_carrier_entry_for("concept:reference", ["x"], ["object"], "object")
    assert entry is not None
    loss_value = entry.loss_record_contribution.get("value", {})
    assert "python-references-are-name-bindings" in loss_value
    item = loss_value["python-references-are-name-bindings"]
    assert item["head"] == "atomic"
    assert item["name"] == "python-references-are-name-bindings"
    assert item["args"] == []


def test_reference_citation_comment_discrimination_sibling_does_not_match() -> None:
    # concept:closure is verbatim, not citation-comment -- sugar_carrier returns None for it
    entry = sugar_carrier_entry_for("concept:closure", ["x"], ["object"], "object")
    assert entry is None


# ---------------------------------------------------------------------------
# concept:dynamic-dispatch  (concept-citation-comment)
# ---------------------------------------------------------------------------

def test_dynamic_dispatch_citation_comment_positive_entry_found() -> None:
    entry = sugar_carrier_entry_for("concept:dynamic-dispatch", ["obj"], ["object"], "object")
    assert entry is not None
    assert entry.concept_name == "concept:dynamic-dispatch"
    assert entry.template_kind == "concept-citation-comment"
    assert entry.template == "pass"


def test_dynamic_dispatch_citation_comment_structural_loss_record() -> None:
    entry = sugar_carrier_entry_for("concept:dynamic-dispatch", ["obj"], ["object"], "object")
    assert entry is not None
    loss_value = entry.loss_record_contribution.get("value", {})
    assert "python-mro-dict-lookup-has-no-syntactic-body" in loss_value
    item = loss_value["python-mro-dict-lookup-has-no-syntactic-body"]
    assert item["head"] == "atomic"
    assert item["name"] == "python-mro-dict-lookup-has-no-syntactic-body"
    assert item["args"] == []


def test_dynamic_dispatch_discrimination_body_template_returns_none() -> None:
    # body_template_for skips citation-comment entries -- must return None
    body = body_template_for("concept:dynamic-dispatch", ["obj"], ["object"], "object")
    assert body is None


# ---------------------------------------------------------------------------
# concept:double-dispatch  (concept-citation-comment)
# ---------------------------------------------------------------------------

def test_double_dispatch_citation_comment_positive_entry_found() -> None:
    entry = sugar_carrier_entry_for("concept:double-dispatch", ["a", "b"], ["object", "object"], "object")
    assert entry is not None
    assert entry.concept_name == "concept:double-dispatch"
    assert entry.template_kind == "concept-citation-comment"
    assert entry.template == "pass"


def test_double_dispatch_citation_comment_structural_loss_record() -> None:
    entry = sugar_carrier_entry_for("concept:double-dispatch", ["a", "b"], ["object", "object"], "object")
    assert entry is not None
    loss_value = entry.loss_record_contribution.get("value", {})
    assert "python-match-type-pair-not-emittable-as-body" in loss_value
    item = loss_value["python-match-type-pair-not-emittable-as-body"]
    assert item["head"] == "atomic"
    assert item["name"] == "python-match-type-pair-not-emittable-as-body"
    assert item["args"] == []


def test_double_dispatch_discrimination_body_template_returns_none() -> None:
    body = body_template_for("concept:double-dispatch", ["a", "b"], ["object", "object"], "object")
    assert body is None


# ---------------------------------------------------------------------------
# concept:generic-instantiation  (concept-citation-comment)
# ---------------------------------------------------------------------------

def test_generic_instantiation_citation_comment_positive_entry_found() -> None:
    entry = sugar_carrier_entry_for("concept:generic-instantiation", ["cls"], ["object"], "object")
    assert entry is not None
    assert entry.concept_name == "concept:generic-instantiation"
    assert entry.template_kind == "concept-citation-comment"
    assert entry.template == "pass"


def test_generic_instantiation_citation_comment_structural_loss_record() -> None:
    entry = sugar_carrier_entry_for("concept:generic-instantiation", ["cls"], ["object"], "object")
    assert entry is not None
    loss_value = entry.loss_record_contribution.get("value", {})
    assert "python-duck-typing-no-generic-instantiation-syntax" in loss_value
    item = loss_value["python-duck-typing-no-generic-instantiation-syntax"]
    assert item["head"] == "atomic"
    assert item["name"] == "python-duck-typing-no-generic-instantiation-syntax"
    assert item["args"] == []


def test_generic_instantiation_discrimination_body_template_returns_none() -> None:
    body = body_template_for("concept:generic-instantiation", ["cls"], ["object"], "object")
    assert body is None
