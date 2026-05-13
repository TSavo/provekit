# SPDX-License-Identifier: Apache-2.0
#
# cpython_ctypes_resolver: Python ctypes FFI call-site resolver.
#
# Walks a Python AST module and detects ctypes FFI load + call patterns,
# emitting CallEdgeDecl mementos per
# protocol/specs/2026-05-03-bridge-linkage-protocol.md §1 R1 and R3.
#
# Mirrors Go's cgo resolver in implementations/go/cmd/provekit-lsp-go/main.go
# (parseCgoPreamble / resolveCgoKit / walkCallEdges pattern). Each FFI host
# has its own resolver because each has different load conventions and ABI
# assumptions; the IR is the common substrate beneath them all.
#
# Supported patterns (spec #114 §Python ctypes):
#   A: lib = ctypes.CDLL("./libfoo.so"); lib.bar(...)
#   B: lib = ctypes.cdll.LoadLibrary("libfoo"); foo = ctypes.cast(lib.foo, ...); foo(...)
#   C: lib = ctypes.PyDLL("./libfoo.so"); lib.bar(...)
#   D: lib = ctypes.WinDLL("foo.dll"); lib.WinFunc(...)
#
# Out of scope: ctypes.util.find_library, dynamic string construction,
# __import__-based loading. Those are follow-up resolvers.

from __future__ import annotations

import ast
import os
from dataclasses import dataclass
from typing import Dict, List, Optional, Tuple

from .ir import (
    CallEdgeDecl,
    Formula,
    Locus,
    atomic,
    make_var,
)


# ---------------------------------------------------------------------------
# Known system libraries (pseudo-kit "libc-system", opaque to the linker).
# ---------------------------------------------------------------------------

_SYSTEM_LIBS = frozenset(
    {
        "c",
        "m",
        "pthread",
        "dl",
        "rt",
        "util",
        "resolv",
        "nsl",
        "z",
        "bz2",
        "lzma",
        "crypto",
        "ssl",
        "X11",
        "Xext",
        "GL",
    }
)

# CDLL loader attribute names (Pattern A, C, D).
_CDLL_ATTR = frozenset({"CDLL", "PyDLL", "WinDLL"})


# ---------------------------------------------------------------------------
# Library-name normalisation
# ---------------------------------------------------------------------------


def _strip_lib_name(raw: str) -> str:
    """Strip directory prefix, 'lib' prefix, and .so/.dll/.dylib suffix.

    Examples:
        "./librust_callee.so" -> "rust_callee"
        "librust_callee"      -> "rust_callee"
        "libc.so.6"           -> "c"
        "foo.dll"             -> "foo"
        "libfoo.dylib"        -> "foo"

    The special form 'libc.so.6' is handled by stripping the basename,
    then the 'lib' prefix, then all trailing components that start with
    a digit or are a known extension.
    """
    name = os.path.basename(raw)

    # Strip known extensions repeatedly (handles libc.so.6 -> libc.so -> libc).
    while True:
        base, ext = os.path.splitext(name)
        if ext.lower() in (".so", ".dll", ".dylib", ".a"):
            name = base
        elif ext and ext[1:].isdigit():
            # Trailing version component like .6 in libc.so.6
            name = base
        else:
            break

    # Strip 'lib' prefix.
    if name.startswith("lib"):
        name = name[3:]

    return name


def _resolve_kit(lib_name: str) -> Optional[str]:
    """Map a normalised library name to a kit name.

    Returns:
        - "libc-system" for known system libraries (opaque pseudo-kit).
        - "rust-kit" as the default for Rust-origin libraries (mirrors the
          Go cgo resolver's default per the Michael Jordan demo framing).
        - None when the library is unknown (caller emits a linker-error).

    Spec note: R3 example says ctypes maps to "cpp-kit:foo" but the dispatch
    and Go resolver both default to "rust-kit". This discrepancy is flagged in
    the PR; the default here follows the dispatch, not the spec example.
    """
    if lib_name in _SYSTEM_LIBS:
        return "libc-system"
    # For the MVP, all non-system libraries are assumed Rust-origin.
    # Future work: inspect Cargo.toml to confirm crate name matches lib_name.
    if lib_name:
        return "rust-kit"
    return None


# ---------------------------------------------------------------------------
# AST scanning
# ---------------------------------------------------------------------------


@dataclass
class _LibLoad:
    """Records a ctypes library load: variable name bound -> (lib_name, kit)."""

    var_name: str
    lib_name: str  # normalised
    kit: Optional[str]  # None -> unresolvable


def _extract_string_arg(node: ast.expr) -> Optional[str]:
    """Return the string value of a string-literal AST node, or None."""
    if isinstance(node, ast.Constant) and isinstance(node.value, str):
        return node.value
    # Python < 3.8 ast.Str
    if hasattr(ast, "Str") and isinstance(node, ast.Str):  # type: ignore[attr-defined]
        return node.s  # type: ignore[attr-defined]
    return None


def _detect_cdll_load(node: ast.Call) -> Optional[str]:
    """Detect ctypes.{CDLL,PyDLL,WinDLL}("lib") and return the raw lib string.

    Handles:
      ctypes.CDLL("./libfoo.so")   -> Pattern A / C / D
    """
    func = node.func
    if not isinstance(func, ast.Attribute):
        return None
    if func.attr not in _CDLL_ATTR:
        return None
    # Receiver must be `ctypes` (simple name).
    if not isinstance(func.value, ast.Name) or func.value.id != "ctypes":
        return None
    if not node.args:
        return None
    return _extract_string_arg(node.args[0])


def _detect_load_library(node: ast.Call) -> Optional[str]:
    """Detect ctypes.cdll.LoadLibrary("lib") and return the raw lib string.

    Pattern B.
    """
    func = node.func
    if not isinstance(func, ast.Attribute):
        return None
    if func.attr != "LoadLibrary":
        return None
    # Receiver must be ctypes.cdll
    recv = func.value
    if not isinstance(recv, ast.Attribute):
        return None
    if recv.attr != "cdll":
        return None
    if not isinstance(recv.value, ast.Name) or recv.value.id != "ctypes":
        return None
    if not node.args:
        return None
    return _extract_string_arg(node.args[0])


def _scan_loads(tree: ast.Module) -> Dict[str, _LibLoad]:
    """Walk the module and collect ctypes library loads.

    Returns a dict mapping variable name -> _LibLoad for each assignment
    of the form:
        <name> = ctypes.CDLL(...)
        <name> = ctypes.cdll.LoadLibrary(...)
        <name> = ctypes.PyDLL(...)
        <name> = ctypes.WinDLL(...)
    """
    loads: Dict[str, _LibLoad] = {}
    for node in ast.walk(tree):
        if not isinstance(node, ast.Assign):
            continue
        call = node.value
        if not isinstance(call, ast.Call):
            continue
        # Use explicit None sentinel to distinguish "not a ctypes load"
        # from "ctypes load with empty string path".
        raw = _detect_cdll_load(call)
        if raw is None:
            raw = _detect_load_library(call)
        if raw is None:
            continue
        lib_name = _strip_lib_name(raw)
        kit = _resolve_kit(lib_name)
        # Only assign to simple name targets.
        for target in node.targets:
            if isinstance(target, ast.Name):
                loads[target.id] = _LibLoad(
                    var_name=target.id,
                    lib_name=lib_name,
                    kit=kit,
                )
    return loads


# ---------------------------------------------------------------------------
# LinkerError memento
# ---------------------------------------------------------------------------


@dataclass
class LinkerErrorMemento:
    """Emitted when a ctypes load cannot be resolved to a kit.

    kind: "linker-error"
    errorKind: "unresolvable-ctypes-target"
    """

    lib_name: str
    call_site_locus: Locus
    source_contract_cid: str


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------


@dataclass
class CtypesResolverResult:
    """Resolver output: call-edge mementos and linker-error mementos."""

    call_edges: List[CallEdgeDecl]
    linker_errors: List[LinkerErrorMemento]


def resolve_ctypes_calls(
    source: str,
    path: str,
    contract_index: Dict[str, str],
) -> CtypesResolverResult:
    """Walk Python source and emit call-edge mementos for ctypes FFI calls.

    Args:
        source: Python source text.
        path: Source file path (used for locus field).
        contract_index: Map from function name -> contractCid.
            Only call sites inside contracted functions produce edges,
            mirroring Go's walkCallEdges guard (callers without contracts
            are skipped).

    Returns:
        CtypesResolverResult with call_edges and linker_errors lists.

    The two lists are both emitted in deterministic source order (line asc,
    column asc within the same line) so two runs over byte-identical source
    produce byte-identical output.
    """
    try:
        tree = ast.parse(source, filename=path)
    except SyntaxError:
        return CtypesResolverResult(call_edges=[], linker_errors=[])

    # Check that ctypes is imported.
    has_ctypes_import = _has_ctypes_import(tree)
    if not has_ctypes_import:
        return CtypesResolverResult(call_edges=[], linker_errors=[])

    # Collect all ctypes library loads in module scope.
    lib_loads = _scan_loads(tree)
    if not lib_loads:
        return CtypesResolverResult(call_edges=[], linker_errors=[])

    call_edges: List[CallEdgeDecl] = []
    linker_errors: List[LinkerErrorMemento] = []

    # Walk function bodies whose function name has a contract.
    for node in ast.walk(tree):
        if not isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            continue
        caller_name = node.name
        source_cid = contract_index.get(caller_name)
        if source_cid is None:
            # Caller has no contract; skip per R1 (mirrors Go's guard).
            continue

        # Walk the function body for lib.<attr>(...) call expressions.
        _walk_function_body(
            func_node=node,
            caller_name=caller_name,
            source_cid=source_cid,
            lib_loads=lib_loads,
            path=path,
            call_edges=call_edges,
            linker_errors=linker_errors,
        )

    return CtypesResolverResult(call_edges=call_edges, linker_errors=linker_errors)


def _has_ctypes_import(tree: ast.Module) -> bool:
    """Return True if the module imports ctypes (any form)."""
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                if alias.name == "ctypes" or alias.name.startswith("ctypes."):
                    return True
        if isinstance(node, ast.ImportFrom):
            if node.module == "ctypes" or (
                node.module is not None and node.module.startswith("ctypes.")
            ):
                return True
    return False


def _walk_function_body(
    func_node: ast.FunctionDef,
    caller_name: str,
    source_cid: str,
    lib_loads: Dict[str, _LibLoad],
    path: str,
    call_edges: List[CallEdgeDecl],
    linker_errors: List[LinkerErrorMemento],
) -> None:
    """Walk a function body and emit edges/errors for ctypes lib calls."""
    for child in ast.walk(func_node):
        if not isinstance(child, ast.Call):
            continue
        func_expr = child.func
        # Detect lib.<func_name>(...) call.
        if not isinstance(func_expr, ast.Attribute):
            continue
        recv = func_expr.value
        if not isinstance(recv, ast.Name):
            continue
        lib_var = recv.id
        load = lib_loads.get(lib_var)
        if load is None:
            continue
        func_name = func_expr.attr
        # Get source position (1-based line, 1-based column via +1).
        line = getattr(child, "lineno", 1)
        col = getattr(child, "col_offset", 0) + 1  # Python AST is 0-based
        locus = Locus(file=path, line=line, column=col)
        evidence: Formula = atomic(
            "call-site-obligation",
            [make_var(caller_name)],
        )
        if load.kit is None:
            linker_errors.append(
                LinkerErrorMemento(
                    lib_name=load.lib_name,
                    call_site_locus=locus,
                    source_contract_cid=source_cid,
                )
            )
        else:
            target_symbol = f"{load.kit}:{func_name}"
            call_edges.append(
                CallEdgeDecl(
                    source_contract_cid=source_cid,
                    target_contract_cid=None,
                    target_symbol=target_symbol,
                    call_site_locus=locus,
                    evidence_term=evidence,
                )
            )
