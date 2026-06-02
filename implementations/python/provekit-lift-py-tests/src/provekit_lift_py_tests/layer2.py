# SPDX-License-Identifier: Apache-2.0
#
# provekit-lift-py-tests / layer 2
#
# Layer 2 sits ABOVE the (future) Python Layer 0 ("mechanical assert
# recognition") and below the eventual Layer 3 LLM lift. It walks the
# Python AST of a source file and recognizes five structural patterns:
#
#   PATTERN 1 - bounded for loop as universal quantifier
#       def test_x():
#           for i in range(lo, hi):           # range(hi) accepted, step skipped
#               assert <comp> ...             # single-stmt body
#   Lift to: forall i:Int. (lo <= i AND i < hi) -> phi(i).
#   Memento name: ``<test>::loop::<var>``.
#
#   Variant 1b - bounded loop over a literal list:
#       def test_x():
#           for v in [1, 2, 3]:
#               assert <comp involving v>
#   Lift to: enumerated and-conjunction of phi[v_i] across the list.
#   Same memento name shape.
#
#   PATTERN 2 - helper function inlined at each call site
#       def assert_is_42(x: int):
#           assert x == 42
#       def test_x():
#           assert_is_42(42)
#           assert_is_42(42)
#   Lift to: ONE memento per call site, ``<test>::call::<i>`` zero-indexed.
#   The helper must have one TYPED parameter and a body that is exactly one
#   liftable assertion.
#
#   PATTERN 3 - multi-assertion characterization
#       def test_x():
#           assert f(1) == 1
#           assert f(2) == 2
#           assert f(3) != 0
#   Lift to: ONE conjunction memento at ``<test>``. >=2 atoms required;
#   <2 releases the claim back to Layer 0.
#
#   PATTERN 4 - @pytest.mark.parametrize over a literal arg list
#       @pytest.mark.parametrize("v", [1, 2, 3])
#       def test_x(v):
#           assert v >= 0
#   Lift to: ONE memento, body = and-conjunction over each row substitution.
#   Memento name: ``<test>::parametrize::<param-names>``.
#
#   PATTERN 5 - callsite value-scope facts plus implications
#       def test_parse():
#           actual = parse_int("42")
#           assert actual == 42
#   Lift to: two callsite-owned contracts,
#       ``parse_int@<file>:<line>:<col>::facts`` and
#       ``parse_int@<file>:<line>:<col>::assertion``,
#   plus an implication edge from facts to assertion. The test is evidence
#   describing the callsite contract; it does not own the contract name.
#
# CLAIM SET:
#   ``lift_file_layer2`` returns a ``claimed_tests`` set: each test name
#   Layer 2 took ownership of. The dispatcher passes that set to Layer 0
#   (when Python Layer 0 lands) so each test fn is lifted by AT MOST one
#   layer. Out-of-scope shapes (hypothesis ``@given``, pytest fixtures,
#   ``with pytest.raises``, etc.) are left for Layer 0 / Layer 3.

from __future__ import annotations

import ast
import os
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Sequence, Set, Tuple

from .ir import (
    ContractDecl,
    Formula,
    Int,
    Term,
    _Ctor,
    and_,
    atomic,
    bool_const,
    comparison_with_none_guard,
    connective,
    ctor,
    eq,
    gt,
    gte,
    implies,
    lt,
    lte,
    make_var,
    ne,
    not_,
    num,
    str_const,
    subst_var_in_formula,
)


# ---------------------------------------------------------------------------
# Data shapes
# ---------------------------------------------------------------------------


@dataclass
class LiftWarning:
    source_path: str
    item_name: str
    reason: str


@dataclass
class Layer2Output:
    decls: List[ContractDecl] = field(default_factory=list)
    warnings: List[LiftWarning] = field(default_factory=list)
    seen: int = 0
    lifted: int = 0
    claimed_tests: Set[str] = field(default_factory=set)
    bounded_loop_lifted: int = 0
    bounded_loop_skipped: int = 0
    helper_inlined_lifted: int = 0
    helper_inlined_skipped: int = 0
    characterization_lifted: int = 0
    characterization_skipped: int = 0
    parametrize_lifted: int = 0
    parametrize_skipped: int = 0
    value_scope_lifted: int = 0
    value_scope_skipped: int = 0
    implications: List["ImplicationDecl"] = field(default_factory=list)


@dataclass
class ImplicationDecl:
    name: str
    antecedent: str
    consequent: str
    antecedent_slot: str = "inv"
    consequent_slot: str = "inv"
    prover: str = "python-test-value-scope"
    proof_witness: str = ""


@dataclass
class _HelperDef:
    """A function with one typed param + a single liftable assertion."""
    param_name: str
    assertion: ast.stmt  # the single body statement


@dataclass
class _CallOrigin:
    callee: str
    lineno: int
    col: int


@dataclass
class _ValueScope:
    current: Dict[str, Term] = field(default_factory=dict)
    origins: Dict[str, _CallOrigin] = field(default_factory=dict)
    facts: List[Formula] = field(default_factory=list)

    def copy(self) -> "_ValueScope":
        return _ValueScope(
            current=dict(self.current),
            origins=dict(self.origins),
            facts=list(self.facts),
        )


# ---------------------------------------------------------------------------
# Public entry point
# ---------------------------------------------------------------------------


def lift_file_layer2(source: str, source_path: str) -> Layer2Output:
    """Parse ``source`` and lift Layer 2 patterns. ``source`` is the file
    text; ``source_path`` is informational (used in warnings).
    """
    out = Layer2Output()
    try:
        tree = ast.parse(source, filename=source_path)
    except SyntaxError as e:
        out.warnings.append(
            LiftWarning(source_path, "<file>", f"layer2: failed to parse Python source: {e}")
        )
        return out

    helpers = _collect_helpers(tree)
    test_fns = list(_iter_test_functions(tree))
    for fn in test_fns:
        _classify_and_lift(fn, source_path, helpers, out)
    return out


# ---------------------------------------------------------------------------
# Test-function recognition
# ---------------------------------------------------------------------------


def _iter_test_functions(tree: ast.AST):
    """Yield FunctionDef / AsyncFunctionDef nodes that look like tests:
    free function ``test_*`` OR method ``test_*`` on a ``TestCase`` class.
    """
    for node in ast.walk(tree):
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            if node.name.startswith("test_") or node.name.startswith("test"):
                if node.name.startswith("test"):
                    # Methods inside a class show up here too; they have
                    # an ``self`` first arg which we strip when classifying.
                    yield node


def _collect_helpers(tree: ast.AST) -> Dict[str, _HelperDef]:
    """Find module-level (and class-level) functions whose body is exactly
    one liftable assertion and that take one typed parameter.
    """
    out: Dict[str, _HelperDef] = {}
    for node in ast.walk(tree):
        if not isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            continue
        if node.name.startswith("test"):
            continue
        helper = _helper_def_from_fn(node)
        if helper is not None:
            out[node.name] = helper
    return out


def _helper_def_from_fn(fn: ast.FunctionDef) -> Optional[_HelperDef]:
    args = fn.args
    # No *args / **kwargs / kw-only / pos-only.
    if args.vararg is not None or args.kwarg is not None:
        return None
    if args.kwonlyargs or args.kw_defaults:
        return None
    if args.defaults:
        return None
    positional = list(args.posonlyargs) + list(args.args)
    # Skip ``self`` for method helpers.
    if positional and positional[0].arg == "self":
        positional = positional[1:]
    if len(positional) != 1:
        return None
    arg = positional[0]
    if arg.annotation is None:
        # Mirrors Rust's FnArg::Typed requirement.
        return None
    body = fn.body
    if len(body) != 1:
        return None
    stmt = body[0]
    if not _is_assertion_stmt(stmt):
        return None
    return _HelperDef(param_name=arg.arg, assertion=stmt)


# ---------------------------------------------------------------------------
# Pattern classifier
# ---------------------------------------------------------------------------


def _strip_self(stmts: Sequence[ast.stmt], fn: ast.FunctionDef) -> List[ast.stmt]:
    """unittest TestCase methods carry ``self`` as their first arg. The
    body is unchanged, but we filter out docstring expression statements
    so they don't break the "every stmt is an assert" rule.
    """
    out: List[ast.stmt] = []
    for i, s in enumerate(stmts):
        # Drop a docstring at index 0.
        if (
            i == 0
            and isinstance(s, ast.Expr)
            and isinstance(s.value, ast.Constant)
            and isinstance(s.value.value, str)
        ):
            continue
        out.append(s)
    return out


def _classify_and_lift(
    fn: ast.FunctionDef,
    source_path: str,
    helpers: Dict[str, _HelperDef],
    out: Layer2Output,
) -> None:
    test_name = fn.name
    body = _strip_self(fn.body, fn)

    # PATTERN 4: parametrize decorator is the strongest claim. If a
    # ``@pytest.mark.parametrize`` decorator is present with a literal
    # arg list, claim regardless of body shape (we may still skip lift
    # if the body isn't a single liftable assert OR rows are non-literal).
    presence = _find_parametrize(fn)
    if presence is not None:
        if presence.mark is None:
            out.claimed_tests.add(test_name)
            out.seen += 1
            out.parametrize_skipped += 1
            out.warnings.append(
                LiftWarning(source_path, test_name, f"layer2 {presence.reason}")
            )
            return
        _classify_parametrize(fn, body, presence.mark, test_name, source_path, out)
        return

    # PATTERN 1: single bounded for-loop with single-stmt body.
    if len(body) == 1 and isinstance(body[0], ast.For):
        _classify_for_loop(body[0], test_name, source_path, out)
        return

    # PATTERN 2: every top-level stmt is a single-arg call to a known helper.
    helper_calls = _collect_helper_calls(body, helpers)
    if helper_calls is not None and helper_calls:
        _classify_helper_inlining(helper_calls, helpers, test_name, source_path, out)
        return

    # PATTERN 3: every stmt is an assertion AND there are >= 2.
    asserts: List[ast.stmt] = []
    all_asserts = bool(body)
    for stmt in body:
        if _is_assertion_stmt(stmt):
            asserts.append(stmt)
        else:
            all_asserts = False
            break
    if all_asserts and len(asserts) >= 2:
        _classify_characterization(asserts, test_name, source_path, out)
        return

    if _classify_value_scope(body, test_name, source_path, out):
        return

    unsupported_unittest_asserts = _unsupported_unittest_assertions(body)
    if unsupported_unittest_asserts:
        out.claimed_tests.add(test_name)
        out.seen += 1
        for name in unsupported_unittest_asserts:
            out.warnings.append(
                LiftWarning(
                    source_path,
                    test_name,
                    f"layer2 unittest lift-gap: unsupported assertion method `{name}`",
                )
            )
        return

    # No Layer 2 pattern claimed it. Leave for Layer 0.


# ---------------------------------------------------------------------------
# Assertion-statement recognition (Python flavor)
# ---------------------------------------------------------------------------


_UNITTEST_BINARY_PREDICATES = {
    "assertEqual": "=",
    "assertEquals": "=",
    "assertNotEqual": "≠",
    "assertGreater": ">",
    "assertGreaterEqual": "≥",
    "assertLess": "<",
    "assertLessEqual": "≤",
    "assertIs": "=",
    "assertIsNot": "≠",
}

_UNITTEST_NONE_PREDICATES = {
    "assertIsNone": "=",
    "assertIsNotNone": "≠",
}

_UNITTEST_TRUTH_PREDICATES = {"assertTrue", "assertFalse"}


def _is_assertion_stmt(stmt: ast.stmt) -> bool:
    """Return True iff ``stmt`` looks like a liftable assertion in either
    pytest (``assert <comp>``) or unittest (``self.assertX(...)``) form.
    Liftability is checked here only at the surface; ``_lift_assertion_stmt``
    does the actual term/formula construction and may still error.
    """
    if isinstance(stmt, ast.Assert):
        return True
    if isinstance(stmt, ast.Expr):
        call = stmt.value
        if isinstance(call, ast.Call):
            name = _attr_method_name(call.func)
            if name is not None and name in _UNITTEST_BINARY_PREDICATES:
                return True
            if name is not None and name in _UNITTEST_NONE_PREDICATES:
                return True
            if name in _UNITTEST_TRUTH_PREDICATES:
                return True
    return False


def _attr_method_name(func: ast.expr) -> Optional[str]:
    """``self.assertEqual`` -> ``"assertEqual"``. Returns None if shape
    doesn't fit ``self.<name>``.
    """
    if isinstance(func, ast.Attribute):
        return func.attr
    return None


def _unsupported_unittest_assertions(stmts: Sequence[ast.stmt]) -> List[str]:
    out: List[str] = []
    for stmt in stmts:
        if not isinstance(stmt, ast.Expr):
            continue
        call = stmt.value
        if not isinstance(call, ast.Call):
            continue
        name = _attr_method_name(call.func)
        if name is None:
            continue
        if not name.startswith("assert"):
            continue
        if (
            name in _UNITTEST_BINARY_PREDICATES
            or name in _UNITTEST_NONE_PREDICATES
            or name in _UNITTEST_TRUTH_PREDICATES
        ):
            continue
        out.append(name)
    return out


# ---------------------------------------------------------------------------
# Term and formula translation
# ---------------------------------------------------------------------------


def _translate_term(node: ast.expr) -> Term:
    """Whitelist:
      - identifier (Var)
      - integer / string / bool literal
      - unary-neg of an integer literal
      - single-arg call (Ctor with one arg)
    Everything else raises ValueError.
    """
    if isinstance(node, ast.Name):
        if node.id == "None":
            return ctor("None", [])
        if node.id == "True":
            return bool_const(True)
        if node.id == "False":
            return bool_const(False)
        return make_var(node.id)
    if isinstance(node, ast.Constant):
        v = node.value
        if isinstance(v, bool):
            return bool_const(v)
        if isinstance(v, int):
            return num(v)
        if isinstance(v, str):
            return str_const(v)
        if v is None:
            return ctor("None", [])
        raise ValueError(f"only int / str / bool / None literals are liftable; got {type(v)!r}")
    if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.USub):
        if isinstance(node.operand, ast.Constant) and isinstance(node.operand.value, int) and not isinstance(node.operand.value, bool):
            return num(-node.operand.value)
        raise ValueError("unary-neg only liftable on integer literals")
    if isinstance(node, ast.Call):
        if not isinstance(node.func, ast.Name):
            raise ValueError("call target must be a simple name")
        if node.keywords:
            raise ValueError("call with kwargs is not liftable")
        if len(node.args) == 0:
            return ctor(node.func.id, [])
        if len(node.args) > 1:
            raise ValueError(f"call `{node.func.id}` with {len(node.args)} args is not liftable in v0 (single-arg only)")
        inner = _translate_term(node.args[0])
        return ctor(node.func.id, [inner])
    raise ValueError("expression shape not in v0 lift whitelist")


_COMPARE_OP_MAP = {
    ast.Eq: "=",
    ast.NotEq: "≠",
    ast.Lt: "<",
    ast.LtE: "≤",
    ast.Gt: ">",
    ast.GtE: "≥",
}

_IDENTITY_OP_MAP = {
    ast.Is: "=",
    ast.IsNot: "≠",
}


def _is_none_term(term: Term) -> bool:
    return isinstance(term, _Ctor) and term.name == "None" and not term.args


def _comparison_from_ast_op(op: ast.cmpop, left: Term, right: Term) -> Formula:
    identity_sym = _IDENTITY_OP_MAP.get(type(op))
    if identity_sym is not None:
        if _is_none_term(left) == _is_none_term(right):
            raise ValueError("identity comparison is only supported against None")
        return comparison_with_none_guard(
            identity_sym,
            left,
            right,
            emit_none_guard=True,
        )
    sym = _COMPARE_OP_MAP.get(type(op))
    if sym is None:
        raise ValueError(f"unsupported comparison op: {type(op).__name__}")
    return comparison_with_none_guard(sym, left, right, emit_none_guard=False)


def _translate_bool_expr(node: ast.expr) -> Formula:
    """``assert <expr>``: only a single-comparison expression is liftable."""
    if isinstance(node, ast.Compare):
        if len(node.ops) != 1 or len(node.comparators) != 1:
            raise ValueError("only single comparisons are liftable (no chained `a < b < c`)")
        l = _translate_term(node.left)
        r = _translate_term(node.comparators[0])
        return _comparison_from_ast_op(node.ops[0], l, r)
    if isinstance(node, ast.NamedExpr):
        # Walrus inside an assert: skip.
        raise ValueError("walrus operator in assert is not liftable")
    raise ValueError("assert body must be a comparison expression")


def _lift_assertion_stmt(stmt: ast.stmt) -> Formula:
    """Translate a recognized assertion statement to a Formula. Raises
    ValueError if the stmt's surface looked liftable but a leaf isn't.
    """
    if isinstance(stmt, ast.Assert):
        return _translate_bool_expr(stmt.test)
    if isinstance(stmt, ast.Expr) and isinstance(stmt.value, ast.Call):
        call = stmt.value
        name = _attr_method_name(call.func)
        if name in _UNITTEST_BINARY_PREDICATES:
            if len(call.args) < 2:
                raise ValueError(f"{name} expects at least 2 positional args")
            l = _translate_term(call.args[0])
            r = _translate_term(call.args[1])
            sym = _UNITTEST_BINARY_PREDICATES[name]
            return comparison_with_none_guard(sym, l, r, emit_none_guard=False)
        if name in _UNITTEST_NONE_PREDICATES:
            if len(call.args) < 1:
                raise ValueError(f"{name} expects 1 positional arg")
            t = _translate_term(call.args[0])
            return comparison_with_none_guard(
                _UNITTEST_NONE_PREDICATES[name],
                t,
                ctor("None", []),
                emit_none_guard=True,
            )
        if name == "assertTrue":
            if len(call.args) < 1:
                raise ValueError("assertTrue expects 1 positional arg")
            return _translate_truth_assertion(call.args[0], True)
        if name == "assertFalse":
            if len(call.args) < 1:
                raise ValueError("assertFalse expects 1 positional arg")
            return _translate_truth_assertion(call.args[0], False)
        raise ValueError(f"unrecognized assertion method: {name!r}")
    raise ValueError("statement is not a liftable assertion")


def _translate_truth_assertion(node: ast.expr, expected: bool) -> Formula:
    try:
        formula = _translate_bool_expr(node)
    except ValueError as bool_error:
        try:
            t = _translate_term(node)
        except ValueError:
            raise bool_error
        return eq(t, bool_const(expected))
    return formula if expected else not_(formula)


# ---------------------------------------------------------------------------
# PATTERN 1: bounded for-loop
# ---------------------------------------------------------------------------


def _classify_for_loop(
    fl: ast.For,
    test_name: str,
    source_path: str,
    out: Layer2Output,
) -> None:
    out.claimed_tests.add(test_name)
    out.seen += 1

    if not isinstance(fl.target, ast.Name):
        out.bounded_loop_skipped += 1
        out.warnings.append(
            LiftWarning(source_path, test_name,
                        "layer2 bounded-loop: loop target is not a simple identifier")
        )
        return
    var_name = fl.target.id

    if len(fl.body) != 1:
        out.bounded_loop_skipped += 1
        out.warnings.append(
            LiftWarning(source_path, test_name,
                        f"layer2 bounded-loop: body has {len(fl.body)} stmts (only single-stmt bodies in v0)")
        )
        return
    if fl.orelse:
        out.bounded_loop_skipped += 1
        out.warnings.append(
            LiftWarning(source_path, test_name,
                        "layer2 bounded-loop: for/else clause is not liftable in v0")
        )
        return

    # Reject nested for-loop bodies.
    if _has_nested_for(fl.body[0]):
        out.bounded_loop_skipped += 1
        out.warnings.append(
            LiftWarning(source_path, test_name,
                        "layer2 bounded-loop: nested for-loop detected; deferred to Layer 2.5")
        )
        return

    body_stmt = fl.body[0]
    if not _is_assertion_stmt(body_stmt):
        out.bounded_loop_skipped += 1
        out.warnings.append(
            LiftWarning(source_path, test_name,
                        "layer2 bounded-loop: body stmt is not an assertion")
        )
        return

    # Iterator must be a literal-bounded numeric range OR a literal list.
    iter_node = fl.iter
    if _is_range_call(iter_node):
        rng = _parse_range_call(iter_node)
        if rng is None:
            out.bounded_loop_skipped += 1
            out.warnings.append(
                LiftWarning(source_path, test_name,
                            "layer2 bounded-loop: range() arg shape not in v0 whitelist (literal int / unary-neg int / bare ident only; no step)")
            )
            return
        lo_term, hi_term = rng
        try:
            inner_formula = _lift_assertion_stmt(body_stmt)
        except ValueError as e:
            out.bounded_loop_skipped += 1
            out.warnings.append(
                LiftWarning(source_path, test_name,
                            f"layer2 bounded-loop: inner assertion not liftable: {e}")
            )
            return
        var_term = make_var(var_name)
        antecedent = and_([gte(var_term, lo_term), lt(var_term, hi_term)])
        body_formula = implies(antecedent, inner_formula)
        from .ir import _Quantifier  # type: ignore
        q = _Quantifier("forall", var_name, Int(), body_formula)
        memento_name = f"{test_name}::loop::{var_name}"
        out.decls.append(ContractDecl(name=memento_name, inv=q))
        out.lifted += 1
        out.bounded_loop_lifted += 1
        return

    if isinstance(iter_node, ast.List):
        # for v in [1, 2, 3]: assert phi(v)  ->  and_( phi[v_i] )
        elements: List[Term] = []
        for el in iter_node.elts:
            try:
                elements.append(_translate_term(el))
            except ValueError as e:
                out.bounded_loop_skipped += 1
                out.warnings.append(
                    LiftWarning(source_path, test_name,
                                f"layer2 bounded-loop: list element not liftable: {e}")
                )
                return
        if not elements:
            out.bounded_loop_skipped += 1
            out.warnings.append(
                LiftWarning(source_path, test_name,
                            "layer2 bounded-loop: empty list iterator yields a vacuous conjunction; skipping")
            )
            return
        try:
            inner_formula = _lift_assertion_stmt(body_stmt)
        except ValueError as e:
            out.bounded_loop_skipped += 1
            out.warnings.append(
                LiftWarning(source_path, test_name,
                            f"layer2 bounded-loop: inner assertion not liftable: {e}")
            )
            return
        substituted = [subst_var_in_formula(inner_formula, var_name, t) for t in elements]
        body_formula = and_(substituted) if len(substituted) > 1 else substituted[0]
        memento_name = f"{test_name}::loop::{var_name}"
        out.decls.append(ContractDecl(name=memento_name, inv=body_formula))
        out.lifted += 1
        out.bounded_loop_lifted += 1
        return

    out.bounded_loop_skipped += 1
    out.warnings.append(
        LiftWarning(source_path, test_name,
                    "layer2 bounded-loop: iterator is not a literal-bounded numeric range or literal list")
    )


def _is_range_call(node: ast.expr) -> bool:
    return (
        isinstance(node, ast.Call)
        and isinstance(node.func, ast.Name)
        and node.func.id == "range"
    )


def _parse_range_call(node: ast.Call) -> Optional[Tuple[Term, Term]]:
    """Parse ``range(hi)`` / ``range(lo, hi)`` into (lo_term, hi_term).
    ``range(lo, hi, step)`` is rejected (out of v0 spec). Endpoints must
    be int literals, unary-neg int literals, or bare identifiers.
    """
    if node.keywords:
        return None
    n = len(node.args)
    if n == 1:
        hi = _literal_int_or_var(node.args[0])
        if hi is None:
            return None
        return (num(0), hi)
    if n == 2:
        lo = _literal_int_or_var(node.args[0])
        hi = _literal_int_or_var(node.args[1])
        if lo is None or hi is None:
            return None
        return (lo, hi)
    return None


def _literal_int_or_var(node: ast.expr) -> Optional[Term]:
    if isinstance(node, ast.Constant) and isinstance(node.value, int) and not isinstance(node.value, bool):
        return num(node.value)
    if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.USub):
        inner = node.operand
        if isinstance(inner, ast.Constant) and isinstance(inner.value, int) and not isinstance(inner.value, bool):
            return num(-inner.value)
        return None
    if isinstance(node, ast.Name):
        return make_var(node.id)
    return None


def _has_nested_for(stmt: ast.stmt) -> bool:
    """``stmt`` is the body[0] of the outer for-loop. A nested for-loop is
    either ``stmt`` itself (``for x in ...: for y in ...: ...``) OR a for
    anywhere inside its descendants (the body[0] is some other compound
    that contains a for).
    """
    if isinstance(stmt, ast.For):
        return True
    for sub in ast.walk(stmt):
        if isinstance(sub, ast.For):
            return True
    return False


# ---------------------------------------------------------------------------
# PATTERN 2: helper-inlined calls
# ---------------------------------------------------------------------------


@dataclass
class _HelperCall:
    helper_name: str
    arg: ast.expr


def _collect_helper_calls(
    stmts: Sequence[ast.stmt],
    helpers: Dict[str, _HelperDef],
) -> Optional[List[_HelperCall]]:
    """If every top-level stmt is a single-arg call to a known helper,
    return the list of calls. Otherwise None.
    """
    if not stmts:
        return None
    out: List[_HelperCall] = []
    for stmt in stmts:
        if not isinstance(stmt, ast.Expr):
            return None
        call = stmt.value
        if not isinstance(call, ast.Call):
            return None
        if not isinstance(call.func, ast.Name):
            return None
        callee = call.func.id
        if callee not in helpers:
            return None
        if call.keywords or len(call.args) != 1:
            return None
        out.append(_HelperCall(helper_name=callee, arg=call.args[0]))
    return out


def _classify_helper_inlining(
    calls: List[_HelperCall],
    helpers: Dict[str, _HelperDef],
    test_name: str,
    source_path: str,
    out: Layer2Output,
) -> None:
    out.claimed_tests.add(test_name)
    for i, call in enumerate(calls):
        out.seen += 1
        memento_name = f"{test_name}::call::{i}"
        helper = helpers[call.helper_name]
        try:
            arg_term = _translate_term(call.arg)
        except ValueError as e:
            out.helper_inlined_skipped += 1
            out.warnings.append(
                LiftWarning(source_path, memento_name,
                            f"layer2 helper-inline: argument not liftable: {e}")
            )
            continue
        try:
            raw = _lift_assertion_stmt(helper.assertion)
        except ValueError as e:
            out.helper_inlined_skipped += 1
            out.warnings.append(
                LiftWarning(source_path, memento_name,
                            f"layer2 helper-inline: helper `{call.helper_name}` body not liftable: {e}")
            )
            continue
        inlined = subst_var_in_formula(raw, helper.param_name, arg_term)
        out.decls.append(ContractDecl(name=memento_name, inv=inlined))
        out.lifted += 1
        out.helper_inlined_lifted += 1


# ---------------------------------------------------------------------------
# PATTERN 3: multi-assertion characterization
# ---------------------------------------------------------------------------


def _classify_characterization(
    asserts: Sequence[ast.stmt],
    test_name: str,
    source_path: str,
    out: Layer2Output,
) -> None:
    out.claimed_tests.add(test_name)
    out.seen += 1

    atoms: List[Formula] = []
    skipped: List[str] = []
    for i, stmt in enumerate(asserts):
        try:
            atoms.append(_lift_assertion_stmt(stmt))
        except ValueError as e:
            skipped.append(f"#{i}: {e}")

    if len(atoms) < 2:
        out.claimed_tests.discard(test_name)
        out.characterization_skipped += 1
        out.warnings.append(
            LiftWarning(source_path, test_name,
                        f"layer2 characterization: only {len(atoms)} of {len(asserts)} asserts were liftable; releasing to layer 0")
        )
        return

    out.decls.append(ContractDecl(name=test_name, inv=and_(atoms)))
    out.lifted += 1
    out.characterization_lifted += 1
    if skipped:
        out.warnings.append(
            LiftWarning(source_path, test_name,
                        f"layer2 characterization: {len(skipped)} atoms skipped from conjunction: {'; '.join(skipped)}")
        )


# ---------------------------------------------------------------------------
# PATTERN 4: @pytest.mark.parametrize
# ---------------------------------------------------------------------------


@dataclass
class _ParametrizeMark:
    """A literal-arg-list ``parametrize`` decorator we can lift."""
    param_names: List[str]   # ["x"] or ["x", "y"]
    rows: List[List[ast.expr]]   # each row is a list of arg exprs (one per param_name)


@dataclass
class _ParametrizePresence:
    """Records that a parametrize decorator is present, alongside the
    parsed mark (or None if the decorator is malformed / non-literal so
    we want the lift path to warn rather than silently fall through).
    """
    mark: Optional[_ParametrizeMark]
    reason: Optional[str]  # set when mark is None


def _find_parametrize(fn: ast.FunctionDef) -> Optional[_ParametrizePresence]:
    """Return None iff no parametrize decorator at all. Otherwise return
    a presence record; ``mark`` may be None when the decorator is present
    but its arg shape isn't liftable.
    """
    for dec in fn.decorator_list:
        if not isinstance(dec, ast.Call):
            continue
        if not _is_parametrize_attr(dec.func):
            continue
        if len(dec.args) < 2:
            return _ParametrizePresence(None, "parametrize: expected at least 2 positional args")
        names_node = dec.args[0]
        rows_node = dec.args[1]
        param_names = _parse_parametrize_names(names_node)
        if param_names is None:
            return _ParametrizePresence(None, "parametrize: param-names arg is not a literal string or list of strings")
        rows = _parse_parametrize_rows(rows_node, len(param_names))
        if rows is None:
            return _ParametrizePresence(None, "parametrize: row arg is not a literal list of leaf values")
        return _ParametrizePresence(_ParametrizeMark(param_names=param_names, rows=rows), None)
    return None


def _is_parametrize_attr(func: ast.expr) -> bool:
    # bare ``parametrize`` (rare)
    if isinstance(func, ast.Name) and func.id == "parametrize":
        return True
    # ``mark.parametrize``
    if (
        isinstance(func, ast.Attribute)
        and func.attr == "parametrize"
        and isinstance(func.value, ast.Name)
        and func.value.id == "mark"
    ):
        return True
    # ``pytest.mark.parametrize``
    if (
        isinstance(func, ast.Attribute)
        and func.attr == "parametrize"
        and isinstance(func.value, ast.Attribute)
        and func.value.attr == "mark"
        and isinstance(func.value.value, ast.Name)
        and func.value.value.id == "pytest"
    ):
        return True
    return False


def _parse_parametrize_names(node: ast.expr) -> Optional[List[str]]:
    if isinstance(node, ast.Constant) and isinstance(node.value, str):
        # "x" or "x, y" or "x,y"
        parts = [p.strip() for p in node.value.split(",")]
        parts = [p for p in parts if p]
        if not parts:
            return None
        return parts
    if isinstance(node, (ast.List, ast.Tuple)):
        out: List[str] = []
        for el in node.elts:
            if isinstance(el, ast.Constant) and isinstance(el.value, str):
                out.append(el.value)
            else:
                return None
        return out or None
    return None


def _parse_parametrize_rows(node: ast.expr, n_params: int) -> Optional[List[List[ast.expr]]]:
    """Strictly literal-leaf rows: int / str / bool / None / unary-neg-int.
    A row containing a call (e.g., ``some_helper()``) is rejected here so
    the calling pattern can warn rather than silently lift a free-var Ctor
    that the verifier would later reject.
    """
    if not isinstance(node, ast.List):
        return None
    rows: List[List[ast.expr]] = []
    for el in node.elts:
        if n_params == 1:
            if not _is_literal_leaf(el):
                return None
            rows.append([el])
        else:
            if not isinstance(el, (ast.Tuple, ast.List)):
                return None
            if len(el.elts) != n_params:
                return None
            if not all(_is_literal_leaf(x) for x in el.elts):
                return None
            rows.append(list(el.elts))
    return rows


def _is_literal_leaf(node: ast.expr) -> bool:
    if isinstance(node, ast.Constant):
        v = node.value
        return isinstance(v, (int, str, bool)) or v is None
    if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.USub):
        return (
            isinstance(node.operand, ast.Constant)
            and isinstance(node.operand.value, int)
            and not isinstance(node.operand.value, bool)
        )
    return False


def _classify_parametrize(
    fn: ast.FunctionDef,
    body: Sequence[ast.stmt],
    pmark: _ParametrizeMark,
    test_name: str,
    source_path: str,
    out: Layer2Output,
) -> None:
    out.claimed_tests.add(test_name)
    out.seen += 1

    # Body must reduce to a single liftable assertion. Multi-stmt bodies skip.
    if len(body) != 1 or not _is_assertion_stmt(body[0]):
        out.parametrize_skipped += 1
        out.warnings.append(
            LiftWarning(source_path, test_name,
                        "layer2 parametrize: body must be a single liftable assertion in v0")
        )
        return

    try:
        raw = _lift_assertion_stmt(body[0])
    except ValueError as e:
        out.parametrize_skipped += 1
        out.warnings.append(
            LiftWarning(source_path, test_name,
                        f"layer2 parametrize: body assertion not liftable: {e}")
        )
        return

    # Substitute each row.
    row_atoms: List[Formula] = []
    for row in pmark.rows:
        f = raw
        for pname, arg_node in zip(pmark.param_names, row):
            try:
                arg_term = _translate_term(arg_node)
            except ValueError as e:
                out.parametrize_skipped += 1
                out.warnings.append(
                    LiftWarning(source_path, test_name,
                                f"layer2 parametrize: row arg not liftable: {e}")
                )
                return
            f = subst_var_in_formula(f, pname, arg_term)
        row_atoms.append(f)

    if not row_atoms:
        out.parametrize_skipped += 1
        out.warnings.append(
            LiftWarning(source_path, test_name,
                        "layer2 parametrize: empty row list; nothing to lift")
        )
        return

    folded: Formula = and_(row_atoms) if len(row_atoms) > 1 else row_atoms[0]
    suffix = "_".join(pmark.param_names)
    memento_name = f"{test_name}::parametrize::{suffix}"
    out.decls.append(ContractDecl(name=memento_name, inv=folded))
    out.lifted += 1
    out.parametrize_lifted += 1


# ---------------------------------------------------------------------------
# PATTERN 5: callsite-scoped value facts from tests
# ---------------------------------------------------------------------------


def _classify_value_scope(
    body: Sequence[ast.stmt],
    test_name: str,
    source_path: str,
    out: Layer2Output,
) -> bool:
    scopes: List[_ValueScope] = [_ValueScope()]
    versions: Dict[str, int] = {}
    decls: List[ContractDecl] = []
    implications: List[ImplicationDecl] = []
    used_names: Set[str] = set()
    assertion_index = 0

    for stmt in body:
        if _is_assertion_stmt(stmt):
            made = _emit_value_scope_assertion(
                stmt,
                scopes,
                test_name,
                assertion_index,
                source_path,
                decls,
                implications,
                used_names,
            )
            assertion_index += 1
            if made:
                continue
            if not decls:
                return False
            continue

        next_scopes = _apply_value_scope_statement(stmt, scopes, versions)
        if next_scopes is None:
            return False
        scopes = next_scopes

    if not implications:
        return False

    out.claimed_tests.add(test_name)
    out.seen += 1
    out.decls.extend(decls)
    out.implications.extend(implications)
    out.lifted += len(decls)
    out.value_scope_lifted += 1
    return True


def _apply_value_scope_statement(
    stmt: ast.stmt,
    scopes: List[_ValueScope],
    versions: Dict[str, int],
) -> Optional[List[_ValueScope]]:
    if isinstance(stmt, ast.Assign):
        if len(stmt.targets) != 1 or not isinstance(stmt.targets[0], ast.Name):
            return None
        return _apply_value_scope_binding(stmt.targets[0].id, stmt.value, scopes, versions)

    if isinstance(stmt, ast.AnnAssign):
        if stmt.value is None or not isinstance(stmt.target, ast.Name):
            return None
        return _apply_value_scope_binding(stmt.target.id, stmt.value, scopes, versions)

    if isinstance(stmt, ast.If):
        return _apply_value_scope_if(stmt, scopes, versions)

    if isinstance(stmt, ast.Pass):
        return scopes

    return None


def _apply_value_scope_if(
    stmt: ast.If,
    scopes: List[_ValueScope],
    versions: Dict[str, int],
) -> Optional[List[_ValueScope]]:
    out: List[_ValueScope] = []
    for scope in scopes:
        guard = _lift_branch_guard(stmt.test, scope)

        then_scope = scope.copy()
        then_scope.facts.append(guard)
        then_scopes = _apply_value_scope_block(stmt.body, [then_scope], versions)
        if then_scopes is None:
            return None
        out.extend(then_scopes)

        else_scope = scope.copy()
        else_scope.facts.append(not_(guard))
        else_scopes = _apply_value_scope_block(stmt.orelse, [else_scope], versions)
        if else_scopes is None:
            return None
        out.extend(else_scopes or [else_scope])

    return out


def _apply_value_scope_block(
    stmts: Sequence[ast.stmt],
    scopes: List[_ValueScope],
    versions: Dict[str, int],
) -> Optional[List[_ValueScope]]:
    current = scopes
    for stmt in stmts:
        if _is_assertion_stmt(stmt):
            return None
        next_scopes = _apply_value_scope_statement(stmt, current, versions)
        if next_scopes is None:
            return None
        current = next_scopes
    return current


def _apply_value_scope_binding(
    name: str,
    value: ast.expr,
    scopes: List[_ValueScope],
    versions: Dict[str, int],
) -> List[_ValueScope]:
    out: List[_ValueScope] = []
    for scope in scopes:
        next_scope = scope.copy()
        try:
            rhs = _translate_term_scoped(value, scope)
        except ValueError:
            next_scope.current.pop(name, None)
            next_scope.origins.pop(name, None)
            out.append(next_scope)
            continue

        version = versions.get(name, 0)
        versions[name] = version + 1
        ssa_name = f"{name}${version}"
        ssa = make_var(ssa_name)
        next_scope.current[name] = ssa
        origin = _call_origin_from_expr(value)
        if origin is not None:
            next_scope.origins[name] = origin
        else:
            next_scope.origins.pop(name, None)
        next_scope.facts.append(eq(ssa, rhs))
        out.append(next_scope)
    return out


def _emit_value_scope_assertion(
    stmt: ast.stmt,
    scopes: List[_ValueScope],
    test_name: str,
    assertion_index: int,
    source_path: str,
    decls: List[ContractDecl],
    implications: List[ImplicationDecl],
    used_names: Set[str],
) -> int:
    made = 0
    for scope in scopes:
        context = _assertion_callsite_context(stmt, scope)
        if context is None:
            continue
        origins, facts, assertion = context

        for origin in origins:
            base = _callsite_contract_base(origin, source_path)
            facts_name = _unique_contract_name(f"{base}::facts", used_names)
            assertion_name = _unique_contract_name(f"{base}::assertion", used_names)
            implication_name = _unique_contract_name(
                f"{base}::facts-implies-assertion", used_names
            )
            fact_formula = facts[0] if len(facts) == 1 else and_(facts)
            decls.append(ContractDecl(name=facts_name, inv=fact_formula))
            decls.append(ContractDecl(name=assertion_name, inv=assertion))
            implications.append(
                ImplicationDecl(
                    name=implication_name,
                    antecedent=facts_name,
                    consequent=assertion_name,
                    proof_witness=f"{test_name} assertion {assertion_index}",
                )
            )
            made += 1
    return made


def _assertion_callsite_context(
    stmt: ast.stmt,
    scope: _ValueScope,
) -> Optional[Tuple[List[_CallOrigin], List[Formula], Formula]]:
    direct_calls = _collect_assertion_calls(stmt)
    call_vars: Dict[Tuple[int, int], Term] = {}
    for call in direct_calls:
        origin = _call_origin_from_expr(call)
        if origin is not None:
            call_vars[_call_key(call)] = make_var(_call_result_var_name(origin))

    transient_facts: List[Formula] = []
    direct_origins: List[_CallOrigin] = []
    for call in direct_calls:
        origin = _call_origin_from_expr(call)
        if origin is None:
            continue
        try:
            rhs = _translate_call_rhs(call, scope, call_vars)
        except ValueError:
            continue
        var_term = call_vars[_call_key(call)]
        transient_facts.append(eq(var_term, rhs))
        direct_origins.append(origin)

    origins = _unique_origins(_origins_for_assertion(stmt, scope) + direct_origins)
    if not origins:
        return None

    facts = scope.facts + transient_facts
    if not facts:
        return None

    try:
        assertion = _lift_assertion_stmt_scoped(stmt, scope, call_vars)
    except ValueError:
        return None

    return origins, facts, assertion


def _unique_contract_name(name: str, used_names: Set[str]) -> str:
    if name not in used_names:
        used_names.add(name)
        return name
    i = 1
    while f"{name}::{i}" in used_names:
        i += 1
    unique = f"{name}::{i}"
    used_names.add(unique)
    return unique


def _callsite_contract_base(origin: _CallOrigin, source_path: str) -> str:
    file_name = os.path.basename(source_path) or source_path or "<unknown>"
    return f"{origin.callee}@{file_name}:{origin.lineno}:{origin.col}"


def _call_result_var_name(origin: _CallOrigin) -> str:
    return f"{origin.callee}$call${origin.lineno}${origin.col}"


def _call_key(call: ast.Call) -> Tuple[int, int]:
    return (getattr(call, "lineno", 0), getattr(call, "col_offset", 0))


def _call_origin_from_expr(node: ast.expr) -> Optional[_CallOrigin]:
    if (
        isinstance(node, ast.Call)
        and isinstance(node.func, ast.Name)
        and not node.keywords
    ):
        return _CallOrigin(
            callee=node.func.id,
            lineno=getattr(node, "lineno", 0),
            col=getattr(node, "col_offset", 0),
        )
    return None


def _assertion_value_exprs(stmt: ast.stmt) -> List[ast.expr]:
    exprs: List[ast.expr] = []
    if isinstance(stmt, ast.Assert):
        exprs.append(stmt.test)
    elif isinstance(stmt, ast.Expr) and isinstance(stmt.value, ast.Call):
        name = _attr_method_name(stmt.value.func)
        if name in _UNITTEST_BINARY_PREDICATES:
            exprs.extend(stmt.value.args[:2])
        elif name in _UNITTEST_NONE_PREDICATES and stmt.value.args:
            exprs.append(stmt.value.args[0])
        elif name in _UNITTEST_TRUTH_PREDICATES and stmt.value.args:
            exprs.append(stmt.value.args[0])
    return exprs


def _collect_assertion_calls(stmt: ast.stmt) -> List[ast.Call]:
    calls: List[ast.Call] = []
    seen: Set[Tuple[int, int]] = set()
    for expr in _assertion_value_exprs(stmt):
        for node in ast.walk(expr):
            if not isinstance(node, ast.Call):
                continue
            if _call_origin_from_expr(node) is None:
                continue
            key = _call_key(node)
            if key in seen:
                continue
            seen.add(key)
            calls.append(node)
    return sorted(calls, key=_call_key)


def _origins_for_assertion(stmt: ast.stmt, scope: _ValueScope) -> List[_CallOrigin]:
    origins: List[_CallOrigin] = []
    for expr in _assertion_value_exprs(stmt):
        for node in ast.walk(expr):
            if isinstance(node, ast.Name) and node.id in scope.origins:
                origins.append(scope.origins[node.id])
    return _unique_origins(origins)


def _unique_origins(origins: List[_CallOrigin]) -> List[_CallOrigin]:
    out: List[_CallOrigin] = []
    seen: Set[Tuple[str, int, int]] = set()
    for origin in origins:
        key = (origin.callee, origin.lineno, origin.col)
        if key in seen:
            continue
        seen.add(key)
        out.append(origin)
    return out


def _translate_call_rhs(
    call: ast.Call,
    scope: _ValueScope,
    call_vars: Dict[Tuple[int, int], Term],
) -> Term:
    if not isinstance(call.func, ast.Name):
        raise ValueError("call target must be a simple name")
    if call.keywords:
        raise ValueError("call with kwargs is not liftable")
    return ctor(
        call.func.id,
        [_translate_term_scoped(arg, scope, call_vars) for arg in call.args],
    )


def _lift_branch_guard(node: ast.expr, scope: _ValueScope) -> Formula:
    try:
        return _translate_bool_expr_scoped(node, scope)
    except ValueError:
        return atomic("python_branch_condition", [str_const(_unparse(node))])


def _lift_assertion_stmt_scoped(
    stmt: ast.stmt,
    scope: _ValueScope,
    call_vars: Optional[Dict[Tuple[int, int], Term]] = None,
) -> Formula:
    if isinstance(stmt, ast.Assert):
        return _translate_bool_expr_scoped(stmt.test, scope, call_vars)
    if isinstance(stmt, ast.Expr) and isinstance(stmt.value, ast.Call):
        call = stmt.value
        name = _attr_method_name(call.func)
        if name in _UNITTEST_BINARY_PREDICATES:
            if len(call.args) < 2:
                raise ValueError(f"{name} expects at least 2 positional args")
            l = _translate_term_scoped(call.args[0], scope, call_vars)
            r = _translate_term_scoped(call.args[1], scope, call_vars)
            return comparison_with_none_guard(
                _UNITTEST_BINARY_PREDICATES[name],
                l,
                r,
                emit_none_guard=False,
            )
        if name in _UNITTEST_NONE_PREDICATES:
            if len(call.args) < 1:
                raise ValueError(f"{name} expects 1 positional arg")
            t = _translate_term_scoped(call.args[0], scope, call_vars)
            return comparison_with_none_guard(
                _UNITTEST_NONE_PREDICATES[name],
                t,
                ctor("None", []),
                emit_none_guard=True,
            )
        if name == "assertTrue":
            if len(call.args) < 1:
                raise ValueError("assertTrue expects 1 positional arg")
            return _translate_bool_expr_scoped(call.args[0], scope, call_vars)
        if name == "assertFalse":
            if len(call.args) < 1:
                raise ValueError("assertFalse expects 1 positional arg")
            return not_(_translate_bool_expr_scoped(call.args[0], scope, call_vars))
    raise ValueError("statement is not a value-scope assertion")


def _translate_bool_expr_scoped(
    node: ast.expr,
    scope: _ValueScope,
    call_vars: Optional[Dict[Tuple[int, int], Term]] = None,
) -> Formula:
    if isinstance(node, ast.Compare):
        if len(node.ops) != 1 or len(node.comparators) != 1:
            raise ValueError("only single comparisons are liftable")
        return _comparison_from_ast_op(
            node.ops[0],
            _translate_term_scoped(node.left, scope, call_vars),
            _translate_term_scoped(node.comparators[0], scope, call_vars),
        )
    if isinstance(node, ast.BoolOp):
        operands = [
            _translate_bool_expr_scoped(v, scope, call_vars) for v in node.values
        ]
        if isinstance(node.op, ast.And):
            return and_(operands)
        if isinstance(node.op, ast.Or):
            return connective("or", operands)
    if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.Not):
        return not_(_translate_bool_expr_scoped(node.operand, scope, call_vars))
    term = _translate_term_scoped(node, scope, call_vars)
    return eq(term, bool_const(True))


def _translate_term_scoped(
    node: ast.expr,
    scope: _ValueScope,
    call_vars: Optional[Dict[Tuple[int, int], Term]] = None,
) -> Term:
    call_vars = call_vars or {}
    if isinstance(node, ast.Name):
        if node.id in scope.current:
            return scope.current[node.id]
        return _translate_term(node)
    if isinstance(node, ast.Constant):
        return _translate_term(node)
    if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.USub):
        return _translate_term(node)
    if isinstance(node, ast.Call):
        if not isinstance(node.func, ast.Name):
            raise ValueError("call target must be a simple name")
        if node.keywords:
            raise ValueError("call with kwargs is not liftable")
        key = _call_key(node)
        if key in call_vars:
            return call_vars[key]
        return ctor(
            node.func.id,
            [_translate_term_scoped(arg, scope, call_vars) for arg in node.args],
        )
    if isinstance(node, ast.BinOp):
        op = _BINOP_TERM_NAMES.get(type(node.op))
        if op is None:
            raise ValueError("unsupported binary operator")
        return ctor(
            op,
            [
                _translate_term_scoped(node.left, scope, call_vars),
                _translate_term_scoped(node.right, scope, call_vars),
            ],
        )
    raise ValueError("expression shape not in value-scope lift whitelist")


_BINOP_TERM_NAMES = {
    ast.Add: "+",
    ast.Sub: "-",
    ast.Mult: "*",
    ast.Div: "/",
    ast.Mod: "%",
}


def _unparse(node: ast.AST) -> str:
    try:
        return ast.unparse(node)  # type: ignore[attr-defined]
    except Exception:
        return node.__class__.__name__
