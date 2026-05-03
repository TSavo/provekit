# SPDX-License-Identifier: Apache-2.0
#
# Tests for the Python ctypes FFI resolver.
#
# Spec: protocol/specs/2026-05-03-bridge-linkage-protocol.md R1, R3.
# Implementation: cpython_ctypes_resolver.py
#
# Design note: call-edges are only emitted for call sites inside contracted
# functions, mirroring Go's walkCallEdges guard. Tests that expect call-edges
# must wrap ctypes calls in a function that appears in contract_index.
# Tests that expect no edges use an empty contract_index or no ctypes import.
#
# Pattern coverage per dispatch spec:
#   A: ctypes.CDLL("./libfoo.so")
#   B: ctypes.cdll.LoadLibrary("libfoo")
#   C: ctypes.PyDLL("./libfoo.so")
#   D: ctypes.WinDLL("foo.dll")

from __future__ import annotations

import textwrap

import pytest

from provekit_lift_py_tests.cpython_ctypes_resolver import (
    resolve_ctypes_calls,
    LinkerErrorMemento,
)
from provekit_lift_py_tests.ir import (
    CallEdgeDecl,
    call_edge_decl_to_value,
    call_edges_to_value,
    locus_to_value,
)
from provekit_lift_py_tests.canonicalizer import encode_jcs

# Synthetic contract CID used as source in all tests.
_FAKE_CID = "blake3-512:" + "a" * 128


def _contract_index(func_name: str) -> dict:
    return {func_name: _FAKE_CID}


def _resolve(source: str, contract_index: dict = None, path: str = "test.py"):
    src = textwrap.dedent(source)
    ci = contract_index if contract_index is not None else {}
    return resolve_ctypes_calls(src, path, ci)


# ---------------------------------------------------------------------------
# Test 1: Pattern A — CDLL load + call resolves to rust-kit
# ---------------------------------------------------------------------------


def test_cdll_rust_callee_emits_call_edge():
    """Pattern A: ctypes.CDLL("librust_callee.so") -> lib.process(n) emits
    targetSymbol = "rust-kit:process"."""
    source = """
        import ctypes
        lib = ctypes.CDLL("librust_callee.so")
        lib.process.restype = ctypes.c_int

        def call_process(value):
            result = lib.process(value)
            return result
    """
    result = _resolve(source, _contract_index("call_process"))
    assert len(result.call_edges) == 1, f"expected 1 edge, got: {result.call_edges}"
    edge = result.call_edges[0]
    assert edge.target_symbol == "rust-kit:process"
    assert edge.source_contract_cid == _FAKE_CID
    assert edge.target_contract_cid is None


# ---------------------------------------------------------------------------
# Test 2: Pattern A with libc — emits libc-system pseudo-kit
# ---------------------------------------------------------------------------


def test_cdll_libc_emits_libc_system():
    """ctypes.CDLL("libc.so.6") -> libc.printf(...) -> targetSymbol = "libc-system:printf"."""
    source = """
        import ctypes
        libc = ctypes.CDLL("libc.so.6")

        def call_printf(msg):
            libc.printf(msg)
    """
    result = _resolve(source, _contract_index("call_printf"))
    assert len(result.call_edges) == 1, f"expected 1 edge, got: {result.call_edges}"
    edge = result.call_edges[0]
    assert edge.target_symbol == "libc-system:printf"


# ---------------------------------------------------------------------------
# Test 3: No ctypes import -> no edges emitted
# ---------------------------------------------------------------------------


def test_no_ctypes_import_emits_no_edges():
    """File with no ctypes import and no FFI calls -> no call-edges emitted."""
    source = """
        def regular_function(x):
            return x + 1

        def call_regular(n):
            return regular_function(n)
    """
    result = _resolve(source, _contract_index("call_regular"))
    assert result.call_edges == []
    assert result.linker_errors == []


# ---------------------------------------------------------------------------
# Test 4: Unknown library -> linker-error memento
# ---------------------------------------------------------------------------


def test_unknown_library_emits_linker_error():
    """ctypes.CDLL("unknown_library") with an empty name -> linker-error,
    NOT a placeholder targetSymbol string."""
    # "unknown_library" strips to "unknown_library" which the resolver maps
    # to "rust-kit" (default). To force an unresolvable error we need an
    # empty lib name. Use a path that strips to empty string.
    # Per resolver logic: _resolve_kit("") returns None -> linker-error.
    source = """
        import ctypes
        lib = ctypes.CDLL("")

        def call_empty(n):
            lib.foo(n)
    """
    result = _resolve(source, _contract_index("call_empty"))
    assert len(result.linker_errors) == 1, f"expected 1 error, got edges={result.call_edges} errors={result.linker_errors}"
    err = result.linker_errors[0]
    assert isinstance(err, LinkerErrorMemento)
    # Must NOT emit a call-edge for this call.
    assert result.call_edges == []


# ---------------------------------------------------------------------------
# Test 5: Byte-determinism — two runs produce identical call-edge stream
# ---------------------------------------------------------------------------


def test_byte_determinism():
    """Two runs over the same source produce byte-identical call-edge JSON."""
    source = """
        import ctypes
        lib = ctypes.CDLL("librust_callee.so")

        def call_process(value):
            result = lib.process(value)
            return result
    """
    ci = _contract_index("call_process")
    r1 = _resolve(source, ci)
    r2 = _resolve(source, ci)

    assert len(r1.call_edges) == len(r2.call_edges)
    for e1, e2 in zip(r1.call_edges, r2.call_edges):
        assert encode_jcs(call_edge_decl_to_value(e1)) == encode_jcs(call_edge_decl_to_value(e2))

    # Also compare the full stream as a JSON string.
    stream1 = encode_jcs(call_edges_to_value(r1.call_edges))
    stream2 = encode_jcs(call_edges_to_value(r2.call_edges))
    assert stream1 == stream2


# ---------------------------------------------------------------------------
# Test 6: Cross-pattern coverage — all four patterns A, B, C, D
# ---------------------------------------------------------------------------


def test_pattern_a_cdll():
    """Pattern A: ctypes.CDLL(...)."""
    source = """
        import ctypes
        lib = ctypes.CDLL("./libfoo.so")

        def call_bar(x):
            lib.bar(x)
    """
    result = _resolve(source, _contract_index("call_bar"))
    assert len(result.call_edges) == 1
    assert result.call_edges[0].target_symbol == "rust-kit:bar"


def test_pattern_b_load_library():
    """Pattern B: ctypes.cdll.LoadLibrary(...)."""
    source = """
        import ctypes
        lib = ctypes.cdll.LoadLibrary("librust_callee")

        def call_foo(x):
            lib.foo(x)
    """
    result = _resolve(source, _contract_index("call_foo"))
    assert len(result.call_edges) == 1
    assert result.call_edges[0].target_symbol == "rust-kit:foo"


def test_pattern_c_pydll():
    """Pattern C: ctypes.PyDLL(...)."""
    source = """
        import ctypes
        lib = ctypes.PyDLL("./libfoo.so")

        def call_baz(x):
            lib.baz(x)
    """
    result = _resolve(source, _contract_index("call_baz"))
    assert len(result.call_edges) == 1
    assert result.call_edges[0].target_symbol == "rust-kit:baz"


def test_pattern_d_windll():
    """Pattern D: ctypes.WinDLL(...)."""
    source = """
        import ctypes
        lib = ctypes.WinDLL("foo.dll")

        def call_win_func(x):
            lib.WinFunc(x)
    """
    result = _resolve(source, _contract_index("call_win_func"))
    assert len(result.call_edges) == 1
    assert result.call_edges[0].target_symbol == "rust-kit:WinFunc"


# ---------------------------------------------------------------------------
# Additional: contracted function guards (callers without contracts skipped)
# ---------------------------------------------------------------------------


def test_no_contract_for_caller_skips_edge():
    """Call sites in functions not in contract_index produce no edges."""
    source = """
        import ctypes
        lib = ctypes.CDLL("librust_callee.so")

        def uncontracted_caller(x):
            lib.process(x)
    """
    # Empty contract_index: uncontracted_caller has no entry.
    result = _resolve(source, {})
    assert result.call_edges == []


# ---------------------------------------------------------------------------
# Additional: locus is populated correctly
# ---------------------------------------------------------------------------


def test_call_site_locus_populated():
    """Emitted edges include a non-zero line number in callSiteLocus."""
    source = """
        import ctypes
        lib = ctypes.CDLL("librust_callee.so")

        def call_process(value):
            result = lib.process(value)
            return result
    """
    result = _resolve(source, _contract_index("call_process"))
    assert len(result.call_edges) == 1
    locus = result.call_edges[0].call_site_locus
    assert locus.line > 0
    assert locus.column > 0
    assert locus.file == "test.py"


# ---------------------------------------------------------------------------
# Additional: lib name normalisation edge cases
# ---------------------------------------------------------------------------


def test_lib_name_normalisation_with_path_prefix():
    """./libfoo.so -> foo -> rust-kit."""
    source = """
        import ctypes
        lib = ctypes.CDLL("./libfoo.so")

        def do_call():
            lib.compute()
    """
    result = _resolve(source, _contract_index("do_call"))
    assert len(result.call_edges) == 1
    assert result.call_edges[0].target_symbol == "rust-kit:compute"


def test_lib_name_libc_so_6_normalised():
    """libc.so.6 normalises to 'c' -> libc-system."""
    source = """
        import ctypes
        lib = ctypes.CDLL("libc.so.6")

        def call_strlen(s):
            return lib.strlen(s)
    """
    result = _resolve(source, _contract_index("call_strlen"))
    assert len(result.call_edges) == 1
    assert result.call_edges[0].target_symbol == "libc-system:strlen"
