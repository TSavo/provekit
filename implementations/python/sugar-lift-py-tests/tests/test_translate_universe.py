from __future__ import annotations

import sys
import textwrap
import tempfile
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[4]
PY_TESTS_SRC = ROOT / "implementations/python/sugar-lift-py-tests/src"
PKG_SRC = ROOT / "implementations/python/sugar-lift-python-source/src"
for p in (str(PY_TESTS_SRC), str(PKG_SRC)):
    if p not in sys.path:
        sys.path.insert(0, p)

from sugar_lift_py_tests.layer2 import lift_file_layer2
from sugar_lift_py_tests.lsp import _lift_source
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
    source = textwrap.dedent(source)
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "test_mod.py"
        path.write_text(source, encoding="utf-8")
        return lift_file_layer2(source, str(path))


def _lift_source_from_disk(tmp_path: Path, name: str, source: str):
    source = textwrap.dedent(source)
    path = tmp_path / name
    path.write_text(source, encoding="utf-8")
    return _lift_source(str(path), source)


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


def test_universe_assertion_carries_source_warrant(vendor_path):
    vendor_path("venduniv_warrant", VENDOR_TRANSLATE)
    out = _lift(
        """
        import venduniv_warrant

        def test_urlsafe():
            assert venduniv_warrant.urlsafe("abc") == "abc"
        """
    )

    decl = next(
        d
        for d in out.decls
        if d.name.endswith("::assertion") and "venduniv_warrant.urlsafe" in d.name
    )
    roles = {warrant.get("role") for warrant in decl.source_warrants}
    assert {"python.test-fact", "python.translate-universe"} <= roles
    warrant = next(
        warrant
        for warrant in decl.source_warrants
        if warrant.get("role") == "python.translate-universe"
    )
    assert warrant["kind"] == "source-memento"
    assert warrant["role"] == "python.translate-universe"
    assert warrant["source_function_name"] == "urlsafe"
    assert warrant["file"].endswith("venduniv_warrant.py")
    assert warrant["source_cid"].startswith("blake3-512:")
    assert warrant["template_cid"].startswith("blake3-512:")
    assert warrant["span"]["start_line"] > 0
    assert "body_text" not in warrant
    assert "ast_template" not in warrant

    assert out.source_ledger["source_loci"] > 0
    assert out.source_ledger["source_warranted"] > 0
    assert out.source_ledger["source_refused"] >= 0
    assert out.source_ledger["source_inactive"] == 0
    assert out.source_ledger["source_refuted"] == 0
    assert "source_work" not in out.source_ledger
    assert out.source_ledger["unclassified_source"] == 0
    assert out.source_audits
    audit = next(
        audit
        for audit in out.source_audits
        if audit["contract"]["name"] == decl.name
        and audit["role"] == "python.translate-universe"
    )
    assert audit["kind"] == "source-audit"
    assert audit["language"] == "python"
    assert audit["source_memento"]["kind"] == "source-memento"
    assert "body_text" not in audit["source_memento"]
    assert "ast_template" not in audit["source_memento"]
    assert audit["totals"]["source_loci"] == len(audit["loci"])
    assert audit["totals"]["source_inactive"] == 0
    assert audit["totals"]["unclassified_source"] == 0
    assert {locus["status"] for locus in audit["loci"]} <= {"warranted", "support"}
    for locus in audit["loci"]:
        assert locus.get("ast_path", "").startswith("$.body"), locus
        assert locus.get("span", {}).get("start_line", 0) > 0, locus
        assert locus.get("line_range") == [
            locus["span"]["start_line"],
            locus["span"]["end_line"],
        ], locus
    assert any(
        locus["status"] == "warranted" and locus.get("ast_kind") == "Return"
        for locus in audit["loci"]
    ), audit
    assert any(
        locus["status"] == "warranted"
        and locus.get("ast_kind") == "Attribute"
        and locus.get("ast_path") == "$.body[1].value.func"
        for locus in audit["loci"]
    ), audit
    assert any(
        locus["status"] == "support" and locus.get("ast_kind") == "Expr"
        for locus in audit["loci"]
    ), audit


def test_lift_source_exposes_source_audit_countdown(vendor_path, tmp_path):
    vendor_path("venduniv_wire", VENDOR_TRANSLATE)
    lifted = _lift_source_from_disk(
        tmp_path,
        "test_mod.py",
        """
        import venduniv_wire

        def test_urlsafe():
            assert venduniv_wire.urlsafe("abc") == "abc"
        """,
    )

    source_ledger = lifted["sourceLedger"]
    assert source_ledger["source_loci"] > 0
    assert source_ledger["source_warranted"] > 0
    assert source_ledger["source_support"] > 0
    assert source_ledger["source_refused"] == 0
    assert source_ledger["source_inactive"] == 0
    assert source_ledger["unclassified_source"] == 0
    assert lifted["sourceMementos"]
    rpc_memento = next(
        m
        for m in lifted["sourceMementos"]
        if m.get("role") == "python.translate-universe"
    )
    assert rpc_memento["kind"] == "source-memento"
    assert rpc_memento["claimName"].endswith("::assertion")
    assert rpc_memento["contractName"].endswith("::assertion")
    assert rpc_memento["source_cid"].startswith("blake3-512:")
    assert "body_text" not in rpc_memento
    assert "ast_template" not in rpc_memento
    assert lifted["sourceAudits"]


def test_lift_source_dedupes_shared_fact_source_audits(vendor_path, tmp_path):
    vendor_path("vendrstrip_dedupe", VENDOR_RSTRIP)
    source = textwrap.dedent(
        """
        import vendrstrip_dedupe

        def test_token():
            assert vendrstrip_dedupe.b64e("abc") == "abc"
        """
    )
    source_path = tmp_path / "test_mod.py"
    source_path.write_text(source, encoding="utf-8")
    lifted = _lift_source(str(source_path), source)

    fact_audits = [
        audit
        for audit in lifted["sourceAudits"]
        if audit.get("role") == "python.test-fact"
    ]
    assert len(fact_audits) == 1
    assert fact_audits[0]["totals"]["source_warranted"] == 1


def test_lift_source_emits_package_unclassified_accounting(tmp_path, monkeypatch):
    pkg = tmp_path / "vendpkg_accounting"
    pkg.mkdir()
    (pkg / "__init__.py").write_text("", encoding="utf-8")
    (pkg / "encoding.py").write_text(
        textwrap.dedent(
            """
            def _inner(s):
                return s


            def b64e(s):
                return _inner(s).rstrip(b"=")
            """
        ),
        encoding="utf-8",
    )
    (pkg / "extra.py").write_text(
        textwrap.dedent(
            """
            def skipped(value):
                return value + "!"
            """
        ),
        encoding="utf-8",
    )
    monkeypatch.syspath_prepend(str(tmp_path))
    translate_universe_for_callee.cache_clear()

    lifted = _lift_source_from_disk(
        tmp_path,
        "test_mod.py",
        """
        import vendpkg_accounting.encoding as enc

        def test_token():
            assert enc.b64e("abc") == "abc"
        """,
    )

    package_audits = [
        audit
        for audit in lifted["sourceAudits"]
        if audit.get("role") == "python.package-source"
    ]
    assert len(package_audits) == 1
    audit = package_audits[0]
    assert audit["package"] == "vendpkg_accounting"
    assert audit["totals"]["source_loci"] == len(audit["loci"])
    assert audit["totals"]["source_warranted"] == 0
    assert audit["totals"]["source_support"] > 0
    assert audit["totals"]["source_refused"] == 0
    assert audit["totals"]["unclassified_source"] > 0
    assert audit["totals"]["unclassified_source"] < len(audit["loci"])
    assert lifted["sourceLedger"]["unclassified_source"] >= audit["totals"]["unclassified_source"]
    assert any(
        locus["status"] == "unclassified"
        and locus["file"].endswith("vendpkg_accounting/extra.py")
        and locus.get("ast_kind") == "Return"
        for locus in audit["loci"]
    ), audit


def test_lift_source_classifies_imports_as_package_support(tmp_path, monkeypatch):
    pkg = tmp_path / "vendpkg_import_support"
    pkg.mkdir()
    (pkg / "__init__.py").write_text("", encoding="utf-8")
    (pkg / "encoding.py").write_text(
        textwrap.dedent(
            """
            def b64e(s):
                return s.rstrip(b"=")
            """
        ),
        encoding="utf-8",
    )
    (pkg / "extra.py").write_text(
        textwrap.dedent(
            """
            import json
            from .encoding import b64e as imported_b64e

            def skipped(value):
                return imported_b64e(json.dumps(value))
            """
        ),
        encoding="utf-8",
    )
    monkeypatch.syspath_prepend(str(tmp_path))
    translate_universe_for_callee.cache_clear()

    lifted = _lift_source_from_disk(
        tmp_path,
        "test_mod.py",
        """
        import vendpkg_import_support.encoding as enc

        def test_token():
            assert enc.b64e("abc") == "abc"
        """,
    )

    audit = next(
        audit
        for audit in lifted["sourceAudits"]
        if audit.get("role") == "python.package-source"
    )
    import_loci = [
        locus
        for locus in audit["loci"]
        if locus["file"].endswith("vendpkg_import_support/extra.py")
        and locus.get("ast_kind") in {"Import", "ImportFrom", "alias"}
    ]
    assert import_loci
    assert {locus["status"] for locus in import_loci} == {"support"}
    assert audit["totals"]["source_support"] >= len(import_loci)
    assert lifted["sourceLedger"]["source_support"] >= len(import_loci)


def test_lift_source_classifies_package_signatures_and_docstrings_as_support(
    tmp_path, monkeypatch
):
    pkg = tmp_path / "vendpkg_decl_support"
    pkg.mkdir()
    (pkg / "__init__.py").write_text("", encoding="utf-8")
    (pkg / "encoding.py").write_text(
        textwrap.dedent(
            '''
            class Helper:
                """Declaration metadata, not a value constraint."""

            def b64e(s: str | bytes = b"abc") -> bytes:
                """Docstring metadata, not a value constraint."""
                return s.rstrip(b"=")

            def skipped(value: str) -> str:
                return value + "!"

            def typed_only(
                value: str | bytes,
            ) -> bytes:
                return value
            '''
        ),
        encoding="utf-8",
    )
    monkeypatch.syspath_prepend(str(tmp_path))
    translate_universe_for_callee.cache_clear()

    lifted = _lift_source_from_disk(
        tmp_path,
        "test_mod.py",
        """
        import vendpkg_decl_support.encoding as enc

        def test_token():
            assert enc.b64e(b"abc") == b"abc"
        """,
    )

    audit = next(
        audit
        for audit in lifted["sourceAudits"]
        if audit.get("role") == "python.package-source"
    )
    support_kinds = {
        locus.get("ast_kind")
        for locus in audit["loci"]
        if locus["file"].endswith("vendpkg_decl_support/encoding.py")
        and locus["status"] == "support"
    }
    assert {"ClassDef", "FunctionDef", "arg", "Expr", "Constant"} <= support_kinds
    assert not [
        locus
        for locus in audit["loci"]
        if locus["file"].endswith("vendpkg_decl_support/encoding.py")
        and locus.get("ast_kind") in {"ClassDef", "FunctionDef", "arg"}
        and locus["status"] == "unclassified"
    ]
    assert not [
        locus
        for locus in audit["loci"]
        if locus["file"].endswith("vendpkg_decl_support/encoding.py")
        and (
            ".annotation" in locus.get("ast_path", "")
            or ".returns" in locus.get("ast_path", "")
        )
        and locus["status"] == "unclassified"
    ]


def test_lift_source_classifies_static_assignments_as_warranted_compiler_facts(
    tmp_path, monkeypatch
):
    pkg = tmp_path / "vendpkg_static_warranted"
    pkg.mkdir()
    (pkg / "__init__.py").write_text("", encoding="utf-8")
    (pkg / "encoding.py").write_text(
        textwrap.dedent(
            '''
            import string
            import struct
            import typing as t

            _alphabet = f"{string.ascii_letters}{string.digits}-_=".encode("ascii")
            _int64_struct = struct.Struct(">Q")
            _int_to_bytes = _int64_struct.pack
            _bytes_to_int = t.cast("t.Callable[[bytes], tuple[int]]", _int64_struct.unpack)

            class Holder:
                default_key_derivation: str = "django-concat"
                typed_only: str

            def b64e(s):
                return s.rstrip(b"=")

            def skipped(value):
                local = value
                return local
            '''
        ),
        encoding="utf-8",
    )
    monkeypatch.syspath_prepend(str(tmp_path))
    translate_universe_for_callee.cache_clear()

    lifted = _lift_source_from_disk(
        tmp_path,
        "test_mod.py",
        """
        import vendpkg_static_warranted.encoding as enc

        def test_token():
            assert enc.b64e(b"abc") == b"abc"
        """,
    )

    audit = next(
        audit
        for audit in lifted["sourceAudits"]
        if audit.get("role") == "python.package-source"
    )
    static_loci = [
        locus
        for locus in audit["loci"]
        if locus["file"].endswith("vendpkg_static_warranted/encoding.py")
        and locus["line"] in {6, 7, 8, 9}
    ]
    assert static_loci
    assert {locus["status"] for locus in static_loci} == {"warranted"}
    assert all("static binding" in locus.get("reason", "") for locus in static_loci)
    class_static_loci = [
        locus
        for locus in audit["loci"]
        if locus["file"].endswith("vendpkg_static_warranted/encoding.py")
        and locus["line"] == 12
    ]
    assert class_static_loci
    assert not [locus for locus in class_static_loci if locus["status"] == "unclassified"]
    assert any(
        locus["status"] == "warranted" and locus.get("ast_kind") == "AnnAssign"
        for locus in class_static_loci
    ), class_static_loci
    assert any(
        locus["status"] == "support"
        and locus["file"].endswith("vendpkg_static_warranted/encoding.py")
        and locus["line"] == 13
        and locus.get("ast_kind") == "AnnAssign"
        for locus in audit["loci"]
    ), audit
    assert any(
        locus["status"] == "warranted"
        and locus["file"].endswith("vendpkg_static_warranted/encoding.py")
        and locus["line"] == 19
        and locus.get("ast_kind") == "Assign"
        and "SSA alias" in locus.get("reason", "")
        for locus in audit["loci"]
    ), audit


def test_lift_source_warrants_local_name_assignment_accounting(
    tmp_path, monkeypatch
):
    pkg = tmp_path / "vendpkg_local_name_warranted"
    pkg.mkdir()
    (pkg / "__init__.py").write_text("", encoding="utf-8")
    (pkg / "encoding.py").write_text(
        textwrap.dedent(
            '''
            def b64e(s):
                return s.rstrip(b"=")

            def skipped(value):
                alias = value
                computed = helper(value)
                flag = False
                missing = None
                empty = {}
                return alias
            '''
        ),
        encoding="utf-8",
    )
    monkeypatch.syspath_prepend(str(tmp_path))
    translate_universe_for_callee.cache_clear()

    lifted = _lift_source_from_disk(
        tmp_path,
        "test_mod.py",
        """
        import vendpkg_local_name_warranted.encoding as enc

        def test_token():
            assert enc.b64e(b"abc") == b"abc"
        """,
    )

    audit = next(
        audit
        for audit in lifted["sourceAudits"]
        if audit.get("role") == "python.package-source"
    )
    local_loci = [
        locus
        for locus in audit["loci"]
        if locus["file"].endswith("vendpkg_local_name_warranted/encoding.py")
    ]
    assert any(
        locus["status"] == "warranted"
        and locus["line"] == 6
        and locus.get("ast_kind") == "Assign"
        and "SSA alias" in locus.get("reason", "")
        for locus in local_loci
    ), local_loci
    assert not [
        locus
        for locus in local_loci
        if locus["line"] == 6
        and locus.get("ast_kind") == "Name"
        and locus["status"] == "unclassified"
    ], local_loci
    assert any(
        locus["status"] == "warranted"
        and locus["line"] == 7
        and locus.get("ast_kind") == "Name"
        and locus.get("ast_path", "").endswith(".targets[0]")
        for locus in local_loci
    ), local_loci
    assert any(
        locus["status"] == "unclassified"
        and locus["line"] == 7
        and locus.get("ast_kind") == "Call"
        for locus in local_loci
    ), local_loci
    for line, ast_kind in ((8, "Constant"), (9, "Constant"), (10, "Dict")):
        assert any(
            locus["status"] == "warranted"
            and locus["line"] == line
            and locus.get("ast_kind") == ast_kind
            and "local literal binding" in locus.get("reason", "")
            for locus in local_loci
        ), local_loci
    assert any(
        locus["status"] == "warranted"
        and locus["line"] == 8
        and locus.get("ast_kind") == "Assign"
        and "local literal binding" in locus.get("reason", "")
        for locus in local_loci
    ), local_loci


def test_lift_source_classifies_type_checking_blocks_as_support_or_inactive(
    tmp_path, monkeypatch
):
    pkg = tmp_path / "vendpkg_type_checking"
    pkg.mkdir()
    (pkg / "__init__.py").write_text("", encoding="utf-8")
    (pkg / "encoding.py").write_text(
        textwrap.dedent(
            '''
            import typing as t

            if t.TYPE_CHECKING:
                import typing_extensions as te
                _TSerialized = te.TypeVar("_TSerialized", bound=t.Union[str, bytes])
            else:
                _TSerialized = t.TypeVar("_TSerialized", bound=t.Union[str, bytes])

            def b64e(s):
                return s.rstrip(b"=")
            '''
        ),
        encoding="utf-8",
    )
    monkeypatch.syspath_prepend(str(tmp_path))
    translate_universe_for_callee.cache_clear()

    lifted = _lift_source_from_disk(
        tmp_path,
        "test_mod.py",
        """
        import vendpkg_type_checking.encoding as enc

        def test_token():
            assert enc.b64e(b"abc") == b"abc"
        """,
    )

    audit = next(
        audit
        for audit in lifted["sourceAudits"]
        if audit.get("role") == "python.package-source"
    )
    type_loci = [
        locus
        for locus in audit["loci"]
        if locus["file"].endswith("vendpkg_type_checking/encoding.py")
        and locus.get("ast_path", "").startswith("$.module.body[1]")
    ]
    assert type_loci
    assert {locus["status"] for locus in type_loci} <= {"support", "inactive"}
    assert not [
        locus
        for locus in type_loci
        if locus["status"] == "unclassified"
    ]
    assert any(locus["status"] == "inactive" for locus in type_loci), type_loci


def test_lift_source_classifies_overload_declarations_as_type_metadata(
    tmp_path, monkeypatch
):
    pkg = tmp_path / "vendpkg_overload_metadata"
    pkg.mkdir()
    (pkg / "__init__.py").write_text("", encoding="utf-8")
    (pkg / "encoding.py").write_text(
        textwrap.dedent(
            '''
            import typing as t

            class Codec:
                @t.overload
                def encode(self, value: str, fallback: None = None) -> str: ...

                @t.overload
                def encode(self, value: bytes, fallback: bytes | None = None) -> bytes: ...

                def encode(self, value, fallback=None):
                    return value

            def b64e(s):
                return s.rstrip(b"=")
            '''
        ),
        encoding="utf-8",
    )
    monkeypatch.syspath_prepend(str(tmp_path))
    translate_universe_for_callee.cache_clear()

    lifted = _lift_source_from_disk(
        tmp_path,
        "test_mod.py",
        """
        import vendpkg_overload_metadata.encoding as enc

        def test_token():
            assert enc.b64e(b"abc") == b"abc"
        """,
    )

    audit = next(
        audit
        for audit in lifted["sourceAudits"]
        if audit.get("role") == "python.package-source"
    )
    overload_loci = [
        locus
        for locus in audit["loci"]
        if locus["file"].endswith("vendpkg_overload_metadata/encoding.py")
        and (
            locus.get("ast_path", "").startswith("$.module.body[1].body[0]")
            or locus.get("ast_path", "").startswith("$.module.body[1].body[1]")
        )
    ]
    assert overload_loci
    assert not [
        locus for locus in overload_loci if locus["status"] == "unclassified"
    ], overload_loci
    assert any(
        locus["status"] == "support"
        and locus.get("ast_kind") == "Attribute"
        and "overload" in locus.get("reason", "")
        for locus in overload_loci
    ), overload_loci
    assert any(
        locus["status"] == "inactive"
        and locus.get("ast_kind") == "Expr"
        and "overload" in locus.get("reason", "")
        for locus in overload_loci
    ), overload_loci
    assert any(
        locus["file"].endswith("vendpkg_overload_metadata/encoding.py")
        and locus.get("ast_path") == "$.module.body[1].body[2].body[0]"
        and locus["status"] == "unclassified"
        for locus in audit["loci"]
    ), audit


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


VENDOR_WANT_BYTES_RSTRIP = '''
def _inner(s):
    return s


def want_bytes(s, encoding="utf-8", errors="strict"):
    if isinstance(s, str):
        s = s.encode(encoding, errors)

    return s


def b64e(s):
    s = want_bytes(s)
    return _inner(s).rstrip(b"=")
'''


VENDOR_LSTRIP = '''
def _pack(n):
    return b"\\x00\\x01"


def int_to_bytes(n):
    return _pack(n).lstrip(b"\\x00")
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


def test_rstrip_source_audit_warrants_return_shape(vendor_path):
    vendor_path("vendrstrip_audit", VENDOR_RSTRIP)
    out = _lift(
        """
        import vendrstrip_audit

        def test_token():
            assert vendrstrip_audit.b64e("abc") == "abc"
        """
    )

    audit = next(
        audit
        for audit in out.source_audits
        if audit["role"] == "python.translate-universe"
        and audit["universe_kind"] == "no-suffix-chars"
    )
    assert audit["totals"]["unclassified_source"] == 0
    assert any(
        locus["status"] == "support"
        and locus.get("ast_kind") == "Assign"
        and locus.get("ast_path") == "$.body[0]"
        for locus in audit["loci"]
    ), audit
    assert any(
        locus["status"] == "warranted"
        and locus.get("ast_kind") == "Return"
        and locus.get("ast_path") == "$.body[1]"
        for locus in audit["loci"]
    ), audit
    assert any(
        locus["status"] == "warranted"
        and locus.get("ast_kind") == "Attribute"
        and locus.get("ast_path") == "$.body[1].value.func"
        for locus in audit["loci"]
    ), audit


def test_rstrip_queues_want_bytes_identity_for_bytes_callsite(vendor_path):
    from sugar_lift_py_tests.ir import ctor, str_const
    from sugar_lift_py_tests.translate_universe import (
        bytes_identity_universe_for_callee,
        translate_universe_for_callee,
    )

    bytes_identity_universe_for_callee.cache_clear()
    translate_universe_for_callee.cache_clear()
    vendor_path("vendrstrip_want_bytes", VENDOR_WANT_BYTES_RSTRIP)
    out = _lift(
        """
        import vendrstrip_want_bytes

        def test_token():
            assert vendrstrip_want_bytes.b64e(b"abc") == b"abc"
        """
    )

    from sugar_lift_py_tests.layer2 import _iter_conjuncts

    bytes_term = ctor("python:bytes", [str_const("abc")])
    identity_atoms = []
    for d in out.decls:
        if not d.name.endswith("::assertion") or d.inv is None:
            continue
        for atom in _iter_conjuncts(d.inv):
            if getattr(atom, "name", None) != "=":
                continue
            args = getattr(atom, "args", ())
            if bytes_term not in args:
                continue
            if any(
                "callresult_vendrstrip_want_bytes_want_bytes_a1"
                in getattr(side, "name", "")
                for side in args
            ):
                identity_atoms.append(atom)
    assert identity_atoms, [d.name for d in out.decls]

    assertion = next(
        d
        for d in out.decls
        if d.name.endswith("::assertion")
        and "vendrstrip_want_bytes.b64e#euf#" in d.name
    )
    assert any(
        warrant.get("role") == "python.bytes-identity-universe"
        and warrant.get("source_function_name") == "want_bytes"
        for warrant in assertion.source_warrants
    )
    audit = next(
        audit
        for audit in out.source_audits
        if audit["role"] == "python.bytes-identity-universe"
    )
    assert audit["totals"]["unclassified_source"] == 0
    assert any(
        locus["status"] == "inactive" and locus.get("ast_kind") == "If"
        for locus in audit["loci"]
    ), audit
    assert any(
        locus["status"] == "warranted" and locus.get("ast_kind") == "Return"
        for locus in audit["loci"]
    ), audit


def test_lstrip_family_walks_no_prefix_chars(vendor_path):
    vendor_path("vendlstrip_ok", VENDOR_LSTRIP)
    universe, refusal = translate_universe_for_callee("vendlstrip_ok.int_to_bytes")
    assert refusal is None
    assert universe is not None
    assert universe.kind == "no-prefix-chars"
    assert universe.forbidden == "\x00"


def test_lstrip_emits_negated_prefix_conjunct(vendor_path):
    vendor_path("vendlstrip_l2", VENDOR_LSTRIP)
    out = _lift(
        """
        import vendlstrip_l2

        def test_int_to_bytes():
            assert vendlstrip_l2.int_to_bytes(1) == b"\\x01"
        """
    )

    prefix_atoms = []
    for d in out.decls:
        if d.name.endswith("::assertion") and d.inv is not None:
            for f in [d.inv] if not hasattr(d.inv, "operands") else list(d.inv.operands):
                if getattr(f, "kind", None) == "not":
                    inner = f.operands[0]
                    if getattr(inner, "name", None) == "prefix-of":
                        prefix_atoms.append(inner)
    assert len(prefix_atoms) == 1
    assert prefix_atoms[0].args[0].value == "\x00"


def test_lstrip_source_audit_warrants_return_shape(vendor_path):
    vendor_path("vendlstrip_audit", VENDOR_LSTRIP)
    out = _lift(
        """
        import vendlstrip_audit

        def test_int_to_bytes():
            assert vendlstrip_audit.int_to_bytes(1) == b"\\x01"
        """
    )

    audit = next(
        audit
        for audit in out.source_audits
        if audit["role"] == "python.translate-universe"
        and audit["universe_kind"] == "no-prefix-chars"
    )
    assert audit["totals"]["unclassified_source"] == 0
    assert any(
        locus["status"] == "warranted"
        and locus.get("ast_kind") == "Return"
        and locus.get("ast_path") == "$.body[0]"
        for locus in audit["loci"]
    ), audit
    assert any(
        locus["status"] == "warranted"
        and locus.get("ast_kind") == "Attribute"
        and locus.get("ast_path") == "$.body[0].value.func"
        for locus in audit["loci"]
    ), audit


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
    from sugar_lift_py_tests.ir import ctor, str_const
    from sugar_lift_py_tests.layer2 import _iter_conjuncts

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


def test_constant_universe_walks_constructor_bound_instance_method(vendor_path):
    constant_universe_for_callee_clear()
    vendor_path(
        "vendconst_method",
        '''
class Algo:
    def get_signature(self, key, value):
        return b""
''',
    )
    out = _lift(
        """
        import vendconst_method

        def test_signature():
            alg = vendconst_method.Algo()
            assert alg.get_signature(b"k", b"v") == b""
        """
    )

    assertion = next(
        (
            d
            for d in out.decls
            if d.name.endswith("::assertion")
            and "vendconst_method.Algo.get_signature" in d.name
        ),
        None,
    )
    assert assertion is not None, [d.name for d in out.decls]
    assert any(
        warrant.get("role") == "python.constant-universe"
        and warrant.get("source_function_name") == "Algo.get_signature"
        and warrant.get("file", "").endswith("vendconst_method.py")
        for warrant in assertion.source_warrants
    ), assertion.source_warrants

    from sugar_lift_py_tests.ir import ctor, str_const
    from sugar_lift_py_tests.layer2 import _iter_conjuncts

    constant_eqs = [
        atom
        for atom in _iter_conjuncts(assertion.inv)
        if getattr(atom, "name", None) == "="
        and any(
            getattr(side, "name", "") == "callval_get_signature_a3"
            for side in getattr(atom, "args", ())
        )
        and ctor("python:bytes", [str_const("")]) in getattr(atom, "args", ())
    ]
    assert constant_eqs

    audit = next(
        audit
        for audit in out.source_audits
        if audit["role"] == "python.constant-universe"
        and "vendconst_method.Algo.get_signature" in audit["contract"]["name"]
    )
    assert audit["totals"]["unclassified_source"] == 0
    assert any(
        locus["status"] == "warranted" and locus.get("ast_kind") == "Return"
        for locus in audit["loci"]
    ), audit


def test_instance_field_universe_maps_constructor_arg_to_getter(vendor_path):
    vendor_path(
        "vendinst_field",
        '''
class Box(Exception):
    def __init__(self, value):
        super().__init__(value)
        self.value = value

    def get(self):
        return self.value
''',
    )
    out = _lift(
        """
        import vendinst_field

        def test_box():
            box = vendinst_field.Box("raaaa")
            assert box.get() == "wrong"
        """
    )

    assertion = next(
        (
            d
            for d in out.decls
            if d.name.endswith("::assertion")
            and "vendinst_field.Box.get" in d.name
        ),
        None,
    )
    assert assertion is not None, [d.name for d in out.decls]

    from sugar_lift_py_tests.ir import str_const
    from sugar_lift_py_tests.layer2 import _iter_conjuncts

    field_eqs = [
        atom
        for atom in _iter_conjuncts(assertion.inv)
        if getattr(atom, "name", None) == "="
        and any(
            getattr(side, "name", "") == "callval_get_a1"
            for side in getattr(atom, "args", ())
        )
        and str_const("raaaa") in getattr(atom, "args", ())
    ]
    assert field_eqs

    field_warrants = [
        warrant
        for warrant in assertion.source_warrants
        if warrant.get("role") == "python.instance-field-universe"
    ]
    assert {
        warrant.get("source_function_name") for warrant in field_warrants
    } == {"Box.__init__", "Box.get"}

    audits = {
        audit["source_memento"].get("source_function_name"): audit
        for audit in out.source_audits
        if audit["role"] == "python.instance-field-universe"
        and "vendinst_field.Box.get" in audit["contract"]["name"]
    }
    assert set(audits) == {"Box.__init__", "Box.get"}
    for audit in audits.values():
        assert audit["totals"]["unclassified_source"] == 0
    assert any(
        locus["status"] == "warranted" and locus.get("ast_kind") == "Assign"
        for locus in audits["Box.__init__"]["loci"]
    ), audits["Box.__init__"]
    assert any(
        locus["status"] == "support" and locus.get("ast_kind") == "Expr"
        for locus in audits["Box.__init__"]["loci"]
    ), audits["Box.__init__"]
    assert any(
        locus["status"] == "warranted" and locus.get("ast_kind") == "Return"
        for locus in audits["Box.get"]["loci"]
    ), audits["Box.get"]


def test_instance_field_universe_maps_constructor_arg_to_attribute(vendor_path):
    vendor_path(
        "vendinst_attr",
        '''
class PayloadError(Exception):
    def __init__(self, message, payload=None):
        super().__init__(message)
        self.payload: object = payload
''',
    )
    out = _lift(
        """
        import vendinst_attr

        def test_payload():
            err = vendinst_attr.PayloadError("bad", payload=b"payload")
            assert err.payload == b"wrong"
        """
    )

    assertion = next(
        (
            d
            for d in out.decls
            if d.name.endswith("::assertion")
            and "vendinst_attr.PayloadError" in d.name
        ),
        None,
    )
    assert assertion is not None, [d.name for d in out.decls]

    from sugar_lift_py_tests.ir import ctor, str_const
    from sugar_lift_py_tests.layer2 import _iter_conjuncts

    payload_term = ctor("python:bytes", [str_const("payload")])
    field_eqs = [
        atom
        for atom in _iter_conjuncts(assertion.inv)
        if getattr(atom, "name", None) == "="
        and payload_term in getattr(atom, "args", ())
        and any(
            getattr(side, "name", "") == "err$0.payload"
            for side in getattr(atom, "args", ())
        )
    ]
    assert field_eqs

    field_warrants = [
        warrant
        for warrant in assertion.source_warrants
        if warrant.get("role") == "python.instance-field-universe"
    ]
    assert {
        warrant.get("source_function_name") for warrant in field_warrants
    } == {"PayloadError.__init__"}

    audit = next(
        audit
        for audit in out.source_audits
        if audit["role"] == "python.instance-field-universe"
        and audit["source_memento"].get("source_function_name")
        == "PayloadError.__init__"
    )
    assert audit["totals"]["unclassified_source"] == 0
    assert any(
        locus["status"] == "warranted" and locus.get("ast_kind") == "AnnAssign"
        for locus in audit["loci"]
    ), audit
    assert any(
        locus["status"] == "support" and locus.get("ast_kind") == "Expr"
        for locus in audit["loci"]
    ), audit


def test_instance_field_universe_maps_one_of_multiple_constructor_fields(vendor_path):
    vendor_path(
        "vendinst_multi_attr",
        '''
class HeaderError(Exception):
    def __init__(self, message, payload=None, header=None):
        super().__init__(message, payload)
        self.payload = payload
        self.header = header
''',
    )
    out = _lift(
        """
        import vendinst_multi_attr

        def test_header():
            err = vendinst_multi_attr.HeaderError("bad", payload=b"payload", header="h")
            assert err.header == "wrong"
        """
    )

    assertion = next(
        (
            d
            for d in out.decls
            if d.name.endswith("::assertion")
            and "vendinst_multi_attr.HeaderError" in d.name
        ),
        None,
    )
    assert assertion is not None, [d.name for d in out.decls]

    from sugar_lift_py_tests.ir import str_const
    from sugar_lift_py_tests.layer2 import _iter_conjuncts

    field_eqs = [
        atom
        for atom in _iter_conjuncts(assertion.inv)
        if getattr(atom, "name", None) == "="
        and str_const("h") in getattr(atom, "args", ())
        and any(
            getattr(side, "name", "") == "err$0.header"
            for side in getattr(atom, "args", ())
        )
    ]
    assert field_eqs

    audit = next(
        audit
        for audit in out.source_audits
        if audit["role"] == "python.instance-field-universe"
        and audit["source_memento"].get("source_function_name")
        == "HeaderError.__init__"
    )
    assert audit["totals"]["unclassified_source"] == 0
    assert any(
        locus["status"] == "warranted"
        and locus.get("ast_kind") == "Assign"
        and locus.get("line") == 6
        for locus in audit["loci"]
    ), audit


def test_instance_field_universe_maps_default_constructor_field(vendor_path):
    vendor_path(
        "vendinst_default_attr",
        '''
class HeaderError(Exception):
    def __init__(self, message, payload=None, header=None):
        super().__init__(message, payload)
        self.payload = payload
        self.header = header
''',
    )
    out = _lift(
        """
        import vendinst_default_attr

        def test_default_header():
            err = vendinst_default_attr.HeaderError("bad")
            assert err.header == None
        """
    )

    assertion = next(
        (
            d
            for d in out.decls
            if d.name.endswith("::assertion")
            and "vendinst_default_attr.HeaderError" in d.name
        ),
        None,
    )
    assert assertion is not None, [d.name for d in out.decls]

    from sugar_lift_py_tests.ir import ctor
    from sugar_lift_py_tests.layer2 import _iter_conjuncts

    none_term = ctor("None", [])
    field_eqs = [
        atom
        for atom in _iter_conjuncts(assertion.inv)
        if getattr(atom, "name", None) == "="
        and none_term in getattr(atom, "args", ())
        and any(
            getattr(side, "name", "") == "err$0.header"
            for side in getattr(atom, "args", ())
        )
    ]
    assert field_eqs

    audit = next(
        audit
        for audit in out.source_audits
        if audit["role"] == "python.instance-field-universe"
        and audit["source_memento"].get("source_function_name")
        == "HeaderError.__init__"
    )
    assert audit["totals"]["unclassified_source"] == 0
    assert any(
        locus["status"] == "warranted"
        and locus.get("ast_kind") == "Constant"
        and locus.get("line") == 3
        and locus.get("ast_path") == "$.args.defaults[1]"
        and "default constructor argument emitted" in locus.get("reason", "")
        for locus in audit["loci"]
    ), audit


def test_instance_field_universe_maps_conditional_default_constructor_field(vendor_path):
    vendor_path(
        "vendinst_conditional_default_attr",
        '''
class HMACAlgorithm:
    default_digest_method = object()

    def __init__(self, digest_method=None):
        if digest_method is None:
            digest_method = self.default_digest_method
        self.digest_method = digest_method
''',
    )
    out = _lift(
        """
        import vendinst_conditional_default_attr

        def test_default_digest_method():
            alg = vendinst_conditional_default_attr.HMACAlgorithm()
            assert alg.digest_method == alg.default_digest_method
        """
    )

    assertion = next(
        (
            d
            for d in out.decls
            if d.name.endswith("::assertion")
            and "vendinst_conditional_default_attr.HMACAlgorithm" in d.name
        ),
        None,
    )
    assert assertion is not None, [d.name for d in out.decls]

    field_warrants = [
        warrant
        for warrant in assertion.source_warrants
        if warrant.get("role") == "python.instance-field-universe"
    ]
    assert len(field_warrants) == 1
    assert field_warrants[0].get("source_function_name") == "HMACAlgorithm.__init__"
    assert field_warrants[0].get("constructor_default_param_names") == [
        "digest_method"
    ]
    assert (
        field_warrants[0].get("constructor_default_attr_name")
        == "default_digest_method"
    )

    audit = next(
        audit
        for audit in out.source_audits
        if audit["role"] == "python.instance-field-universe"
        and audit["source_memento"].get("source_function_name")
        == "HMACAlgorithm.__init__"
    )
    assert audit["source_memento"].get("constructor_default_attr_name") == (
        "default_digest_method"
    )
    assert audit["totals"]["unclassified_source"] == 0
    assert any(
        locus["status"] == "warranted" and locus.get("ast_kind") == "If"
        for locus in audit["loci"]
    ), audit
    assert any(
        locus["status"] == "warranted"
        and locus.get("ast_kind") == "Constant"
        and locus.get("ast_path") == "$.args.defaults[0]"
        and "default constructor argument emitted" in locus.get("reason", "")
        for locus in audit["loci"]
    ), audit
    assert any(
        locus["status"] == "warranted" and locus.get("ast_kind") == "Assign"
        for locus in audit["loci"]
    ), audit


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


def test_return_isinstance_emits_boolean_equivalence_and_source_accounting(
    vendor_path,
):
    from sugar_lift_py_tests.ir import _Ctor
    from sugar_lift_py_tests.translate_universe import (
        return_isinstance_universe_for_callee,
    )

    return_isinstance_universe_for_callee.cache_clear()
    vendor_path(
        "vendisinst_serializer",
        """
        class Serializer:
            def dumps(self, obj):
                return "x"


        def is_text_serializer(serializer):
            return isinstance(serializer.dumps({}), str)
        """,
    )
    out = _lift(
        """
        import vendisinst_serializer

        def test_text_serializer():
            serializer = vendisinst_serializer.Serializer()
            assert vendisinst_serializer.is_text_serializer(serializer) == True
        """
    )

    assertion = next(
        (
            d
            for d in out.decls
            if d.name.endswith("::assertion")
            and "vendisinst_serializer.is_text_serializer" in d.name
        ),
        None,
    )
    assert assertion is not None, [d.name for d in out.decls]
    assert any(
        warrant.get("role") == "python.return-isinstance-universe"
        and warrant.get("source_function_name") == "is_text_serializer"
        for warrant in assertion.source_warrants
    ), assertion.source_warrants

    def _walk_formula(formula):
        stack = [formula]
        while stack:
            current = stack.pop()
            yield current
            stack.extend(getattr(current, "operands", ()))

    isinstance_atoms = [
        atom
        for atom in _walk_formula(assertion.inv)
        if getattr(atom, "name", None) == "isinstance"
    ]
    assert isinstance_atoms, assertion.inv
    assert any(
        isinstance(atom.args[0], _Ctor)
        and atom.args[0].name == "callval_dumps_a2"
        and isinstance(atom.args[1], _Ctor)
        and atom.args[1].name == "pytype_str"
        for atom in isinstance_atoms
    ), isinstance_atoms
    assert any(
        getattr(formula, "kind", None) == "implies"
        for formula in _walk_formula(assertion.inv)
    ), assertion.inv

    audit = next(
        audit
        for audit in out.source_audits
        if audit["role"] == "python.return-isinstance-universe"
    )
    assert audit["totals"]["unclassified_source"] == 0
    assert any(
        locus["status"] == "warranted" and locus.get("ast_kind") == "Return"
        for locus in audit["loci"]
    ), audit
    assert any(
        locus["status"] == "warranted"
        and locus.get("ast_kind") == "Call"
        and "isinstance" in locus.get("reason", "")
        for locus in audit["loci"]
    ), audit


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
    # `x = x + 1; return x` is chain-SHAPED but the chain value is
    # computed: since the SSA-chain rung this refuses LOUDLY (it used to
    # be a silent two-statement non-candidate) — and it must never be
    # identity, which would forward the caller's x unincremented.
    vendor_path(
        "venddeleg_rebind", "def f(x):\n    x = x + 1\n    return x\n"
    )
    u, r = _deleg("venddeleg_rebind.f")
    assert u is None
    assert r is not None and "chain value is computed" in r.reason


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


def test_delegation_queues_delegate_dig_and_carries_source_warrants(vendor_path):
    from sugar_lift_py_tests.ir import str_const
    from sugar_lift_py_tests.translate_universe import (
        constant_universe_for_callee,
        delegation_universe_for_callee,
    )

    constant_universe_for_callee.cache_clear()
    delegation_universe_for_callee.cache_clear()
    vendor_path(
        "venddeleg_queue",
        """
        def g(seed):
            return "fixed"


        def f(seed):
            return g(seed)
        """,
    )
    out = _lift(
        """
        import venddeleg_queue

        def test_route():
            assert venddeleg_queue.f("raaaa") == "fixed"
        """
    )

    atoms = _delegation_eq_atoms(out, "callresult_venddeleg_queue_g_a1")
    assert atoms, [d.name for d in out.decls]

    from sugar_lift_py_tests.layer2 import _iter_conjuncts

    fixed_atoms = [
        a
        for d in out.decls
        if d.name.endswith("::assertion") and d.inv is not None
        for a in _iter_conjuncts(d.inv)
        if getattr(a, "name", None) == "="
        and str_const("fixed") in getattr(a, "args", ())
        and any(
            "callresult_venddeleg_queue_g_a1" in getattr(side, "name", "")
            for side in getattr(a, "args", ())
        )
    ]
    assert fixed_atoms, [d.name for d in out.decls]

    assertion = next(
        d
        for d in out.decls
        if d.name.endswith("::assertion")
        and "venddeleg_queue.f#euf#" in d.name
    )
    roles = {warrant.get("role") for warrant in assertion.source_warrants}
    assert {"python.delegation-universe", "python.constant-universe"} <= roles
    assert any(
        warrant.get("role") == "python.delegation-universe"
        and warrant.get("source_function_name") == "f"
        for warrant in assertion.source_warrants
    )
    assert any(
        warrant.get("role") == "python.constant-universe"
        and warrant.get("source_function_name") == "g"
        for warrant in assertion.source_warrants
    )

    audits = {
        audit["role"]: audit
        for audit in out.source_audits
        if audit["role"] in {"python.delegation-universe", "python.constant-universe"}
    }
    assert set(audits) == {"python.delegation-universe", "python.constant-universe"}
    assert audits["python.constant-universe"]["totals"]["unclassified_source"] == 0
    assert audits["python.delegation-universe"]["totals"]["unclassified_source"] == 0
    assert any(
        locus.get("ast_kind") == "Call" and locus["status"] == "support"
        for locus in audits["python.delegation-universe"]["loci"]
    ), audits["python.delegation-universe"]


def test_staticmethod_delegates_to_imported_stdlib_function(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        delegation_universe_for_callee,
    )

    delegation_universe_for_callee.cache_clear()
    vendor_path(
        "vendstdlib_deleg",
        '''
import json as _json


class Compact:
    @staticmethod
    def loads(payload):
        return _json.loads(payload)
''',
    )
    out = _lift(
        """
        import vendstdlib_deleg

        def test_loads():
            assert vendstdlib_deleg.Compact.loads('{"ok": true}') == {"ok": True}
        """
    )

    assertion = next(
        (
            d
            for d in out.decls
            if d.name.endswith("::assertion")
            and "vendstdlib_deleg.Compact.loads" in d.name
        ),
        None,
    )
    assert assertion is not None, [d.name for d in out.decls]

    from sugar_lift_py_tests.layer2 import _iter_conjuncts

    imported_delegate_eqs = [
        atom
        for atom in _iter_conjuncts(assertion.inv)
        if getattr(atom, "name", None) == "="
        and any(
            getattr(side, "name", "") == "callresult_json_loads_a1"
            for side in getattr(atom, "args", ())
        )
    ]
    assert imported_delegate_eqs
    assert any(
        warrant.get("role") == "python.delegation-universe"
        and warrant.get("source_function_name") == "Compact.loads"
        and warrant.get("universe_kind") == "delegation-stdlib"
        for warrant in assertion.source_warrants
    ), assertion.source_warrants

    audit = next(
        audit
        for audit in out.source_audits
        if audit["role"] == "python.delegation-universe"
        and audit["universe_kind"] == "delegation-stdlib"
    )
    assert audit["totals"]["unclassified_source"] == 0
    assert any(
        locus["status"] == "warranted" and locus.get("ast_kind") == "Return"
        for locus in audit["loci"]
    ), audit
    assert any(
        locus["status"] == "warranted" and locus.get("ast_kind") == "Call"
        for locus in audit["loci"]
    ), audit


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


# ---------------------------------------------------------------------------
# assert-as-guard + the None arm (census: non-return:Assert 179k, Pass 17k,
# empty 7k, bare-return 1.7k). An `assert P` is a guard with polarity
# flipped — it raises exactly when P is false — so it contributes P itself
# as the clause (the negated comparison of NOT P). A body that is, after
# the guard prefix, empty / pass / bare return falls off the end, and
# CPython defines falling off the end as None, unconditionally. Effect
# tails stay non-candidates: their contract is the effect, not a vacuous
# value claim.
# ---------------------------------------------------------------------------


def test_assert_prefix_contributes_guard_clause(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        guard_universe_for_callee,
    )

    guard_universe_for_callee.cache_clear()
    vendor_path(
        "vendassert_guard",
        "def f(x):\n    assert x > 0\n    return x\n",
    )
    guards, refusal = guard_universe_for_callee("vendassert_guard.f")
    assert refusal is None and guards is not None
    (clause,) = guards.clauses
    # assert x > 0 raises when x <= 0: the clause is the negation
    assert (clause.param_name, clause.op, clause.literal) == ("x", "≤", 0)


def test_assert_and_if_raise_clauses_compose(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        guard_universe_for_callee,
    )

    guard_universe_for_callee.cache_clear()
    vendor_path(
        "vendassert_both",
        "def f(x, y):\n"
        "    assert x > 0\n"
        "    if y < 2:\n"
        "        raise ValueError(y)\n"
        "    return x\n",
    )
    guards, refusal = guard_universe_for_callee("vendassert_both.f")
    assert refusal is None and guards is not None
    ops = [(c.param_name, c.op, c.literal) for c in guards.clauses]
    assert ops == [("x", "≤", 0), ("y", "<", 2)]


def test_assert_vendor_vector_firing_refuses(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        guard_universe_for_callee,
    )

    guard_universe_for_callee.cache_clear()
    vendor_path(
        "vendassert_fire",
        "def f(x):\n    assert x > 0\n    return x\n",
    )
    vendor_path(
        "test_vendassert_fire",
        "import vendassert_fire\n\n"
        "def test_bad():\n    assert vendassert_fire.f(-3) == -3\n",
    )
    guards, refusal = guard_universe_for_callee("vendassert_fire.f")
    assert guards is None and refusal is not None
    assert "sample-gate" in refusal.reason


def test_assert_only_body_swears_none(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        constant_universe_for_callee,
    )

    constant_universe_for_callee.cache_clear()
    vendor_path(
        "vendnone_assert", "def check(x):\n    assert x > 0\n"
    )
    u, r = constant_universe_for_callee("vendnone_assert.check")
    assert r is None and u is not None
    assert (u.value, u.value_kind) == (None, "none")


def test_pass_body_swears_none(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        constant_universe_for_callee,
    )

    constant_universe_for_callee.cache_clear()
    vendor_path("vendnone_pass", "def noop(x):\n    pass\n")
    u, r = constant_universe_for_callee("vendnone_pass.noop")
    assert r is None and (u.value, u.value_kind) == (None, "none")


def test_docstring_only_body_swears_none(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        constant_universe_for_callee,
    )

    constant_universe_for_callee.cache_clear()
    vendor_path(
        "vendnone_doc", 'def noop(x):\n    """does nothing"""\n'
    )
    u, r = constant_universe_for_callee("vendnone_doc.noop")
    assert r is None and (u.value, u.value_kind) == (None, "none")


def test_bare_return_swears_none(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        constant_universe_for_callee,
    )

    constant_universe_for_callee.cache_clear()
    vendor_path(
        "vendnone_ret",
        "def stop(x):\n    if x < 0:\n        raise ValueError(x)\n    return\n",
    )
    u, r = constant_universe_for_callee("vendnone_ret.stop")
    assert r is None and (u.value, u.value_kind) == (None, "none")


def test_effect_tail_is_not_a_none_candidate(vendor_path):
    # `x.fire()` returns None too — but its contract is the EFFECT; a
    # vacuous value claim would dress a side effect as a proven function.
    from sugar_lift_py_tests.translate_universe import (
        constant_universe_for_callee,
    )

    constant_universe_for_callee.cache_clear()
    vendor_path("vendnone_effect", "def f(x):\n    x.fire()\n")
    u, r = constant_universe_for_callee("vendnone_effect.f")
    assert u is None and r is None


def test_generator_is_not_a_none_candidate(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        constant_universe_for_callee,
    )

    constant_universe_for_callee.cache_clear()
    vendor_path("vendnone_gen", "def f(x):\n    yield x\n")
    u, r = constant_universe_for_callee("vendnone_gen.f")
    assert u is None and r is None


def test_walrus_assert_refuses_everywhere(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        constant_universe_for_callee,
        delegation_universe_for_callee,
        guard_universe_for_callee,
    )

    vendor_path(
        "vendassert_walrus",
        "def f(x):\n    assert (x := x + 1) > 0\n    return x\n",
    )
    guard_universe_for_callee.cache_clear()
    g, gr = guard_universe_for_callee("vendassert_walrus.f")
    assert g is None and gr is not None and "walrus" in gr.reason
    delegation_universe_for_callee.cache_clear()
    d, dr = delegation_universe_for_callee("vendassert_walrus.f")
    assert d is None and dr is not None and "walrus" in dr.reason
    constant_universe_for_callee.cache_clear()
    c, cr = constant_universe_for_callee("vendassert_walrus.f")
    # the tainted strip refuses BEFORE the shape is even considered: a
    # rebound environment poisons every downstream read uniformly
    assert c is None and cr is not None and "walrus" in cr.reason


def test_assert_prefix_identity_composes(vendor_path):
    # assert strips for the delegation family too: the identity universe
    # and the assert clause ride the same body.
    from sugar_lift_py_tests.translate_universe import (
        delegation_universe_for_callee,
    )

    delegation_universe_for_callee.cache_clear()
    vendor_path(
        "vendassert_ident",
        "def f(x):\n    assert x > 0\n    return x\n",
    )
    u, r = delegation_universe_for_callee("vendassert_ident.f")
    assert r is None and u is not None and u.kind == "identity"


def test_assert_guard_and_none_emit_together(vendor_path):
    # e2e through layer2: the consumer swears check(-5) == 3. The body
    # swears TWO universes that each refute it — the None equality (the
    # body falls off the end: the value is None, not 3) and the assert
    # clause instantiated at -5 (not(-5 <= 0) is false: you swore a
    # return from a call the vendor's own source says raises). Both
    # conjuncts must land in the same inv as the claim. (A consumer
    # writing `== None` takes the dedicated None-check encoding, which
    # carries no extractable call subject — universes inject on the
    # standard equality path.)
    from sugar_lift_py_tests.translate_universe import (
        constant_universe_for_callee,
        guard_universe_for_callee,
    )
    from sugar_lift_py_tests.layer2 import _iter_conjuncts

    constant_universe_for_callee.cache_clear()
    guard_universe_for_callee.cache_clear()
    vendor_path(
        "vendassert_l2", "def check(x):\n    assert x > 0\n"
    )
    out = _lift(
        """
        import vendassert_l2

        def test_neg():
            assert vendassert_l2.check(-5) == 3
        """
    )
    none_eqs, guard_negs = [], []
    for d in out.decls:
        if d.inv is None:
            continue
        # raw operand walk: _iter_conjuncts yields only ATOMIC leaves, so
        # the guard's not(...) conjunct is invisible to it by design
        for a in getattr(d.inv, "operands", (d.inv,)):
            if getattr(a, "name", None) == "=" and any(
                getattr(s, "name", None) == "None"
                for s in getattr(a, "args", ())
            ):
                none_eqs.append(a)
            if getattr(a, "kind", None) == "not":
                guard_negs.append(a)
    assert none_eqs, [d.name for d in out.decls]
    assert guard_negs, [d.name for d in out.decls]


# ---------------------------------------------------------------------------
# method delegation (census return-method-call, 113k bodies):
# `return <param|literal>.method(<params|literals>)` swears
# eq(subject, callval_<method>(recv, args...)). No body backs a method
# delegate — the receiver's type is not static — so the license is
# narrower than function delegation: nondeterminism-marker methods refuse
# by name, and the EMITTER bridges only GROUND instantiations (every
# mapped term concrete at the callsite).
# ---------------------------------------------------------------------------


def test_method_delegation_walks(vendor_path):
    vendor_path(
        "vendmdeleg_ok", "def up(s):\n    return s.upper()\n"
    )
    u, r = _deleg("vendmdeleg_ok.up")
    assert r is None and u is not None
    assert u.kind == "delegation-method"
    assert u.delegate == "upper"
    assert u.args == (("param", 0),)


def test_method_delegation_literal_receiver(vendor_path):
    vendor_path(
        "vendmdeleg_join", "def j(xs):\n    return ','.join(xs)\n"
    )
    u, r = _deleg("vendmdeleg_join.j")
    assert r is None and u is not None
    assert u.delegate == "join"
    assert u.args == (("lit", ",", "str"), ("param", 0))


def test_nondet_method_refuses(vendor_path):
    vendor_path(
        "vendmdeleg_nd", "def f(x):\n    return x.random()\n"
    )
    u, r = _deleg("vendmdeleg_nd.f")
    assert u is None and r is not None and "nondeterminism marker" in r.reason


def test_method_keyword_refuses(vendor_path):
    vendor_path(
        "vendmdeleg_kw", "def f(x):\n    return x.get('a', default=1)\n"
    )
    u, r = _deleg("vendmdeleg_kw.f")
    assert u is None and r is not None and "keyword" in r.reason


def test_computed_receiver_is_not_a_candidate(vendor_path):
    vendor_path(
        "vendmdeleg_deep", "def f(x):\n    return x.attr.m()\n"
    )
    u, r = _deleg("vendmdeleg_deep.f")
    assert u is None and r is None  # other families' shape


def test_method_arg_not_param_refuses(vendor_path):
    vendor_path(
        "vendmdeleg_comp", "def f(x):\n    return x.count(x + 1)\n"
    )
    u, r = _deleg("vendmdeleg_comp.f")
    assert u is None and r is not None
    assert "receiver/argument" in r.reason


def test_method_delegation_emits_ground_equality(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        delegation_universe_for_callee,
    )

    delegation_universe_for_callee.cache_clear()
    vendor_path("vendmdeleg_l2", "def up(s):\n    return s.upper()\n")
    out = _lift(
        """
        import vendmdeleg_l2

        def test_up():
            assert vendmdeleg_l2.up("abc") == "x"
        """
    )
    atoms = _delegation_eq_atoms(out, "callval_upper_a1")
    assert atoms, [d.name for d in out.decls]


def test_method_delegation_skips_symbolic_instantiation():
    # the ground-only gate, exercised at the emission seam directly: a
    # symbolic receiver term (a _Var) must produce NO delegate equality.
    import sugar_lift_py_tests.layer2 as l2
    from sugar_lift_py_tests.ir import ctor as mk_ctor, make_var
    from sugar_lift_py_tests.translate_universe import (
        delegation_universe_for_callee,
        DelegationUniverse,
    )

    u = DelegationUniverse(
        kind="delegation-method",
        module="m",
        qualname="m.up",
        source_path="m.py",
        lineno=1,
        delegate="upper",
        args=(("param", 0),),
    )
    call_args = [make_var("symbolic_receiver")]
    mapped = l2._mapped_delegate_args(u.args, call_args)
    term = mk_ctor(l2._callval_head("upper", len(mapped)), mapped)
    assert not l2._euf_args_all_concrete(term)


# ---------------------------------------------------------------------------
# branch-literal disjunction (census non-return:If, 75k bodies): every
# Return returns a same-kind literal and the body cannot fall off the end
# (terminality: Return | Raise | If with both arms terminal, recursively),
# so output ∈ {walked literals} — sound with NO condition evaluation: any
# execution that returns at all returns SOME Return node's value. Mixed
# kinds refuse by name (the #2103 cross-sort hazard: one subject, two
# theories).
# ---------------------------------------------------------------------------


def _branch(callee):
    from sugar_lift_py_tests.translate_universe import (
        branch_literal_universe_for_callee,
    )

    branch_literal_universe_for_callee.cache_clear()
    return branch_literal_universe_for_callee(callee)


def test_branch_literal_if_else_walks(vendor_path):
    vendor_path(
        "vendbranch_ok",
        'def pick(x):\n'
        '    if x:\n'
        '        return "a"\n'
        '    else:\n'
        '        return "b"\n',
    )
    u, r = _branch("vendbranch_ok.pick")
    assert r is None and u is not None
    assert u.values == ("a", "b") and u.value_kind == "str"


def test_branch_literal_elif_chain_walks(vendor_path):
    vendor_path(
        "vendbranch_chain",
        "def grade(x):\n"
        "    if x > 90:\n"
        "        return 1\n"
        "    elif x > 50:\n"
        "        return 2\n"
        "    else:\n"
        "        return 3\n",
    )
    u, r = _branch("vendbranch_chain.grade")
    assert r is None and u is not None
    assert u.values == (1, 2, 3) and u.value_kind == "int"


def test_branch_literal_tail_return_walks(vendor_path):
    # if-without-else followed by a tail return: terminal via the LAST
    # statement, the if's returns still join the disjunction
    vendor_path(
        "vendbranch_tail",
        'def flag(x):\n'
        '    if x:\n'
        '        return "yes"\n'
        '    return "no"\n',
    )
    u, r = _branch("vendbranch_tail.flag")
    assert r is None and u is not None
    assert u.values == ("yes", "no")


def test_branch_literal_dedupes_repeated_values(vendor_path):
    vendor_path(
        "vendbranch_dup",
        'def same(x):\n'
        '    if x:\n'
        '        return "a"\n'
        '    return "a"\n',
    )
    u, r = _branch("vendbranch_dup.same")
    assert r is None and u is not None and u.values == ("a",)


def test_branch_literal_mixed_kinds_refuse(vendor_path):
    vendor_path(
        "vendbranch_mixed",
        'def odd(x):\n'
        '    if x:\n'
        '        return "a"\n'
        '    return 1\n',
    )
    u, r = _branch("vendbranch_mixed.odd")
    assert u is None and r is not None and "cross-sort" in r.reason


def test_branch_literal_bare_return_refuses(vendor_path):
    vendor_path(
        "vendbranch_bare",
        'def odd(x):\n'
        '    if x:\n'
        '        return "a"\n'
        '    return\n',
    )
    u, r = _branch("vendbranch_bare.odd")
    assert u is None and r is not None and "bare" in r.reason


def test_branch_literal_computed_branch_not_candidate(vendor_path):
    vendor_path(
        "vendbranch_comp",
        'def f(x):\n'
        '    if x:\n'
        '        return "a"\n'
        '    return x\n',
    )
    u, r = _branch("vendbranch_comp.f")
    assert u is None and r is None


def test_branch_literal_loop_tail_not_terminal(vendor_path):
    # a while-tail can fall off the end -> implicit None would join the
    # set; the terminality check excludes it (named residual)
    vendor_path(
        "vendbranch_loop",
        'def f(x):\n'
        '    while x:\n'
        '        return "a"\n',
    )
    u, r = _branch("vendbranch_loop.f")
    assert u is None and r is None


def test_branch_literal_single_return_is_constant_territory(vendor_path):
    vendor_path(
        "vendbranch_single", 'def f(x):\n    return "a"\n'
    )
    u, r = _branch("vendbranch_single.f")
    assert u is None and r is None


def test_branch_literal_generator_excluded(vendor_path):
    vendor_path(
        "vendbranch_gen",
        'def f(x):\n'
        '    if x:\n'
        '        return "a"\n'
        '    yield "b"\n',
    )
    u, r = _branch("vendbranch_gen.f")
    assert u is None and r is None


def test_branch_literal_walrus_guard_refuses(vendor_path):
    vendor_path(
        "vendbranch_walrus",
        'def f(x):\n'
        '    if (x := x + 1) > 99:\n'
        '        raise ValueError(x)\n'
        '    if x:\n'
        '        return "a"\n'
        '    return "b"\n',
    )
    u, r = _branch("vendbranch_walrus.f")
    assert u is None and r is not None and "walrus" in r.reason


def test_branch_literal_sample_gate_refuses_outside_value(vendor_path):
    vendor_path(
        "vendbranch_gate",
        'def pick(x):\n'
        '    if x:\n'
        '        return "a"\n'
        '    return "b"\n',
    )
    vendor_path(
        "test_vendbranch_gate",
        'import vendbranch_gate\n\n'
        'def test_p():\n'
        '    assert vendbranch_gate.pick(1) == "z"\n',
    )
    u, r = _branch("vendbranch_gate.pick")
    assert u is None and r is not None and "sample-gate" in r.reason


def test_branch_literal_sample_gate_licenses_inside_value(vendor_path):
    vendor_path(
        "vendbranch_gate2",
        'def pick(x):\n'
        '    if x:\n'
        '        return "a"\n'
        '    return "b"\n',
    )
    vendor_path(
        "test_vendbranch_gate2",
        'import vendbranch_gate2\n\n'
        'def test_p():\n'
        '    assert vendbranch_gate2.pick(1) == "a"\n',
    )
    u, r = _branch("vendbranch_gate2.pick")
    assert r is None and u is not None
    assert u.vendor_vectors_checked >= 1


def test_branch_literal_emits_disjunction(vendor_path):
    # e2e: the consumer swears pick(1) == "c" — outside the walked set.
    # The inv must carry the or_ disjunction; conjoined with the claim it
    # is UNSAT and the wrong value refutes.
    from sugar_lift_py_tests.translate_universe import (
        branch_literal_universe_for_callee,
    )

    branch_literal_universe_for_callee.cache_clear()
    vendor_path(
        "vendbranch_l2",
        'def pick(x):\n'
        '    if x:\n'
        '        return "a"\n'
        '    return "b"\n',
    )
    out = _lift(
        """
        import vendbranch_l2

        def test_pick():
            assert vendbranch_l2.pick(1) == "c"
        """
    )
    ors = []
    for d in out.decls:
        if d.inv is None:
            continue
        for a in getattr(d.inv, "operands", (d.inv,)):
            if getattr(a, "kind", None) == "or":
                ors.append(a)
    assert ors, [d.name for d in out.decls]
    # both walked literals appear as equality disjuncts
    texts = repr(ors)
    assert "'a'" in texts or '"a"' in texts or "value='a'" in texts


def test_ifexp_return_walks_as_branch_literal(vendor_path):
    # the expression form of the branch shape: one return, two leaves
    vendor_path(
        "vendbranch_ifexp",
        'def pick(x):\n    return "a" if x else "b"\n',
    )
    u, r = _branch("vendbranch_ifexp.pick")
    assert r is None and u is not None
    assert u.values == ("a", "b") and u.value_kind == "str"


def test_nested_ifexp_collects_all_leaves(vendor_path):
    vendor_path(
        "vendbranch_ifexp2",
        'def pick(x):\n    return 1 if x > 9 else (2 if x > 5 else 3)\n',
    )
    u, r = _branch("vendbranch_ifexp2.pick")
    assert r is None and u is not None
    assert u.values == (1, 2, 3)


def test_ifexp_and_statement_returns_compose(vendor_path):
    vendor_path(
        "vendbranch_ifexp3",
        'def pick(x):\n'
        '    if x < 0:\n'
        '        return "neg"\n'
        '    return "big" if x > 9 else "small"\n',
    )
    u, r = _branch("vendbranch_ifexp3.pick")
    assert r is None and u is not None
    assert u.values == ("neg", "big", "small")


def test_ifexp_computed_leaf_not_candidate(vendor_path):
    vendor_path(
        "vendbranch_ifexp4",
        'def pick(x):\n    return "a" if x else x\n',
    )
    u, r = _branch("vendbranch_ifexp4.pick")
    assert u is None and r is None


def test_ifexp_mixed_kinds_refuse(vendor_path):
    vendor_path(
        "vendbranch_ifexp5",
        'def pick(x):\n    return "a" if x else 1\n',
    )
    u, r = _branch("vendbranch_ifexp5.pick")
    assert u is None and r is not None and "cross-sort" in r.reason


def test_walrus_in_ifexp_condition_is_harmless(vendor_path):
    # a rebinding in the CONDITION has nothing downstream of itself to
    # poison: the value is one of the literal leaves either way
    vendor_path(
        "vendbranch_ifexp6",
        'def pick(x):\n    return "a" if (x := x + 1) > 5 else "b"\n',
    )
    u, r = _branch("vendbranch_ifexp6.pick")
    assert r is None and u is not None and u.values == ("a", "b")


# ---------------------------------------------------------------------------
# collection-literal constant arm (census return-collection, 54k bodies):
# a literal tuple/list/dict/set of literal leaves is ONE fixed value; the
# canonical content string is built in exactly one place
# (collection_literal_canonical) and shared with the consumer-side term
# translator, so the universe equality and consumer claims are
# byte-identical by construction. repr-based leaves make 1 and True
# distinct (false-refusal direction only, never a wrong discharge).
# ---------------------------------------------------------------------------


def _const(callee):
    from sugar_lift_py_tests.translate_universe import (
        constant_universe_for_callee,
    )

    constant_universe_for_callee.cache_clear()
    return constant_universe_for_callee(callee)


def test_tuple_return_pins_canonical(vendor_path):
    vendor_path("vendcoll_t", "def pair():\n    return (1, 2)\n")
    u, r = _const("vendcoll_t.pair")
    assert r is None and u is not None
    assert (u.value, u.value_kind) == ("tuple:[1, 2]", "collection")


def test_list_and_tuple_canonicals_are_distinct(vendor_path):
    vendor_path(
        "vendcoll_lt",
        "def t():\n    return (1, 2)\n\ndef l():\n    return [1, 2]\n",
    )
    ut, _ = _const("vendcoll_lt.t")
    ul, _ = _const("vendcoll_lt.l")
    assert ut.value != ul.value
    assert ul.value == "list:[1, 2]"


def test_dict_return_pins_canonical(vendor_path):
    vendor_path(
        "vendcoll_d", "def conf():\n    return {'b': 2, 'a': 1}\n"
    )
    u, r = _const("vendcoll_d.conf")
    assert r is None and u is not None
    # sorted by key repr: insertion order does not leak into the canonical
    assert u.value == "dict:" + repr({"a": 1, "b": 2})


def test_set_return_dedupes_and_sorts(vendor_path):
    vendor_path(
        "vendcoll_s", "def tags():\n    return {'b', 'a', 'b'}\n"
    )
    u, r = _const("vendcoll_s.tags")
    assert r is None and u is not None
    assert u.value == "set:['a', 'b']"


def test_computed_element_not_a_candidate(vendor_path):
    vendor_path(
        "vendcoll_comp", "def f(x):\n    return (1, x)\n"
    )
    u, r = _const("vendcoll_comp.f")
    assert u is None and r is None


def test_nested_collection_not_a_candidate(vendor_path):
    vendor_path(
        "vendcoll_nest", "def f():\n    return ((1, 2), 3)\n"
    )
    u, r = _const("vendcoll_nest.f")
    assert u is None and r is None


def test_collection_universe_contradicts_wrong_tuple(vendor_path):
    # bad twin e2e: vendor returns (1, 2); the consumer swears (1, 3).
    # Both equalities land in one inv over DISTINCT opaque constants —
    # UNSAT, the wrong tuple refutes. This also proves the consumer side
    # now LIFTS tuple-literal equality claims (it loud-refused before).
    from sugar_lift_py_tests.translate_universe import (
        constant_universe_for_callee,
    )
    from sugar_lift_py_tests.layer2 import _iter_conjuncts

    constant_universe_for_callee.cache_clear()
    vendor_path("vendcoll_l2", "def pair():\n    return (1, 2)\n")
    out = _lift(
        """
        import vendcoll_l2

        def test_pair():
            assert vendcoll_l2.pair() == (1, 3)
        """
    )
    consts = []
    for d in out.decls:
        if d.inv is None:
            continue
        for a in _iter_conjuncts(d.inv):
            if getattr(a, "name", None) != "=":
                continue
            for side in getattr(a, "args", ()):
                v = getattr(side, "value", None)
                if isinstance(v, str) and v.startswith("tuple:"):
                    consts.append(v)
    assert "tuple:[1, 2]" in consts, consts  # the vendor's universe
    assert "tuple:[1, 3]" in consts, consts  # the consumer's claim


# ---------------------------------------------------------------------------
# SSA-chain delegation (census return-fn-call, 53k bodies): leading simple
# assigns are a substitution environment — `x = a; return g(x)` forwards
# `a` exactly as `return g(a)` does. Linear and control-flow-free, so
# left-to-right resolution IS the SSA; rebound names shadow correctly.
# ---------------------------------------------------------------------------

VENDOR_CHAIN = '''
def g(a, b):
    return a


def f(a):
    x = a
    return g(x, 5)


def hop(a):
    x = a
    y = x
    return g(y, 5)


def shadow(a):
    a = 7
    return g(a, a)


def ident_chain(a):
    x = a
    return x


def const_chain(a):
    x = 5
    return x


def method_chain(s):
    x = s
    return x.upper()
'''


def test_chain_assign_feeds_delegation(vendor_path):
    vendor_path("vendchain_ok", VENDOR_CHAIN)
    u, r = _deleg("vendchain_ok.f")
    assert r is None and u is not None
    assert u.kind == "delegation"
    assert u.args == (("param", 0), ("lit", 5, "int"))


def test_chain_assign_source_audit_warrants_ssa_feed(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        delegation_universe_for_callee,
    )

    delegation_universe_for_callee.cache_clear()
    vendor_path("vendchain_audit", VENDOR_CHAIN)
    out = _lift(
        """
        import vendchain_audit

        def test_chain():
            assert vendchain_audit.f(7) == 7
        """
    )

    audit = next(
        audit
        for audit in out.source_audits
        if audit["role"] == "python.delegation-universe"
        and audit["universe_kind"] == "delegation"
    )
    assert audit["totals"]["unclassified_source"] == 0
    assert any(
        locus["status"] == "warranted"
        and locus.get("ast_kind") == "Assign"
        and locus.get("ast_path") == "$.body[0]"
        for locus in audit["loci"]
    ), audit
    assert any(
        locus["status"] == "warranted"
        and locus.get("ast_kind") == "Return"
        and locus.get("ast_path") == "$.body[1]"
        for locus in audit["loci"]
    ), audit


def test_chain_resolves_through_hops(vendor_path):
    vendor_path("vendchain_hop", VENDOR_CHAIN)
    u, r = _deleg("vendchain_hop.hop")
    assert r is None and u.args == (("param", 0), ("lit", 5, "int"))


def test_chain_shadowing_param_rebinds(vendor_path):
    # `a = 7; return g(a, a)`: the runtime forwards 7 regardless of the
    # caller's a — the spec must be the literal, never the param
    vendor_path("vendchain_shadow", VENDOR_CHAIN)
    u, r = _deleg("vendchain_shadow.shadow")
    assert r is None and u.args == (("lit", 7, "int"), ("lit", 7, "int"))


def test_chain_identity(vendor_path):
    vendor_path("vendchain_id", VENDOR_CHAIN)
    u, r = _deleg("vendchain_id.ident_chain")
    assert r is None and u.kind == "identity" and u.param_index == 0


def test_chain_constant(vendor_path):
    vendor_path("vendchain_const", VENDOR_CHAIN)
    u, r = _deleg("vendchain_const.const_chain")
    assert r is None and u.kind == "chain-constant"
    assert u.args == (("lit", 5, "int"),)


def test_chain_method_delegation(vendor_path):
    vendor_path("vendchain_m", VENDOR_CHAIN)
    u, r = _deleg("vendchain_m.method_chain")
    assert r is None and u.kind == "delegation-method"
    assert u.delegate == "upper" and u.args == (("param", 0),)


def test_chain_computed_value_refuses(vendor_path):
    vendor_path(
        "vendchain_comp",
        "def g(a):\n    return a\n\n"
        "def f(a):\n    x = h(a)\n    return g(x)\n",
    )
    u, r = _deleg("vendchain_comp.f")
    assert u is None and r is not None and "chain value is computed" in r.reason


def test_chain_walrus_refuses(vendor_path):
    vendor_path(
        "vendchain_walrus",
        "def g(a):\n    return a\n\n"
        "def f(a):\n    x = (y := a)\n    return g(x)\n",
    )
    u, r = _deleg("vendchain_walrus.f")
    assert u is None and r is not None and "walrus" in r.reason


def test_chain_unpack_not_candidate(vendor_path):
    vendor_path(
        "vendchain_unpack",
        "def g(a):\n    return a\n\n"
        "def f(a, b):\n    x, y = a, b\n    return g(x)\n",
    )
    u, r = _deleg("vendchain_unpack.f")
    assert u is None and r is None


def test_chain_constant_emits_equality(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        delegation_universe_for_callee,
    )
    from sugar_lift_py_tests.ir import _ConstInt

    delegation_universe_for_callee.cache_clear()
    vendor_path("vendchain_l2", VENDOR_CHAIN)
    out = _lift(
        """
        import vendchain_l2

        def test_c():
            assert vendchain_l2.const_chain(1) == 9
        """
    )
    from sugar_lift_py_tests.layer2 import _iter_conjuncts

    fives = []
    for d in out.decls:
        if d.inv is None:
            continue
        for a in _iter_conjuncts(d.inv):
            if getattr(a, "name", None) == "=":
                args = getattr(a, "args", ())
                if len(args) == 2 and isinstance(args[1], _ConstInt) and args[1].value == 5:
                    fives.append(a)
    # the universe swears == 5; the claim swears == 9: UNSAT shape present
    assert fives, [d.name for d in out.decls]


# ---------------------------------------------------------------------------
# raise locus (census non-return:Raise, 30k bodies): zero Return/Yield +
# a terminal tail means every path raises — no value exists, so any
# sworn value equality carries the canonical contradiction (0 = 1). The
# guard family's complement, total instead of clause-wise.
# ---------------------------------------------------------------------------


def _raise_locus(callee):
    from sugar_lift_py_tests.translate_universe import (
        raise_locus_universe_for_callee,
    )

    raise_locus_universe_for_callee.cache_clear()
    return raise_locus_universe_for_callee(callee)


def test_bare_raise_body_walks(vendor_path):
    vendor_path(
        "vendraise_ok",
        "def boom(x):\n    raise ValueError(x)\n",
    )
    u, r = _raise_locus("vendraise_ok.boom")
    assert r is None and u is not None


def test_if_else_both_raise_walks(vendor_path):
    vendor_path(
        "vendraise_both",
        "def boom(x):\n"
        "    if x:\n"
        "        raise ValueError(x)\n"
        "    else:\n"
        "        raise TypeError(x)\n",
    )
    u, r = _raise_locus("vendraise_both.boom")
    assert r is None and u is not None


def test_prefix_then_tail_raise_walks(vendor_path):
    vendor_path(
        "vendraise_prefix",
        "def boom(x):\n"
        "    msg = format(x)\n"
        "    raise ValueError(msg)\n",
    )
    u, r = _raise_locus("vendraise_prefix.boom")
    assert r is None and u is not None


def test_fall_off_path_not_candidate(vendor_path):
    # the guarded raise without an else can fall off the end -> None
    vendor_path(
        "vendraise_fall",
        "def maybe(x):\n    if x:\n        raise ValueError(x)\n",
    )
    u, r = _raise_locus("vendraise_fall.maybe")
    assert u is None and r is None


def test_try_wrapped_raise_not_candidate(vendor_path):
    # a handler may swallow the raise and fall off -> None can exist
    vendor_path(
        "vendraise_try",
        "def maybe(x):\n"
        "    try:\n"
        "        raise ValueError(x)\n"
        "    except ValueError:\n"
        "        pass\n",
    )
    u, r = _raise_locus("vendraise_try.maybe")
    assert u is None and r is None


def test_any_return_not_candidate(vendor_path):
    vendor_path(
        "vendraise_ret",
        "def maybe(x):\n"
        "    if x:\n"
        "        return 1\n"
        "    raise ValueError(x)\n",
    )
    u, r = _raise_locus("vendraise_ret.maybe")
    assert u is None and r is None


def test_generator_raise_not_candidate(vendor_path):
    # calling a generator function returns a generator object: a value
    vendor_path(
        "vendraise_gen",
        "def gen(x):\n    yield x\n    raise ValueError(x)\n",
    )
    u, r = _raise_locus("vendraise_gen.gen")
    assert u is None and r is None


def test_raise_locus_contradicts_any_value_claim(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        raise_locus_universe_for_callee,
    )
    from sugar_lift_py_tests.ir import _ConstInt

    raise_locus_universe_for_callee.cache_clear()
    vendor_path(
        "vendraise_l2", "def boom(x):\n    raise ValueError(x)\n"
    )
    out = _lift(
        """
        import vendraise_l2

        def test_boom():
            assert vendraise_l2.boom(1) == 3
        """
    )
    from sugar_lift_py_tests.layer2 import _iter_conjuncts

    contradictions = []
    for d in out.decls:
        if d.inv is None:
            continue
        for a in _iter_conjuncts(d.inv):
            if getattr(a, "name", None) != "=":
                continue
            args = getattr(a, "args", ())
            if (
                len(args) == 2
                and isinstance(args[0], _ConstInt)
                and isinstance(args[1], _ConstInt)
                and args[0].value == 0
                and args[1].value == 1
            ):
                contradictions.append(a)
    assert contradictions, [d.name for d in out.decls]


def test_pytest_raises_carries_raise_locus_source_warrant(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        raise_locus_universe_for_callee,
    )

    raise_locus_universe_for_callee.cache_clear()
    vendor_path(
        "vendraise_source",
        """
        class Abstract:
            def boom(self, value):
                raise NotImplementedError()
        """,
    )
    out = _lift(
        """
        import pytest
        import vendraise_source

        def test_boom():
            with pytest.raises(NotImplementedError):
                vendraise_source.Abstract.boom(None, 1)
        """
    )

    assertion = next(
        (
            d for d in out.decls
            if d.name == "test_boom"
        ),
        None,
    )
    assert assertion is not None, [d.name for d in out.decls]
    assert any(
        warrant.get("role") == "python.raise-locus-universe"
        and warrant.get("source_function_name") == "Abstract.boom"
        for warrant in assertion.source_warrants
    ), assertion.source_warrants

    audit = next(
        audit
        for audit in out.source_audits
        if audit["role"] == "python.raise-locus-universe"
    )
    assert audit["totals"]["unclassified_source"] == 0
    assert any(
        locus["status"] == "warranted"
        and locus.get("ast_kind") == "Raise"
        for locus in audit["loci"]
    ), audit


# ---------------------------------------------------------------------------
# chain-expr (census return-binop, 17k bodies): the returned arithmetic
# expression as STRUCTURE — eq(subject, ctor("+", ...)) over the same
# operator ctors the consumer side builds. + - * lower to real Int math
# substrate-side; / % stay EUF. The emitter bridges only all-Int-const
# instantiations: '+' on strings is CONCAT by dispatch, and a string
# leaf under an arithmetic-lowered ctor is the cross-sort mislower.
# ---------------------------------------------------------------------------


def test_binop_return_walks(vendor_path):
    vendor_path("vendbinop_ok", "def add(a, b):\n    return a + b\n")
    u, r = _deleg("vendbinop_ok.add")
    assert r is None and u is not None
    assert u.kind == "chain-expr"
    assert u.expr_spec == ("binop", "+", ("param", 0), ("param", 1))


def test_nested_binop_with_chain(vendor_path):
    vendor_path(
        "vendbinop_nest",
        "def scale(a, b):\n    x = b\n    return (a + x) * 2\n",
    )
    u, r = _deleg("vendbinop_nest.scale")
    assert r is None and u.expr_spec == (
        "binop", "*",
        ("binop", "+", ("param", 0), ("param", 1)),
        ("lit", 2, "int"),
    )


def test_unsupported_binop_refuses(vendor_path):
    vendor_path("vendbinop_pow", "def p(a, b):\n    return a ** b\n")
    u, r = _deleg("vendbinop_pow.p")
    assert u is None and r is not None and "lowered set" in r.reason


def test_computed_binop_leaf_refuses(vendor_path):
    vendor_path(
        "vendbinop_comp", "def f(a):\n    return a + g(a)\n"
    )
    u, r = _deleg("vendbinop_comp.f")
    assert u is None and r is not None and "binop leaf" in r.reason


def test_binop_emits_arithmetic_equality(vendor_path):
    from sugar_lift_py_tests.translate_universe import (
        delegation_universe_for_callee,
    )

    delegation_universe_for_callee.cache_clear()
    vendor_path("vendbinop_l2", "def add(a, b):\n    return a + b\n")
    out = _lift(
        """
        import vendbinop_l2

        def test_add():
            assert vendbinop_l2.add(2, 3) == 9
        """
    )
    from sugar_lift_py_tests.layer2 import _iter_conjuncts

    plus_eqs = []
    for d in out.decls:
        if d.inv is None:
            continue
        for a in _iter_conjuncts(d.inv):
            if getattr(a, "name", None) != "=":
                continue
            for side in getattr(a, "args", ()):
                if getattr(side, "name", None) == "+":
                    plus_eqs.append(a)
    # eq(subject, +(2, 3)) conjoined with the claim == 9: Int theory
    # makes it UNSAT (2 + 3 = 5)
    assert plus_eqs, [d.name for d in out.decls]


def test_binop_skips_string_instantiation():
    # the ground gate at the emission seam: a string leaf must emit
    # nothing — '+' over strings is concat by dispatch, not arithmetic
    import sugar_lift_py_tests.layer2 as l2
    from sugar_lift_py_tests.ir import str_const, num

    term = l2._expr_spec_term(
        ("binop", "+", ("param", 0), ("lit", 1, "int")),
        [str_const("a")],
    )
    assert term is not None
    assert not l2._term_leaves_all_const_int(term)
    ok = l2._expr_spec_term(
        ("binop", "+", ("param", 0), ("lit", 1, "int")),
        [num(4)],
    )
    assert l2._term_leaves_all_const_int(ok)
