# SPDX-License-Identifier: Apache-2.0
#
# Recognize runs THROUGH the Source Oracle for a lean `.proof`.
#
# A lean `.proof` (SourceMemento: locus + CIDs, NO inline ast_template -- the
# vendor shape after PROVEKIT_LEAN_SOURCE=1) carries no template to recognize
# with. The recognizer's vendor-template path asks the Source Oracle to resolve
# the ast_template from the on-disk source at the pinned locus, CID-verified, and
# REFUSES LOUDLY on drift (bind_rpc `_resolve_via_source_oracle`, consumed at the
# `ast_template is None` splice in `_vendor_proof_binding_templates`). These tests
# lock that splice: the template the recognizer pattern-matches with is the EXACT
# template a non-lean `.proof` would carry inline -- reconstructed from source by
# the oracle, not read from the `.proof` -- and a tampered source yields nothing.
import os
from pathlib import Path

from provekit_lift_python_source.bind_lifter import lift_source
from provekit_lift_python_source.bind_rpc import _resolve_via_source_oracle


def _entry(tmp: Path, rel: str, src: str, *, lean: bool) -> dict:
    (tmp / rel).parent.mkdir(parents=True, exist_ok=True)
    (tmp / rel).write_text(src, encoding="utf-8")
    prev = os.environ.get("PROVEKIT_LEAN_SOURCE")
    if lean:
        os.environ["PROVEKIT_LEAN_SOURCE"] = "1"
    try:
        return next(
            e
            for e in lift_source(src, rel, layer="library-bindings").ir
            if e.get("kind") == "library-sugar-binding-entry"
        )
    finally:
        # Restore the prior value rather than unconditionally deleting it.
        if prev is None:
            os.environ.pop("PROVEKIT_LEAN_SOURCE", None)
        else:
            os.environ["PROVEKIT_LEAN_SOURCE"] = prev


def test_oracle_reconstructs_the_exact_inline_template(tmp_path: Path) -> None:
    src = "def add(x, y):\n    return x + y\n"
    # the lean binding carries only locus + CIDs (no inline template)...
    lean = _entry(tmp_path / "lean", "pkg/calc.py", src, lean=True)
    assert "ast_template" not in lean["body_source"]
    assert lean["body_source"]["source_cid"] and lean["body_source"]["template_cid"]
    # ...and the oracle reconstructs the SAME template a non-lean `.proof` inlines.
    full = _entry(tmp_path / "full", "pkg/calc.py", src, lean=False)
    inline_template = full["body_source"]["ast_template"]
    assert inline_template is not None

    resolved = _resolve_via_source_oracle(str(tmp_path / "lean"), lean)
    assert resolved is not None, "oracle must resolve the lean binding"
    assert resolved["ast_template"] == inline_template, (
        "the recognizer's oracle-resolved template must equal the inline one"
    )


def test_recognizer_gets_nothing_when_the_source_drifts(tmp_path: Path) -> None:
    lean = _entry(tmp_path, "pkg/calc.py", "def add(x, y):\n    return x + y\n", lean=True)
    # clean source resolves...
    assert _resolve_via_source_oracle(str(tmp_path), lean) is not None
    # ...tamper the on-disk source: the bytes you'd run no longer recompute to the
    # pinned CID, so the Source Oracle refuses -> the recognizer has no template.
    (tmp_path / "pkg" / "calc.py").write_text(
        "def add(x, y):\n    return x - y\n", encoding="utf-8"
    )
    assert _resolve_via_source_oracle(str(tmp_path), lean) is None
