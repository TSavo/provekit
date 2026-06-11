from __future__ import annotations

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

from sugar_lift_py_tests.layer2 import lift_file_layer2
from sugar_lift_py_tests.translate_universe import translate_universe_for_callee

VENDOR_TRANSLATE = '''
_tab = bytes.maketrans(b"+/", b"-_")


def _enc(s):
    return s


def urlsafe(s):
    """Translate-shaped vendor body, CPython base64.urlsafe_b64encode shape."""
    return _enc(s).translate(_tab)
'''

VENDOR_SWAP = '''
_tab = bytes.maketrans(b"+/", b"/+")


def _enc(s):
    return s


def urlsafe(s):
    return _enc(s).translate(_tab)
'''

VENDOR_UNSTABLE = '''
_tab = bytes.maketrans(b"+/", b"-_")
_tab = bytes.maketrans(b"+/", b"-_")


def _enc(s):
    return s


def urlsafe(s):
    return _enc(s).translate(_tab)
'''

VENDOR_FLIPPED = '''
_tab = bytes.maketrans(b"+!", b"-_")


def _enc(s):
    return s


def urlsafe(s):
    return _enc(s).translate(_tab)
'''

VENDOR_PLAIN = '''
def plain(s):
    return s + "x"
'''


@pytest.fixture()
def vendor_path(tmp_path, monkeypatch):
    def write(module_name: str, source: str) -> None:
        (tmp_path / f"{module_name}.py").write_text(textwrap.dedent(source))

    monkeypatch.syspath_prepend(str(tmp_path))
    translate_universe_for_callee.cache_clear()
    return write


def test_walk_derives_forbidden_set(vendor_path):
    vendor_path("venduniv_ok", VENDOR_TRANSLATE)
    universe, refusal = translate_universe_for_callee("venduniv_ok.urlsafe")
    assert refusal is None
    assert universe is not None
    assert universe.forbidden == "+/"
    assert universe.table_name == "_tab"
    assert universe.qualname == "venduniv_ok.urlsafe"


def test_swap_table_refuses_no_universe(vendor_path):
    # maketrans(b"+/", b"/+") maps '+' to '/' and back: every mapped char is
    # reintroduced, so NO complement claim exists. Must refuse by name, never
    # emit an empty/false universe.
    vendor_path("venduniv_swap", VENDOR_SWAP)
    universe, refusal = translate_universe_for_callee("venduniv_swap.urlsafe")
    assert universe is None
    assert refusal is not None
    assert "reintroduces" in refusal.reason


def test_unstable_table_refuses(vendor_path):
    vendor_path("venduniv_unstable", VENDOR_UNSTABLE)
    universe, refusal = translate_universe_for_callee("venduniv_unstable.urlsafe")
    assert universe is None
    assert refusal is not None


def test_non_translate_body_is_not_a_candidate(vendor_path):
    vendor_path("venduniv_plain", VENDOR_PLAIN)
    universe, refusal = translate_universe_for_callee("venduniv_plain.plain")
    assert universe is None
    assert refusal is None  # fog was never a candidate; no refusal owed


def test_partial_swap_keeps_surviving_chars(vendor_path):
    vendor_path(
        "venduniv_partial",
        '''
_tab = bytes.maketrans(b"+/", b"/_")


def _enc(s):
    return s


def urlsafe(s):
    return _enc(s).translate(_tab)
''',
    )
    # '+' -> '/' reintroduces '/'; '/' -> '_' removes it. Forbidden = {+}.
    universe, refusal = translate_universe_for_callee("venduniv_partial.urlsafe")
    assert refusal is None
    assert universe is not None
    assert universe.forbidden == "+"


# --- layer2 integration: the ::universe sibling row ---


def _lift(source: str):
    return lift_file_layer2(textwrap.dedent(source), "test_mod.py")


def _universe_atoms(out):
    # The universe is a CONJUNCT inside the base's conjoined ::assertion --
    # never a sibling contract (the verifier conjoins by name; a sibling
    # verifies alone and is vacuously consistent).
    from sugar_lift_py_tests.layer2 import _iter_conjuncts

    atoms = []
    for d in out.decls:
        if d.name.endswith("::assertion") and d.inv is not None:
            atoms.extend(
                a
                for a in _iter_conjuncts(d.inv)
                if a.name == "str.chars-not-in-set"
            )
    return atoms


def _universe_decls(out):
    # Distinct universe claims, deduped by content: coalescing may repeat
    # idempotent conjuncts; WHICH universes exist is the property.
    return sorted({(a.args[0], a.args[1]) for a in _universe_atoms(out)}, key=str)


def test_universe_row_emitted_for_translate_callee(vendor_path):
    vendor_path("venduniv_l2", VENDOR_TRANSLATE)
    out = _lift(
        """
        import venduniv_l2

        def test_urlsafe():
            assert venduniv_l2.urlsafe("abc") == "abc"
        """
    )
    atoms = _universe_atoms(out)
    assert len(atoms) == 1
    assert atoms[0].args[1].value == "+/"
    # contact is structural: the atom lives INSIDE the conjoined assertion
    assert any(d.name.endswith("::assertion") and "urlsafe" in d.name for d in out.decls)


def test_universe_row_emitted_once_per_base_across_tests(vendor_path):
    # Same callee + same concrete args in TWO test functions: the bases
    # collapse cross-location (EUF), and the bundle must carry exactly ONE
    # ::universe decl -- a duplicate name would collide at mint.
    vendor_path("venduniv_once", VENDOR_TRANSLATE)
    out = _lift(
        """
        import venduniv_once

        def test_urlsafe_a():
            assert venduniv_once.urlsafe("abc") == "abc"

        def test_urlsafe_b():
            assert venduniv_once.urlsafe("abc") == "abc"
        """
    )
    assert len(_universe_decls(out)) == 1


def test_refused_walk_surfaces_loud_warning(vendor_path):
    vendor_path("venduniv_warn", VENDOR_SWAP)
    out = _lift(
        """
        import venduniv_warn

        def test_urlsafe():
            assert venduniv_warn.urlsafe("abc") == "abc"
        """
    )
    assert not _universe_decls(out)
    reasons = [w.reason for w in out.warnings if "translate-universe" in w.item_name]
    assert reasons and "reintroduces" in reasons[0]


def test_bad_twin_flip_changes_forbidden_set(vendor_path):
    # Perturb the vendor's maketrans FROM side: the emitted universe must
    # change with it -- proves the row carries the walked table, not
    # decoration.
    vendor_path("venduniv_flip", VENDOR_FLIPPED)
    out = _lift(
        """
        import venduniv_flip

        def test_urlsafe():
            assert venduniv_flip.urlsafe("abc") == "abc"
        """
    )
    atoms = _universe_atoms(out)
    assert len(atoms) == 1
    assert atoms[0].args[1].value == "!+"


def test_non_translate_callee_emits_nothing_and_no_warning(vendor_path):
    vendor_path("venduniv_fog", VENDOR_PLAIN)
    out = _lift(
        """
        import venduniv_fog

        def test_plain():
            assert venduniv_fog.plain("a") == "ax"
        """
    )
    assert not _universe_decls(out)
    assert not [w for w in out.warnings if "translate-universe" in w.item_name]


# --- the rstrip family (no-suffix-chars): the token-padding shape ---

VENDOR_RSTRIP = '''
def _inner(s):
    return s


def b64e(s):
    s = _inner(s)
    return _inner(s).rstrip(b"=")
'''


def test_rstrip_family_walks(vendor_path):
    vendor_path("vendrstrip_ok", VENDOR_RSTRIP)
    universe, refusal = translate_universe_for_callee("vendrstrip_ok.b64e")
    assert refusal is None
    assert universe is not None
    assert universe.kind == "no-suffix-chars"
    assert universe.forbidden == "="


def test_rstrip_emits_negated_suffix_conjunct(vendor_path):
    vendor_path("vendrstrip_l2", VENDOR_RSTRIP)
    out = _lift(
        """
        import vendrstrip_l2

        def test_token():
            assert vendrstrip_l2.b64e("abc") == "abc"
        """
    )
    from sugar_lift_py_tests.layer2 import _iter_conjuncts

    suffix_atoms = []
    for d in out.decls:
        if d.name.endswith("::assertion") and d.inv is not None:
            for f in [d.inv] if not hasattr(d.inv, "operands") else list(d.inv.operands):
                if getattr(f, "kind", None) == "not":
                    inner = f.operands[0]
                    if getattr(inner, "name", None) == "suffix-of":
                        suffix_atoms.append(inner)
    assert len(suffix_atoms) == 1
    assert suffix_atoms[0].args[0].value == "="


def test_rstrip_vendor_vector_endswith_refuses(vendor_path):
    vendor_path("vendrstrip_bad", VENDOR_RSTRIP)
    vendor_path(
        "test_vendrstrip_bad",
        """
        import vendrstrip_bad

        def test_vector():
            assert vendrstrip_bad.b64e("abc") == "abc="
        """,
    )
    universe, refusal = translate_universe_for_callee("vendrstrip_bad.b64e")
    assert universe is None
    assert refusal is not None and "sample-gate" in refusal.reason


# --- from-import callee resolution ---


def test_from_import_module_alias_claims_and_walks(vendor_path, tmp_path):
    # `from vend_pkg import enc` where enc IS a module: alias-bound
    # (find_spec-verified), the callsite claims, the universe attaches.
    pkg = tmp_path / "vendfi_pkg"
    pkg.mkdir()
    (pkg / "__init__.py").write_text("")
    (pkg / "enc.py").write_text(textwrap.dedent(VENDOR_TRANSLATE))
    translate_universe_for_callee.cache_clear()
    out = _lift(
        """
        from vendfi_pkg import enc

        def test_urlsafe():
            assert enc.urlsafe("abc") == "abc"
        """
    )
    atoms = _universe_atoms(out)
    assert len(atoms) == 1
    assert atoms[0].args[1].value == "+/"


def test_from_import_function_qualifies_base_and_walks(vendor_path):
    # `from vendmod import urlsafe`: the bare-name callsite keys to the
    # QUALIFIED base (cross-proof conjoin alignment) and the walk resolves.
    vendor_path("vendfi_fn", VENDOR_TRANSLATE)
    out = _lift(
        """
        from vendfi_fn import urlsafe

        def test_urlsafe():
            assert urlsafe("abc") == "abc"
        """
    )
    atoms = _universe_atoms(out)
    assert len(atoms) == 1
    assert any(
        d.name.startswith("vendfi_fn.urlsafe#euf#")
        for d in out.decls
        if d.name.endswith("::assertion")
    )


def test_from_import_class_does_not_alias(vendor_path):
    # A from-imported NON-module that is not walkable must not crash or
    # mis-claim; behavior stays as before (no universe, no error).
    vendor_path("vendfi_cls", "class Thing:\n    @staticmethod\n    def go(x):\n        return x\n")
    out = _lift(
        """
        from vendfi_cls import Thing

        def test_thing():
            assert Thing.go("abc") == "abc"
        """
    )
    assert not _universe_atoms(out)


# --- the member-of-values family: return TABLE[x] (census #1 cheap shape) ---

VENDOR_TABLE = '''
_STATUSES = ("active", "paused", "deleted")


def status_name(i):
    return _STATUSES[i]
'''


def test_table_subscript_family_walks(vendor_path):
    vendor_path("vendtbl_ok", VENDOR_TABLE)
    universe, refusal = translate_universe_for_callee("vendtbl_ok.status_name")
    assert refusal is None
    assert universe is not None
    assert universe.kind == "member-of-values"
    assert universe.values == ("active", "paused", "deleted")


def test_table_subscript_emits_membership_disjunction(vendor_path):
    vendor_path("vendtbl_l2", VENDOR_TABLE)
    out = _lift(
        """
        import vendtbl_l2

        def test_status():
            assert vendtbl_l2.status_name(0) == "active"
        """
    )
    from sugar_lift_py_tests.layer2 import _iter_conjuncts

    ors = []
    for d in out.decls:
        if d.name.endswith("::assertion") and d.inv is not None:
            stack = [d.inv]
            while stack:
                f = stack.pop()
                if getattr(f, "kind", None) == "or":
                    ors.append(f)
                elif getattr(f, "kind", None) in ("and", "not"):
                    stack.extend(f.operands)
    assert len(ors) == 1
    assert len(ors[0].operands) == 3


def test_mutable_table_refuses(vendor_path):
    vendor_path(
        "vendtbl_list",
        '''
_STATUSES = ["active", "paused"]


def status_name(i):
    return _STATUSES[i]
''',
    )
    universe, refusal = translate_universe_for_callee("vendtbl_list.status_name")
    assert universe is None
    assert refusal is not None
    assert "tuple-literal" in refusal.reason


def test_mixed_type_table_refuses(vendor_path):
    vendor_path(
        "vendtbl_mixed",
        '''
_STATUSES = ("active", 2)


def status_name(i):
    return _STATUSES[i]
''',
    )
    universe, refusal = translate_universe_for_callee("vendtbl_mixed.status_name")
    assert universe is None
    assert refusal is not None and "all-string" in refusal.reason


def test_rebound_table_refuses(vendor_path):
    vendor_path(
        "vendtbl_rebound",
        '''
_STATUSES = ("active",)
_STATUSES = ("active", "paused")


def status_name(i):
    return _STATUSES[i]
''',
    )
    universe, refusal = translate_universe_for_callee("vendtbl_rebound.status_name")
    assert universe is None
    assert refusal is not None


def test_table_vendor_vector_outside_table_refuses(vendor_path):
    vendor_path("vendtbl_gate", VENDOR_TABLE)
    vendor_path(
        "test_vendtbl_gate",
        """
        import vendtbl_gate

        def test_vector():
            assert vendtbl_gate.status_name(0) == "archived"
        """,
    )
    universe, refusal = translate_universe_for_callee("vendtbl_gate.status_name")
    assert universe is None
    assert refusal is not None and "sample-gate" in refusal.reason


def test_table_flip_changes_values(vendor_path):
    vendor_path(
        "vendtbl_flip",
        VENDOR_TABLE.replace('"deleted"', '"removed"'),
    )
    universe, _ = translate_universe_for_callee("vendtbl_flip.status_name")
    assert universe.values == ("active", "paused", "removed")


# --- the guard-then-raise family: census #1 (23,082 bodies) ---

VENDOR_GUARDED = '''
def scale(x, factor):
    """Guarded vendor fn: x must be non-negative, factor must not be 0."""
    if x < 0:
        raise ValueError("negative")
    if factor == 0:
        raise ValueError("zero factor")
    return x * factor
'''


def test_guard_universe_walks(vendor_path):
    from sugar_lift_py_tests.translate_universe import guard_universe_for_callee

    guard_universe_for_callee.cache_clear()
    vendor_path("vendguard_ok", VENDOR_GUARDED)
    guards, refusal = guard_universe_for_callee("vendguard_ok.scale")
    assert refusal is None
    assert guards is not None
    assert len(guards.clauses) == 2
    assert (guards.clauses[0].param_name, guards.clauses[0].op, guards.clauses[0].literal) == ("x", "<", 0)
    assert (guards.clauses[1].param_name, guards.clauses[1].op, guards.clauses[1].literal) == ("factor", "=", 0)


def test_guard_universe_emits_negated_comparisons(vendor_path):
    from sugar_lift_py_tests.translate_universe import guard_universe_for_callee

    guard_universe_for_callee.cache_clear()
    vendor_path("vendguard_l2", VENDOR_GUARDED)
    out = _lift(
        """
        import vendguard_l2

        def test_scale():
            assert vendguard_l2.scale(-3, 2) == -6
        """
    )
    nots = []
    for d in out.decls:
        if d.name.endswith("::assertion") and d.inv is not None:
            stack = [d.inv]
            while stack:
                f = stack.pop()
                if getattr(f, "kind", None) == "not":
                    nots.append(f.operands[0])
                elif getattr(f, "kind", None) == "and":
                    stack.extend(f.operands)
    # both guards instantiate at the concrete args (-3, 2):
    # not(-3 < 0) -- which check will refute -- and not(2 = 0).
    assert len(nots) == 2
    names = sorted(n.name for n in nots)
    assert names == ["<", "="]


def test_guard_vendor_vector_firing_guard_refuses(vendor_path):
    from sugar_lift_py_tests.translate_universe import guard_universe_for_callee

    guard_universe_for_callee.cache_clear()
    vendor_path("vendguard_bad", VENDOR_GUARDED)
    vendor_path(
        "test_vendguard_bad",
        """
        import vendguard_bad

        def test_vector():
            assert vendguard_bad.scale(-1, 2) == -2
        """,
    )
    guards, refusal = guard_universe_for_callee("vendguard_bad.scale")
    assert guards is None
    assert refusal is not None and "sample-gate" in refusal.reason


def test_unreadable_guards_skip_without_poisoning(vendor_path):
    from sugar_lift_py_tests.translate_universe import guard_universe_for_callee

    guard_universe_for_callee.cache_clear()
    vendor_path(
        "vendguard_mixed",
        '''
def f(x, y):
    if complicated(x):
        raise ValueError("opaque")
    if y < 0:
        raise ValueError("negative")
    return x + y
''',
    )
    guards, refusal = guard_universe_for_callee("vendguard_mixed.f")
    assert refusal is None
    assert guards is not None
    assert len(guards.clauses) == 1
    assert guards.clauses[0].param_name == "y"


def test_unguarded_body_is_not_a_candidate(vendor_path):
    from sugar_lift_py_tests.translate_universe import guard_universe_for_callee

    guard_universe_for_callee.cache_clear()
    vendor_path("vendguard_none", "def f(x):\n    return x + 1\n")
    guards, refusal = guard_universe_for_callee("vendguard_none.f")
    assert guards is None and refusal is None


# --- the table-loop family: census #2 (17,781 bodies) ---

VENDOR_LOOP = '''
_HEX = "0123456789abcdef"


def hexify(data):
    out = []
    for b in data:
        out.append(_HEX[b >> 4])
        out.append(_HEX[b & 15])
    return ":".join(out)
'''


def test_table_loop_walks_with_union_and_separator(vendor_path):
    vendor_path("vendloop_ok", VENDOR_LOOP)
    universe, refusal = translate_universe_for_callee("vendloop_ok.hexify")
    assert refusal is None
    assert universe is not None
    assert universe.kind == "chars-in-set"
    assert universe.forbidden == "".join(sorted(set("0123456789abcdef:")))


def test_table_loop_emits_positive_membership(vendor_path):
    vendor_path("vendloop_l2", VENDOR_LOOP)
    out = _lift(
        """
        import vendloop_l2

        def test_hexify():
            assert vendloop_l2.hexify("ab") == "36:31:36:32"
        """
    )
    atoms = []
    from sugar_lift_py_tests.layer2 import _iter_conjuncts

    for d in out.decls:
        if d.name.endswith("::assertion") and d.inv is not None:
            atoms.extend(
                a for a in _iter_conjuncts(d.inv) if a.name == "str.chars-in-set"
            )
    assert len(atoms) == 1
    assert "f" in atoms[0].args[1].value and ":" in atoms[0].args[1].value


def test_table_loop_str_accumulator(vendor_path):
    vendor_path(
        "vendloop_str",
        '''
_DIG = "01"


def bits(n):
    out = ""
    for b in n:
        out += _DIG[b]
    return out
''',
    )
    universe, refusal = translate_universe_for_callee("vendloop_str.bits")
    assert refusal is None
    assert universe.forbidden == "01"


def test_table_loop_foreign_append_refuses(vendor_path):
    vendor_path(
        "vendloop_foreign",
        '''
_HEX = "0123456789abcdef"


def hexify(data, extra):
    out = []
    for b in data:
        out.append(extra)
    return "".join(out)
''',
    )
    universe, refusal = translate_universe_for_callee("vendloop_foreign.hexify")
    assert universe is None
    assert refusal is not None
    assert "not a pinned-table element" in refusal.reason


def test_table_loop_unstable_table_refuses(vendor_path):
    vendor_path(
        "vendloop_unstable",
        '''
_HEX = "0123456789abcdef"
_HEX = "0123456789ABCDEF"


def hexify(data):
    out = []
    for b in data:
        out.append(_HEX[b])
    return "".join(out)
''',
    )
    universe, refusal = translate_universe_for_callee("vendloop_unstable.hexify")
    assert universe is None
    assert refusal is not None and "bound more than once" in refusal.reason


def test_table_loop_vendor_vector_outside_union_refuses(vendor_path):
    vendor_path("vendloop_gate", VENDOR_LOOP)
    vendor_path(
        "test_vendloop_gate",
        """
        import vendloop_gate

        def test_vector():
            assert vendloop_gate.hexify("a") == "6Z"
        """,
    )
    universe, refusal = translate_universe_for_callee("vendloop_gate.hexify")
    assert universe is None
    assert refusal is not None and "sample-gate" in refusal.reason
