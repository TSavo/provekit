"""Grammar debt ledger: totality, discrimination, and the real-corpus join.

The ledger's claim is structural: every bucket grammar_census.classify can
emit on this interpreter is classified lifted/debt/membrane, no fourth
status, no silence. Each invariant here is confirmed by BREAKING it
(synthetic grammar growth, malformed rows, an unknown report shape), not
just by watching it hold.
"""

from __future__ import annotations

import ast
import json
import sys
import textwrap
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[4]
PY_TESTS_SRC = ROOT / "implementations/python/sugar-lift-py-tests/src"
PKG_SRC = ROOT / "implementations/python/sugar-lift-python-source/src"
for p in (str(PY_TESTS_SRC), str(PKG_SRC)):
    if p not in sys.path:
        sys.path.insert(0, p)

from sugar_lift_py_tests import grammar_ledger
from sugar_lift_py_tests.grammar_census import classify
from sugar_lift_py_tests.grammar_ledger import (
    DEBT,
    FLAG_LEDGER,
    LEDGER,
    LIFTED,
    MEMBRANE,
    join_report,
    ledger_cid,
    malformed_rows,
    unaccounted_buckets,
)

FIXTURE = (
    Path(__file__).parent
    / "fixtures"
    / "census-report-2026-06-12-top1000.json"
)


def _fn(src: str) -> ast.FunctionDef:
    tree = ast.parse(textwrap.dedent(src))
    fn = next(
        n
        for n in ast.walk(tree)
        if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef))
    )
    return fn


# ---------------------------------------------------------------------------
# Positive: the floor holds on the running interpreter.
# ---------------------------------------------------------------------------


def test_floor_holds_on_this_interpreter():
    assert unaccounted_buckets() == []
    assert malformed_rows(LEDGER) == []
    assert malformed_rows(FLAG_LEDGER) == []


def test_ledger_cid_is_blake3_512():
    cid = ledger_cid()
    assert cid.startswith("blake3-512:")
    hexpart = cid.split(":", 1)[1]
    assert len(hexpart) == 128
    assert all(c in "0123456789abcdef" for c in hexpart)
    # deterministic: the classification is the claim, same bytes same cid
    assert cid == ledger_cid()


# ---------------------------------------------------------------------------
# Discrimination: grammar growth is a hole, never a silent zero.
# ---------------------------------------------------------------------------


def test_synthetic_stmt_growth_is_reported():
    class FutureStmt(ast.stmt):
        pass

    grown = frozenset(
        grammar_ledger._grammar_classes(ast.stmt) | {FutureStmt}
    )
    holes = unaccounted_buckets(stmt_classes=grown)
    assert holes == ["non-return:FutureStmt"]


def test_synthetic_expr_growth_is_reported():
    class FutureExpr(ast.expr):
        pass

    grown = frozenset(
        grammar_ledger._grammar_classes(ast.expr) | {FutureExpr}
    )
    holes = unaccounted_buckets(expr_classes=grown)
    assert holes == ["return-other:FutureExpr"]


def test_missing_row_is_reported_not_skipped():
    pruned = dict(LEDGER)
    del pruned["pure-delegation"]
    # named shapes are covered by the classify-exemplar bridge below, not
    # the parametric floor; pruning a PARAMETRIC row must trip the floor.
    del pruned["non-return:Raise"]
    assert "non-return:Raise" in unaccounted_buckets(ledger=pruned)


# ---------------------------------------------------------------------------
# Discrimination: malformed rows, one per breakage kind.
# ---------------------------------------------------------------------------


def test_lifted_symbol_must_resolve():
    bad = {"x": {"status": LIFTED, "symbol": "no_such_walk", "family": "f"}}
    assert any("does not resolve" in m for m in malformed_rows(bad))


def test_lifted_requires_family():
    bad = {
        "x": {
            "status": LIFTED,
            "symbol": "translate_universe_for_callee",
            "family": "",
        }
    }
    assert any("without family" in m for m in malformed_rows(bad))


def test_debt_requires_owes():
    assert any(
        "without owes" in m
        for m in malformed_rows({"x": {"status": DEBT, "owes": ""}})
    )


def test_membrane_requires_reason():
    assert any(
        "without reason" in m
        for m in malformed_rows({"x": {"status": MEMBRANE}})
    )


def test_unknown_status_is_refused():
    # the fourth bucket ("didn't get around to it") cannot be spelled
    assert any(
        "unknown status" in m
        for m in malformed_rows({"x": {"status": "later"}})
    )


# ---------------------------------------------------------------------------
# Bridge: every bucket the classifier emits on an exemplar battery has a
# ledger row. The battery spans named shapes, parametric fall-throughs,
# and flags — classify() is total, so each exemplar MUST land somewhere,
# and everywhere it lands must be classified.
# ---------------------------------------------------------------------------

EXEMPLARS = [
    "def f():\n    '''doc only'''",
    "def f(s):\n    return s.translate(_t)",
    "def f(s):\n    return s.rstrip(b'=')",
    "def f(s):\n    return s.replace('+', '-')",
    "def f(xs):\n    return ','.join(xs)",
    "def f(s):\n    return s.encode('ascii')",
    "def f(x):\n    return '{}!'.format(x)",
    "def f(s):\n    return s.upper()",
    "def f(o):\n    return o.compute(1)",
    "def f(x):\n    return g(x)",
    "def f(x):\n    y = x\n    return g(y)",
    "def f(x):\n    return handlers[0](x)",
    "def f(x):\n    return TABLE[x]",
    "def f():\n    return 'v'",
    "def f(x):\n    return x",
    "def f(x):\n    return x + 1",
    "def f(x):\n    return x > 0",
    "def f(x):\n    return x if x else 0",
    "def f(x):\n    return f'{x}!'",
    "def f(x):\n    return (x, x)",
    "def f(o):\n    return o.attr",
    "def f(x):\n    if x < 0:\n        raise ValueError(x)\n    return x",
    (
        "def f(t):\n    out = []\n    for c in t:\n"
        "        out.append(T[c])\n    return ''.join(out)"
    ),
    "def f(x):\n    x.fire()",
    "def f(x):\n    assert x",
    "def f(x):\n    y = x",
    "def f(x):\n    if x:\n        pass",
    "def f(x):\n    for i in x:\n        pass",
    "def f(x):\n    while x:\n        break",
    "def f(x):\n    with x:\n        pass",
    "def f(x):\n    try:\n        pass\n    except Exception:\n        pass",
    "def f(x):\n    raise ValueError(x)",
    "def f(x):\n    pass",
    "def f(x):\n    return",
    "def f(x):\n    del x",
    "def f(x):\n    import os",
    "def f(x):\n    from os import path",
    "def f(x):\n    global g",
    "def f(x):\n    match x:\n        case _:\n            pass",
    "def f(x):\n    def inner():\n        pass",
    "def f(x):\n    class C:\n        pass",
    "async def f(x):\n    return await x",
    "async def f(x):\n    async with x:\n        pass",
    "async def f(x):\n    async for i in x:\n        pass",
    "def f(x):\n    return lambda: x",
    "def f(x):\n    return [i for i in x]",
    "def f(x):\n    return {i for i in x}",
    "def f(x):\n    return {i: i for i in x}",
    "def f(x):\n    return (i for i in x)",
    "def f(x):\n    return (y := x)",
    "def f(x):\n    return -x",
    "def f(x):\n    return ~x",
    "def f(x, i):\n    return x.data[i]",
    "def f(x):\n    return (yield x)",
]


@pytest.mark.parametrize("src", EXEMPLARS)
def test_every_emitted_bucket_is_classified(src):
    bucket, flags = classify(_fn(src))
    assert bucket in LEDGER, f"unclassified bucket {bucket!r} for {src!r}"
    for flag in flags:
        assert flag in FLAG_LEDGER, f"unclassified flag {flag!r}"


def test_exemplar_battery_spans_all_three_statuses():
    statuses = {
        LEDGER[classify(_fn(src))[0]]["status"] for src in EXEMPLARS
    }
    assert statuses == {LIFTED, DEBT, MEMBRANE}


# ---------------------------------------------------------------------------
# The real-corpus join: the vendored 2026-06-12 top-1000 report scores
# cleanly, and an unknown shape is refused loudly.
# ---------------------------------------------------------------------------


def test_real_census_report_joins_clean():
    report = json.load(open(FIXTURE, encoding="utf-8"))
    doc = join_report(report)
    assert doc["classified"] == report["classified"] == 1520368
    assert sum(doc["totals"].values()) == doc["classified"]
    assert doc["ledger_cid"].startswith("blake3-512:")
    # the scoreboard is honest in both directions: every status total is
    # present and non-negative (the original "debt dominates" assertion
    # was a snapshot, not an invariant — the campaign exists to falsify
    # it, and did, at 50.02% lifted on 2026-06-12)
    assert all(v >= 0 for v in doc["totals"].values())
    assert doc["totals"][MEMBRANE] > 0  # named residue never vanishes
    assert doc["debt_ranked"][0]["count"] >= doc["debt_ranked"][-1]["count"]
    # every membrane row carries its reason into the joined doc
    assert all(r["reason"] for r in doc["membrane"])


def test_unknown_report_shape_is_refused():
    report = {
        "packages": 1,
        "files": 1,
        "functions": 1,
        "shapes_ranked": [{"shape": "return-quantum-foam", "count": 1}],
    }
    with pytest.raises(LookupError, match="return-quantum-foam"):
        join_report(report)
