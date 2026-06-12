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


# --- the pre-conjoined path: multi-assert bodies carry universes too ---


def test_preconjoined_path_carries_universes(vendor_path):
    # Two asserts in ONE test body route through the characterization
    # (pre-conjoined) classifier, which previously emitted no universes.
    vendor_path("vendpre_l2", VENDOR_TRANSLATE)
    out = _lift(
        """
        import vendpre_l2

        def test_urlsafe_twice():
            assert vendpre_l2.urlsafe("abc") == "abc"
            assert vendpre_l2.urlsafe("xyz") == "xyz"
        """
    )
    atoms = _universe_atoms_anywhere(out)
    assert len(atoms) == 2  # one universe per distinct subject


def _universe_atoms_anywhere(out):
    from sugar_lift_py_tests.layer2 import _iter_conjuncts

    atoms = []
    for d in out.decls:
        if d.inv is None:
            continue
        stack = [d.inv]
        while stack:
            f = stack.pop()
            if getattr(f, "kind", None) in ("and", "or", "not"):
                stack.extend(f.operands)
            elif getattr(f, "name", None) in (
                "str.chars-not-in-set",
                "str.chars-in-set",
            ):
                atoms.append(f)
    return atoms


def test_preconjoined_guard_universe_injects(vendor_path):
    from sugar_lift_py_tests.translate_universe import guard_universe_for_callee

    guard_universe_for_callee.cache_clear()
    vendor_path("vendpre_guard", VENDOR_GUARDED)
    out = _lift(
        """
        import vendpre_guard

        def test_scale_twice():
            assert vendpre_guard.scale(-3, 2) == -6
            assert vendpre_guard.scale(4, 2) == 8
        """
    )
    nots = []
    for d in out.decls:
        if d.inv is None:
            continue
        stack = [d.inv]
        while stack:
            f = stack.pop()
            if getattr(f, "kind", None) == "not":
                nots.append(f)
            elif getattr(f, "kind", None) in ("and", "or"):
                stack.extend(f.operands)
    # two callsites x two guard clauses, MINUS the shared-factor dedupe:
    # not(-3 < 0), not(4 < 0), and ONE not(2 = 0) (identical for both
    # callsites; idempotent conjuncts dedupe).
    assert len(nots) == 3


# --- regression: the corpus (Werkzeug) caught a NameError on the
# single-assertion operator-dispatch path. _Connective was referenced but
# never imported into layer2; a boolean-connective assertion over an
# operator-dispatch ctor reaches the unimported name. ---


def test_connective_operator_dispatch_does_not_NameError():
    out = _lift(
        """
        class C:
            def __eq__(self, other):
                return True

        def test_dispatch():
            assert (C() == C()) and (1 < 2)
        """
    )
    # the point is simply that lifting completes without NameError;
    # whatever the classification, no exception escapes.
    assert out is not None


# --- EUF dropout on non-deterministic callees (corpus finding: Werkzeug
# generate_password_hash salted hash made same-args EUF unify two unequal
# values -> false contradiction) ---


def test_nondeterministic_callee_detected(vendor_path):
    from sugar_lift_py_tests.translate_universe import callee_is_nondeterministic

    callee_is_nondeterministic.cache_clear()
    vendor_path(
        "vendnd_salt",
        '''
import secrets


def gen_salt(n):
    return "".join(secrets.choice("abc") for _ in range(n))


def make_hash(pw):
    return pw + gen_salt(8)
''',
    )
    # direct marker (secrets.choice in gen_salt) and transitive (make_hash
    # -> gen_salt -> secrets) both detected.
    assert callee_is_nondeterministic("vendnd_salt.gen_salt")
    assert callee_is_nondeterministic("vendnd_salt.make_hash")


def test_deterministic_callee_not_flagged(vendor_path):
    from sugar_lift_py_tests.translate_universe import callee_is_nondeterministic

    callee_is_nondeterministic.cache_clear()
    vendor_path("vendnd_pure", "def f(x):\n    return x + 1\n")
    assert not callee_is_nondeterministic("vendnd_pure.f")


def test_nondeterministic_callee_drops_euf_unification(vendor_path):
    from sugar_lift_py_tests.translate_universe import callee_is_nondeterministic

    callee_is_nondeterministic.cache_clear()
    vendor_path(
        "vendnd_l2",
        '''
import secrets


def gen_salt(n):
    return secrets.token_hex(n)


def make_hash(pw):
    return pw + gen_salt(8)
''',
    )
    # the Werkzeug shape: same-args twice, asserted UNEQUAL. With EUF
    # dropout the two calls are independent -> NO false contradiction.
    out = _lift(
        """
        import vendnd_l2

        def test_salted():
            h1 = vendnd_l2.make_hash("secret")
            h2 = vendnd_l2.make_hash("secret")
            assert h1 != h2
        """
    )
    # no contract should argument-key make_hash to a shared euf base
    euf_bases = [
        d.name for d in out.decls if "make_hash#euf#" in d.name
    ]
    assert not euf_bases, euf_bases


def test_unresolvable_callee_stays_pure_conservative():
    from sugar_lift_py_tests.translate_universe import callee_is_nondeterministic

    callee_is_nondeterministic.cache_clear()
    # no such module: evidence-based detector returns False (keeps current
    # sound-conservative unification where we have no body to inspect).
    assert not callee_is_nondeterministic("no_such_module_xyz.f")


# --- return-constant family (census #1, 34k bodies): the equality universal ---

VENDOR_CONST = '''
def version():
    return "3.1.4"


def always_true(x):
    return True


def answer(*a, **k):
    return 42
'''


def test_constant_universe_walks(vendor_path):
    from sugar_lift_py_tests.translate_universe import constant_universe_for_callee

    constant_universe_for_callee.cache_clear()
    vendor_path("vendconst_ok", VENDOR_CONST)
    u, r = constant_universe_for_callee("vendconst_ok.version")
    assert r is None and u is not None
    assert (u.value, u.value_kind) == ("3.1.4", "str")
    u2, _ = constant_universe_for_callee("vendconst_ok.always_true")
    assert (u2.value, u2.value_kind) == (True, "bool")
    u3, _ = constant_universe_for_callee("vendconst_ok.answer")
    assert (u3.value, u3.value_kind) == (42, "int")


def test_constant_guard_prefix_still_constant(vendor_path):
    from sugar_lift_py_tests.translate_universe import constant_universe_for_callee

    constant_universe_for_callee.cache_clear()
    vendor_path(
        "vendconst_guard",
        '''
def f(x):
    if x < 0:
        raise ValueError
    return "ok"
''',
    )
    u, r = constant_universe_for_callee("vendconst_guard.f")
    assert r is None and u is not None and u.value == "ok"


def test_multiple_returns_not_constant(vendor_path):
    from sugar_lift_py_tests.translate_universe import constant_universe_for_callee

    constant_universe_for_callee.cache_clear()
    vendor_path(
        "vendconst_multi",
        'def f(x):\n    if x:\n        return "a"\n    return "b"\n',
    )
    u, r = constant_universe_for_callee("vendconst_multi.f")
    assert u is None and r is None  # not a candidate


def test_constant_emits_equality_and_refutes_wrong(vendor_path):
    constant_universe_for_callee_clear()
    vendor_path("vendconst_l2", VENDOR_CONST)
    out = _lift(
        """
        import vendconst_l2

        def test_version():
            assert vendconst_l2.version() == "3.1.4"
        """
    )
    # the universe equality over the SAME subject coexists with the sworn
    # equality; a bad twin asserting a different constant would conjoin to
    # unsat. Here we just confirm an equality atom to the constant is present.
    from sugar_lift_py_tests.ir import str_const

    eqs = []
    for d in out.decls:
        if d.name.endswith("::assertion") and d.inv is not None:
            stack = [d.inv]
            while stack:
                f = stack.pop()
                if getattr(f, "name", None) == "=" and str_const("3.1.4") in getattr(f, "args", ()):
                    eqs.append(f)
                elif getattr(f, "kind", None) in ("and", "or", "not"):
                    stack.extend(f.operands)
    assert eqs


def test_constant_vendor_vector_mismatch_refuses(vendor_path):
    from sugar_lift_py_tests.translate_universe import constant_universe_for_callee

    constant_universe_for_callee.cache_clear()
    vendor_path("vendconst_gate", 'def version():\n    return "3.1.4"\n')
    vendor_path(
        "test_vendconst_gate",
        'import vendconst_gate\n\ndef test_v():\n    assert vendconst_gate.version() == "9.9.9"\n',
    )
    u, r = constant_universe_for_callee("vendconst_gate.version")
    assert u is None and r is not None and "sample-gate" in r.reason


def constant_universe_for_callee_clear():
    from sugar_lift_py_tests.translate_universe import constant_universe_for_callee

    constant_universe_for_callee.cache_clear()


# --- return-predicate family (census #2, 24k bodies): ground eval at args ---

VENDOR_PRED = '''
def is_neg(x):
    return x < 0


def in_range(x):
    return 0 <= x and x < 100


def is_empty(s):
    return s == ""
'''


def test_predicate_universe_walks(vendor_path):
    from sugar_lift_py_tests.translate_universe import predicate_universe_for_callee

    predicate_universe_for_callee.cache_clear()
    vendor_path("vendpred_ok", VENDOR_PRED)
    u, r = predicate_universe_for_callee("vendpred_ok.is_neg")
    assert r is None and u is not None and u.params == ("x",)


def test_predicate_ground_eval(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        predicate_universe_for_callee,
        eval_predicate,
    )

    predicate_universe_for_callee.cache_clear()
    vendor_path("vendpred_eval", VENDOR_PRED)
    u, _ = predicate_universe_for_callee("vendpred_eval.is_neg")
    assert eval_predicate(u.expr, {"x": 5}) is False
    assert eval_predicate(u.expr, {"x": -3}) is True
    rng, _ = predicate_universe_for_callee("vendpred_eval.in_range")
    assert eval_predicate(rng.expr, {"x": 50}) is True
    assert eval_predicate(rng.expr, {"x": 200}) is False


def test_predicate_emits_bool_equality_at_callsite(vendor_path):
    from sugar_lift_py_tests.translate_universe import predicate_universe_for_callee
    from sugar_lift_py_tests.ir import bool_const

    predicate_universe_for_callee.cache_clear()
    vendor_path("vendpred_l2", VENDOR_PRED)
    out = _lift(
        """
        import vendpred_l2

        def test_neg():
            assert vendpred_l2.is_neg(5) == False
        """
    )
    # the universe should compute is_neg(5)==False and conjoin subject==False
    falses = []
    for d in out.decls:
        if d.name.endswith("::assertion") and d.inv is not None:
            stack = [d.inv]
            while stack:
                f = stack.pop()
                if getattr(f, "name", None) == "=" and bool_const(False) in getattr(f, "args", ()):
                    falses.append(f)
                elif getattr(f, "kind", None) in ("and", "or", "not"):
                    stack.extend(f.operands)
    assert falses


def test_predicate_impure_not_candidate(vendor_path):
    from sugar_lift_py_tests.translate_universe import predicate_universe_for_callee

    predicate_universe_for_callee.cache_clear()
    vendor_path(
        "vendpred_impure",
        "def f(x):\n    return helper(x) < 0\n",
    )
    u, r = predicate_universe_for_callee("vendpred_impure.f")
    assert u is None and r is None  # call in predicate -> not purely evaluable


# --- return-replace-literals family (single-char replace complement) ---

VENDOR_REPLACE = '''
def slugify(s):
    return s.replace(" ", "-")
'''


def test_replace_family_walks(vendor_path):
    vendor_path("vendrepl_ok", VENDOR_REPLACE)
    u, r = translate_universe_for_callee("vendrepl_ok.slugify")
    assert r is None and u is not None
    assert u.kind == "chars-not-in-set" and u.forbidden == " "


def test_replace_noop_refuses(vendor_path):
    vendor_path("vendrepl_noop", 'def f(s):\n    return s.replace("x", "x")\n')
    u, r = translate_universe_for_callee("vendrepl_noop.f")
    assert u is None and r is not None and "no-op" in r.reason


def test_replace_multichar_not_candidate(vendor_path):
    vendor_path("vendrepl_multi", 'def f(s):\n    return s.replace("ab", "cd")\n')
    u, r = translate_universe_for_callee("vendrepl_multi.f")
    assert u is None and r is None  # multi-char: no clean char guarantee


def test_replace_emits_complement(vendor_path):
    vendor_path("vendrepl_l2", VENDOR_REPLACE)
    out = _lift(
        """
        import vendrepl_l2

        def test_slug():
            assert vendrepl_l2.slugify("a b") == "a-b"
        """
    )
    from sugar_lift_py_tests.layer2 import _iter_conjuncts

    atoms = [
        a
        for d in out.decls
        if d.name.endswith("::assertion") and d.inv is not None
        for a in _iter_conjuncts(d.inv)
        if a.name == "str.chars-not-in-set"
    ]
    assert atoms and atoms[0].args[1].value == " "


def test_replace_vendor_vector_with_char_refuses(vendor_path):
    vendor_path("vendrepl_gate", VENDOR_REPLACE)
    vendor_path(
        "test_vendrepl_gate",
        'import vendrepl_gate\n\ndef test_s():\n    assert vendrepl_gate.slugify("x") == "a b"\n',
    )
    u, r = translate_universe_for_callee("vendrepl_gate.slugify")
    assert u is None and r is not None and "sample-gate" in r.reason


# --- return-format family (literal prefix → prefix-of) ---

VENDOR_FORMAT = '''
def err(code):
    return "Error {}".format(code)


def ver(a, b):
    return f"v{a}.{b}"


def leading_placeholder(x):
    return "{}!".format(x)
'''


def test_format_dotformat_prefix(vendor_path):
    vendor_path("vendfmt_a", VENDOR_FORMAT)
    u, r = translate_universe_for_callee("vendfmt_a.err")
    assert r is None and u is not None
    assert u.kind == "prefix" and u.forbidden == "Error "


def test_format_fstring_prefix(vendor_path):
    vendor_path("vendfmt_b", VENDOR_FORMAT)
    u, _ = translate_universe_for_callee("vendfmt_b.ver")
    assert u.kind == "prefix" and u.forbidden == "v"


def test_format_leading_placeholder_not_candidate(vendor_path):
    vendor_path("vendfmt_c", VENDOR_FORMAT)
    u, r = translate_universe_for_callee("vendfmt_c.leading_placeholder")
    assert u is None and r is None  # starts with placeholder, no prefix


def test_format_emits_prefix_of(vendor_path):
    vendor_path("vendfmt_l2", VENDOR_FORMAT)
    out = _lift(
        """
        import vendfmt_l2

        def test_err():
            assert vendfmt_l2.err(404) == "Error 404"
        """
    )
    from sugar_lift_py_tests.layer2 import _iter_conjuncts

    atoms = [
        a
        for d in out.decls
        if d.name.endswith("::assertion") and d.inv is not None
        for a in _iter_conjuncts(d.inv)
        if a.name == "prefix-of"
    ]
    assert atoms and atoms[0].args[0].value == "Error "


def test_format_vendor_vector_wrong_prefix_refuses(vendor_path):
    vendor_path("vendfmt_gate", VENDOR_FORMAT)
    vendor_path(
        "test_vendfmt_gate",
        'import vendfmt_gate\n\ndef test_e():\n    assert vendfmt_gate.err(1) == "Oops 1"\n',
    )
    u, r = translate_universe_for_callee("vendfmt_gate.err")
    assert u is None and r is not None and "sample-gate" in r.reason


# ---------------------------------------------------------------------------
# Walrus-in-guard soundness (falsePass closed 2026-06-12). A NamedExpr in a
# stripped guard's test REBINDS a name before the remaining body runs:
# `if (x := x + 10) > 100: raise` then `return x > 5` returns True for
# f(1) at runtime, while ground-evaluating the return expression at the
# callsite's argument computes False — an emitted equality would DISCHARGE
# a wrong claim. Every strip site must refuse; each refusal is confirmed
# against a pure twin that still licenses (the refusal is the walrus, not
# collateral).
# ---------------------------------------------------------------------------

VENDOR_WALRUS_PREDICATE = '''
def f(x):
    if (x := x + 10) > 100:
        raise ValueError(x)
    return x > 5
'''

VENDOR_PURE_PREDICATE = '''
def f(x):
    if x > 100:
        raise ValueError(x)
    return x > 5
'''

VENDOR_WALRUS_CONSTANT = '''
def f(x):
    if (x := x + 10) > 100:
        raise ValueError(x)
    return "v"
'''

VENDOR_PURE_CONSTANT = '''
def f(x):
    if x > 100:
        raise ValueError(x)
    return "v"
'''


def test_walrus_guard_predicate_runtime_divergence_is_real(vendor_path):
    # The evidence, kept executable: the runtime and the naive ground-eval
    # disagree, which is exactly why the walk below must refuse.
    import importlib

    vendor_path("vendwalrus_evidence", VENDOR_WALRUS_PREDICATE)
    mod = importlib.import_module("vendwalrus_evidence")
    assert mod.f(1) is True  # x rebinds to 11; 11 > 5
    # naive evaluation of the return expression at the callsite's arg:
    assert (1 > 5) is False  # what a stripped-guard walk would emit


def test_walrus_guard_predicate_refuses(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        predicate_universe_for_callee,
    )

    predicate_universe_for_callee.cache_clear()
    vendor_path("vendwalrus_pred", VENDOR_WALRUS_PREDICATE)
    universe, refusal = predicate_universe_for_callee("vendwalrus_pred.f")
    assert universe is None
    assert refusal is not None
    assert "walrus" in refusal.reason


def test_pure_guard_predicate_still_licenses(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        predicate_universe_for_callee,
    )

    predicate_universe_for_callee.cache_clear()
    vendor_path("vendpure_pred", VENDOR_PURE_PREDICATE)
    universe, refusal = predicate_universe_for_callee("vendpure_pred.f")
    assert refusal is None
    assert universe is not None
    assert universe.params == ("x",)


def test_walrus_guard_constant_refuses(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        constant_universe_for_callee,
    )

    constant_universe_for_callee.cache_clear()
    vendor_path("vendwalrus_const", VENDOR_WALRUS_CONSTANT)
    universe, refusal = constant_universe_for_callee("vendwalrus_const.f")
    assert universe is None
    assert refusal is not None
    assert "walrus" in refusal.reason


def test_pure_guard_constant_still_licenses(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        constant_universe_for_callee,
    )

    constant_universe_for_callee.cache_clear()
    vendor_path("vendpure_const", VENDOR_PURE_CONSTANT)
    universe, refusal = constant_universe_for_callee("vendpure_const.f")
    assert refusal is None
    assert universe is not None
    assert universe.value == "v"


def test_walrus_guard_guard_family_refuses(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        guard_universe_for_callee,
    )

    guard_universe_for_callee.cache_clear()
    vendor_path("vendwalrus_guard", VENDOR_WALRUS_PREDICATE)
    guards, refusal = guard_universe_for_callee("vendwalrus_guard.f")
    assert guards is None
    assert refusal is not None
    assert "walrus" in refusal.reason


def test_pure_guard_guard_family_still_licenses(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        guard_universe_for_callee,
    )

    guard_universe_for_callee.cache_clear()
    vendor_path("vendpure_guard", VENDOR_PURE_PREDICATE)
    guards, refusal = guard_universe_for_callee("vendpure_guard.f")
    assert refusal is None
    assert guards is not None
    assert len(guards.clauses) == 1


# ---------------------------------------------------------------------------
# pure-delegation + identity family (census: 57k delegation bodies + the
# param arm of return-name's 146k). The body forwards verbatim, so the
# output EQUALS the forwarded term — eq between call terms in EUF, zero
# new atoms. The license is syntactic (the body IS the claim); every
# refusal class is named and each is confirmed against a twin that still
# licenses.
# ---------------------------------------------------------------------------

VENDOR_DELEG = '''
def g(a, b):
    return a + b


def f(a, b):
    return g(b, a)


def ident(x):
    return x


def second(a, b):
    return b


def partial(a):
    return g(a, 5)


def forward_all(*args):
    return g(*args)
'''


def _deleg(callee):
    from sugar_lift_py_tests.translate_universe import (
        delegation_universe_for_callee,
    )

    delegation_universe_for_callee.cache_clear()
    return delegation_universe_for_callee(callee)


def test_identity_walks(vendor_path):
    vendor_path("venddeleg_ok", VENDOR_DELEG)
    u, r = _deleg("venddeleg_ok.ident")
    assert r is None and u is not None
    assert (u.kind, u.param_index) == ("identity", 0)
    u2, r2 = _deleg("venddeleg_ok.second")
    assert r2 is None and (u2.kind, u2.param_index) == ("identity", 1)


def test_delegation_walks_with_reordered_params(vendor_path):
    vendor_path("venddeleg_ok2", VENDOR_DELEG)
    u, r = _deleg("venddeleg_ok2.f")
    assert r is None and u is not None
    assert u.kind == "delegation"
    assert u.delegate == "venddeleg_ok2.g"
    assert u.args == (("param", 1), ("param", 0))


def test_delegation_walks_with_literal_arg(vendor_path):
    vendor_path("venddeleg_ok3", VENDOR_DELEG)
    u, r = _deleg("venddeleg_ok3.partial")
    assert r is None and u is not None
    assert u.args == (("param", 0), ("lit", 5, "int"))


def test_splat_forwarding_walks(vendor_path):
    vendor_path("venddeleg_ok4", VENDOR_DELEG)
    u, r = _deleg("venddeleg_ok4.forward_all")
    assert r is None and u is not None
    assert u.kind == "delegation-splat"
    assert u.delegate == "venddeleg_ok4.g"


def test_free_name_return_is_not_identity(vendor_path):
    vendor_path(
        "venddeleg_free", "Y = 3\n\ndef f(x):\n    return Y\n"
    )
    u, r = _deleg("venddeleg_free.f")
    assert u is None and r is None  # return-name's pinned-local arm, not ours


def test_rebound_param_is_not_identity(vendor_path):
    vendor_path(
        "venddeleg_rebind", "def f(x):\n    x = x + 1\n    return x\n"
    )
    u, r = _deleg("venddeleg_rebind.f")
    assert u is None and r is None  # two statements: not a forwarding body


def test_walrus_guard_delegation_refuses(vendor_path):
    vendor_path(
        "venddeleg_walrus",
        "def f(x):\n"
        "    if (x := x + 10) > 100:\n"
        "        raise ValueError(x)\n"
        "    return x\n",
    )
    u, r = _deleg("venddeleg_walrus.f")
    assert u is None and r is not None and "walrus" in r.reason


def test_pure_guard_identity_still_licenses(vendor_path):
    vendor_path(
        "venddeleg_guarded",
        "def f(x):\n"
        "    if x > 100:\n"
        "        raise ValueError(x)\n"
        "    return x\n",
    )
    u, r = _deleg("venddeleg_guarded.f")
    assert r is None and u is not None and u.kind == "identity"


def test_keyword_forwarding_refuses(vendor_path):
    vendor_path(
        "venddeleg_kw",
        "def g(a, b):\n    return a\n\ndef f(a, b):\n    return g(a, b=b)\n",
    )
    u, r = _deleg("venddeleg_kw.f")
    assert u is None and r is not None and "keyword" in r.reason


def test_computed_arg_refuses(vendor_path):
    vendor_path(
        "venddeleg_computed",
        "def g(a):\n    return a\n\ndef f(a):\n    return g(a + 1)\n",
    )
    u, r = _deleg("venddeleg_computed.f")
    assert u is None and r is not None
    assert "neither a parameter nor an ascii literal" in r.reason


def test_imported_delegate_refuses(vendor_path):
    vendor_path(
        "venddeleg_import",
        "from os.path import join\n\ndef f(a):\n    return join(a)\n",
    )
    u, r = _deleg("venddeleg_import.f")
    assert u is None and r is not None
    assert "not a module-level function" in r.reason


def test_nondeterministic_delegate_refuses(vendor_path):
    vendor_path(
        "venddeleg_nondet",
        "import random\n\n"
        "def g(a):\n    return a + random.random()\n\n"
        "def f(a):\n    return g(a)\n",
    )
    u, r = _deleg("venddeleg_nondet.f")
    assert u is None and r is not None and "nondeterminism" in r.reason


def test_rebound_delegate_refuses(vendor_path):
    vendor_path(
        "venddeleg_rebound",
        "def g(a):\n    return a\n\ng = len\n\ndef f(a):\n    return g(a)\n",
    )
    u, r = _deleg("venddeleg_rebound.f")
    assert u is None and r is not None and "binding events" in r.reason


def test_global_puncture_delegate_refuses(vendor_path):
    vendor_path(
        "venddeleg_glob",
        "def g(a):\n    return a\n\n"
        "def swap():\n    global g\n    g = len\n\n"
        "def f(a):\n    return g(a)\n",
    )
    u, r = _deleg("venddeleg_glob.f")
    assert u is None and r is not None and "global" in r.reason


def test_self_delegation_refuses(vendor_path):
    vendor_path(
        "venddeleg_self", "def f(a):\n    return f(a)\n"
    )
    u, r = _deleg("venddeleg_self.f")
    assert u is None and r is not None and "self-delegation" in r.reason


def test_async_delegate_refuses(vendor_path):
    vendor_path(
        "venddeleg_async",
        "async def g(a):\n    return a\n\ndef f(a):\n    return g(a)\n",
    )
    u, r = _deleg("venddeleg_async.f")
    assert u is None and r is not None and "async" in r.reason


def test_splat_with_extra_arg_refuses(vendor_path):
    vendor_path(
        "venddeleg_splatx",
        "def g(*a):\n    return a\n\n"
        "def f(*args):\n    return g(*args, 1)\n",
    )
    u, r = _deleg("venddeleg_splatx.f")
    assert u is None and r is not None and "splat" in r.reason


def test_multiple_returns_not_delegation(vendor_path):
    vendor_path(
        "venddeleg_multi",
        "def g(a):\n    return a\n\n"
        "def f(a):\n    if a:\n        return g(a)\n    return g(a)\n",
    )
    u, r = _deleg("venddeleg_multi.f")
    assert u is None and r is None  # not a single-return forwarding body


def _delegation_eq_atoms(out, delegate_head_fragment):
    from sugar_lift_py_tests.layer2 import _iter_conjuncts

    found = []
    for d in out.decls:
        if d.name.endswith("::assertion") and d.inv is not None:
            for a in _iter_conjuncts(d.inv):
                if getattr(a, "name", None) != "=":
                    continue
                for side in getattr(a, "args", ()):
                    if delegate_head_fragment in getattr(side, "name", ""):
                        found.append(a)
    return found


def test_delegation_emits_call_term_equality(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        delegation_universe_for_callee,
    )

    delegation_universe_for_callee.cache_clear()
    vendor_path("venddeleg_l2", VENDOR_DELEG)
    out = _lift(
        """
        import venddeleg_l2

        def test_route():
            assert venddeleg_l2.f(1, 2) == 3
        """
    )
    # the universe ties callresult_<f>(1,2) to callresult_<g>(2,1): claims
    # about f and claims about g now meet in one term. A consumer swearing
    # venddeleg_l2.g(2, 1) != 3 elsewhere would conjoin to UNSAT.
    atoms = _delegation_eq_atoms(out, "callresult_venddeleg_l2_g_a2")
    assert atoms, [d.name for d in out.decls]


def test_identity_universe_contradicts_wrong_claim(vendor_path):
    # THE BAD TWIN: the consumer swears ident(7) == 8; the identity
    # universe swears the output IS the argument (== 7). Both equalities
    # land in the SAME conjoined ::assertion inv — the conjunction is
    # UNSAT and the wrong claim refutes. (The good twin's universe
    # conjunct is byte-identical to the consumer's own assertion and is
    # correctly deduped — the universe adds information exactly when the
    # claim deviates.)
    from sugar_lift_py_tests.translate_universe import (
        delegation_universe_for_callee,
    )
    from sugar_lift_py_tests.ir import _ConstInt

    delegation_universe_for_callee.cache_clear()
    vendor_path("venddeleg_l2i", VENDOR_DELEG)
    out = _lift(
        """
        import venddeleg_l2i

        def test_ident():
            assert venddeleg_l2i.ident(7) == 8
        """
    )
    from sugar_lift_py_tests.layer2 import _iter_conjuncts

    claimed, universe = [], []
    for d in out.decls:
        if d.name.endswith("::assertion") and d.inv is not None:
            for a in _iter_conjuncts(d.inv):
                if getattr(a, "name", None) != "=":
                    continue
                args = getattr(a, "args", ())
                if len(args) == 2 and isinstance(args[1], _ConstInt):
                    (claimed if args[1].value == 8 else universe).append(
                        (a, args[1].value)
                    )
    assert claimed, [d.name for d in out.decls]
    assert [v for _, v in universe] == [7], universe


def test_impure_delegate_emits_no_equality_but_warns(vendor_path):
    # DEFENSE IN DEPTH, the case only the walk catches: a nondeterminism
    # source FOUR hops from f (f->g->h->i->random). callee_is_nondeterministic
    # scans depth 3 from f and clears it, so the assertion still lifts and
    # argument-keys; the walk then scans depth 3 from the DELEGATE g,
    # reaches the source, and refuses to equate — surfaced as a loud
    # warning, never silence. (One hop closer and the callee gate itself
    # de-keys the call before any universe is consulted — also covered:
    # test_nondeterministic_delegate_refuses exercises the walk directly.)
    from sugar_lift_py_tests.translate_universe import (
        callee_is_nondeterministic,
        delegation_universe_for_callee,
    )

    callee_is_nondeterministic.cache_clear()
    delegation_universe_for_callee.cache_clear()
    vendor_path(
        "venddeleg_l2bad",
        "import random\n\n"
        "def i(a):\n    return a + random.random()\n\n"
        "def h(a):\n    return i(a)\n\n"
        "def g(a):\n    return h(a)\n\n"
        "def f(a):\n    return g(a)\n",
    )
    assert not callee_is_nondeterministic("venddeleg_l2bad.f")
    out = _lift(
        """
        import venddeleg_l2bad

        def test_route():
            assert venddeleg_l2bad.f(1) == 2
        """
    )
    atoms = _delegation_eq_atoms(out, "callresult_venddeleg_l2bad_g")
    assert not atoms
    assert any(
        "delegation-universe" in w.item_name and "nondeterminism" in w.reason
        for w in out.warnings
    ), [(w.item_name, w.reason) for w in out.warnings]


# ---------------------------------------------------------------------------
# Decorated defs are not their bodies (falsePass closed 2026-06-12). The
# name binds whatever the decorator returns: @negate over `return True`
# runs False while the body walk swore True — through EVERY family, since
# they all resolve via _resolve_vendor_function. The fix is at that one
# chokepoint: a decorated def is the same non-candidate class as a C
# extension (the source we can read is not the callable that runs).
# ---------------------------------------------------------------------------

VENDOR_DECORATED = '''
def negate(fn):
    def inner(*a, **k):
        return not fn(*a, **k)
    return inner


@negate
def truth():
    return True


def plain_truth():
    return True
'''


def test_decorator_runtime_divergence_is_real(vendor_path):
    # The evidence, kept executable: the decorated callable and the def
    # body disagree, which is why resolution below must refuse.
    import importlib

    vendor_path("venddeco_evidence", VENDOR_DECORATED)
    mod = importlib.import_module("venddeco_evidence")
    assert mod.truth() is False  # the decorator negates the body
    assert mod.plain_truth() is True


def test_decorated_vendor_is_not_walkable_any_family(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        constant_universe_for_callee,
        delegation_universe_for_callee,
        guard_universe_for_callee,
        predicate_universe_for_callee,
    )

    vendor_path("venddeco_all", VENDOR_DECORATED)
    for walk in (
        constant_universe_for_callee,
        predicate_universe_for_callee,
        guard_universe_for_callee,
        delegation_universe_for_callee,
    ):
        walk.cache_clear()
        u, r = walk("venddeco_all.truth")
        assert u is None and r is None, (walk.__name__, u, r)


def test_undecorated_twin_still_walks(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        constant_universe_for_callee,
    )

    constant_universe_for_callee.cache_clear()
    vendor_path("venddeco_twin", VENDOR_DECORATED)
    u, r = constant_universe_for_callee("venddeco_twin.plain_truth")
    assert r is None and u is not None
    assert (u.value, u.value_kind) == (True, "bool")


def test_decorated_delegate_refuses(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        delegation_universe_for_callee,
    )

    delegation_universe_for_callee.cache_clear()
    vendor_path(
        "venddeco_deleg",
        "def wrap(fn):\n    return fn\n\n"
        "@wrap\ndef g(a):\n    return a\n\n"
        "def f(a):\n    return g(a)\n",
    )
    u, r = delegation_universe_for_callee("venddeco_deleg.f")
    assert u is None and r is not None and "decorated" in r.reason
