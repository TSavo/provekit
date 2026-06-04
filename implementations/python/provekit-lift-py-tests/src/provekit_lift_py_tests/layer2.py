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
    mixed_body_lifted: int = 0
    mixed_body_skipped: int = 0
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
    for fn, class_name in test_fns:
        _classify_and_lift(fn, source_path, helpers, out, class_name=class_name)
    return out


# ---------------------------------------------------------------------------
# Test-function recognition
# ---------------------------------------------------------------------------


def _iter_test_functions(tree: ast.AST):
    """Yield (FunctionDef, class_name_or_None) pairs for test functions:
    free function ``test_*`` OR method ``test_*`` on a class.

    Class name is the enclosing class so the caller can qualify the decl
    name as ``ClassName::test_method`` and keep each class-method's
    scope isolated from same-named methods in other classes.
    Methods inside a class carry ``self`` (or ``cls``) as their first
    arg which ``_strip_self`` strips before classification.
    """
    # Build a mapping from function-node id -> enclosing class name.
    # ast.walk yields nodes in no particular order, so we do a targeted
    # traversal of top-level and nested statements to find ClassDef bodies.
    class_of: Dict[int, str] = {}
    for node in ast.walk(tree):
        if isinstance(node, ast.ClassDef):
            for child in node.body:
                if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    class_of[id(child)] = node.name

    for node in ast.walk(tree):
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            if node.name.startswith("test"):
                yield node, class_of.get(id(node))


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
    class_name: Optional[str] = None,
) -> None:
    # Qualify the test name with the enclosing class so that same-named
    # methods in different classes produce independent decl scopes.
    # ``TestFoo::test_bar`` is distinct from ``TestBaz::test_bar`` even
    # though both have ``fn.name == "test_bar"``.  Free functions keep
    # their bare name.
    test_name = f"{class_name}::{fn.name}" if class_name else fn.name
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

    if _classify_mixed_body(body, test_name, source_path, out):
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
}

_UNITTEST_IDENTITY_PREDICATES = {
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
            if name is not None and name in _UNITTEST_IDENTITY_PREDICATES:
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
            or name in _UNITTEST_IDENTITY_PREDICATES
            or name in _UNITTEST_NONE_PREDICATES
            or name in _UNITTEST_TRUTH_PREDICATES
        ):
            continue
        out.append(name)
    return out


# ---------------------------------------------------------------------------
# Term and formula translation
# ---------------------------------------------------------------------------


def _dotted_attr_path(node: ast.expr) -> Optional[str]:
    """Extract the dotted path from an attribute-access chain.

    ``out.bounded_loop_lifted`` -> ``"out.bounded_loop_lifted"``.
    ``a.b.c`` -> ``"a.b.c"``.
    Returns None if the base is not a simple Name (e.g. ``f().attr``).
    """
    if isinstance(node, ast.Name):
        return node.id
    if isinstance(node, ast.Attribute):
        base = _dotted_attr_path(node.value)
        if base is None:
            return None
        return f"{base}.{node.attr}"
    return None


def _ssa_dotted_attr_path(node: ast.Attribute, scope: "_ValueScope") -> Optional[str]:
    """Build a dotted attribute path whose base is SSA-keyed.

    Walks the attribute chain to find the leftmost Name base.  If that
    base is tracked in ``scope.current`` (i.e. it is an SSA-renamed
    local variable), replace it with its SSA name and return the
    reassembled path.  Otherwise return None so the caller falls back
    to the unscoped (raw) dotted path.

    Examples with ``scope.current = {"out": make_var("out$1")}``:
      ``out.val``   -> ``"out$1.val"``
      ``out.a.b``   -> ``"out$1.a.b"``
      ``other.val`` -> None  (``other`` not in scope)
    """
    raw = _dotted_attr_path(node)
    if raw is None:
        return None
    # Find the leftmost Name segment (root of the dotted chain).
    root: ast.expr = node
    while isinstance(root, ast.Attribute):
        root = root.value
    if not isinstance(root, ast.Name):
        return None
    root_name = root.id
    if root_name not in scope.current:
        return None
    ssa_term = scope.current[root_name]
    # Extract the SSA var name from the Term (always a _Var here).
    from .ir import _Var as _IrVar
    if not isinstance(ssa_term, _IrVar):
        return None
    ssa_base = ssa_term.name
    # Replace the root segment with its SSA name.  The raw path starts
    # with root_name; everything after the root is the suffix.
    suffix = raw[len(root_name):]   # e.g. ".val" or ".a.b" or ""
    return f"{ssa_base}{suffix}"


def _subscript_key_term(key_node: ast.expr) -> Term:
    """Translate a subscript key to a Term, for use in the ``subscript`` Ctor.

    ONLY literal keys are supported: string, integer, bool, None.  A
    non-literal / computed key (``parsed[i]``, ``parsed[CONST]``) raises
    ValueError with an explicit message — these must LOUDLY REFUSE because
    we cannot soundly establish that two subscript expressions refer to the
    same slot without knowing the key's value.

    Design: literal string keys produce ``str_const(s)`` (encoded as
    ``strlit_<hash>`` in SMT, sort-safe Int constant).  Integer and bool keys
    produce ``num``/``bool_const`` concrete Int values.  None produces the
    ``None`` nullary ctor.  All of these reuse the existing literal-encoding
    machinery, so cross-type distinctness axioms are emitted correctly.
    """
    if isinstance(key_node, ast.Constant):
        v = key_node.value
        if isinstance(v, bool):
            return bool_const(v)
        if isinstance(v, int):
            return num(v)
        if isinstance(v, str):
            return str_const(v)
        if v is None:
            return ctor("None", [])
        raise ValueError(
            f"subscript-index: non-literal key type {type(v)!r} is not liftable"
        )
    if isinstance(key_node, ast.UnaryOp) and isinstance(key_node.op, ast.USub):
        if (
            isinstance(key_node.operand, ast.Constant)
            and isinstance(key_node.operand.value, int)
            and not isinstance(key_node.operand.value, bool)
        ):
            return num(-key_node.operand.value)
    # Non-literal key: LOUD REFUSE.  We cannot establish slot identity.
    raise ValueError(
        "subscript-index: non-literal key (computed key or variable) is not "
        "soundly liftable — cannot establish that two subscript expressions "
        "refer to the same slot without knowing the key value at lift time"
    )


def _translate_subscript_term(node: ast.Subscript, base_term: Term) -> Term:
    """Build ``ctor('subscript', [base_term, key_term])`` for a subscript node.

    The ``subscript`` Ctor is an uninterpreted 2-arg function in SMT.
    Sound because:
      - same base + same key  -> same Ctor term -> same-subject contradictions fire UNSAT
      - different key or base -> distinct Ctor terms -> independent facts
      - attribute ``obj.attr`` and subscript ``obj['attr']`` NEVER share a term
        (attribute uses a raw Var; subscript uses a Ctor) — no accidental aliasing.

    Non-literal keys raise ValueError (LOUD REFUSE); see ``_subscript_key_term``.
    """
    key_term = _subscript_key_term(node.slice)
    return ctor("subscript", [base_term, key_term])


def _translate_term(node: ast.expr) -> Term:
    """Whitelist:
      - identifier (Var)
      - integer / string / bool literal
      - unary-neg of an integer literal
      - single-arg call (Ctor with one arg)
      - attribute access (Var named by dotted path, e.g. ``obj.attr``)
      - subscript-index with a literal key (``obj['key']``, ``obj[0]``)
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
    if isinstance(node, ast.Attribute):
        # Attribute access (``obj.attr``): lift as an opaque Var named by the
        # dotted path.  The subject ``obj`` must itself reduce to a simple Var
        # or attribute chain so the resulting dotted name is stable and unique.
        # Sound because: same dotted path -> same Var name -> same-term
        # contradictions fire UNSAT; different paths remain independent.
        # Restriction: no method calls (``obj.method()``); those must go
        # through the Call branch with a simple-Name func.
        obj_name = _dotted_attr_path(node)
        if obj_name is None:
            raise ValueError(
                "attribute access on a non-name/non-attribute subject is not liftable"
            )
        return make_var(obj_name)
    if isinstance(node, ast.Subscript):
        # Subscript-index (``obj['key']``, ``obj[0]``): lift as
        # ``ctor('subscript', [base_term, key_term])``.  The base is
        # recursively translated so nested subscripts (``a['b']['c']``) and
        # attribute-then-subscript (``a.b['c']``) chain naturally.
        # Non-literal keys LOUDLY REFUSE via ``_subscript_key_term``.
        base_term = _translate_term(node.value)
        return _translate_subscript_term(node, base_term)
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


def _comparison_from_identity_symbol(sym: str, left: Term, right: Term) -> Formula:
    if _is_none_term(left) == _is_none_term(right):
        raise ValueError("identity comparison is only supported against None")
    return comparison_with_none_guard(
        sym,
        left,
        right,
        emit_none_guard=True,
    )


def _comparison_from_ast_op(op: ast.cmpop, left: Term, right: Term) -> Formula:
    identity_sym = _IDENTITY_OP_MAP.get(type(op))
    if identity_sym is not None:
        return _comparison_from_identity_symbol(identity_sym, left, right)
    # Membership: ``x in coll`` -> ``member(x,coll)``; ``x not in coll`` ->
    # ``not(member(x,coll))``.  Using the SAME predicate symbol for both forms
    # is intentional and necessary for discrimination: ``member(x,c) ∧
    # ¬member(x,c)`` is propositionally UNSAT without any theory; z3 discharges
    # it trivially.  The SMT emitter declares unknown predicates as uninterpreted
    # Bool fns (``(declare-fun member (Int Int) Bool)``), so no Undecidable
    # result fires.  ASCII name chosen deliberately: the Unicode ``∈`` (U+2208)
    # is not a valid SMT-LIB simple-symbol char and z3 rejects it with a parse
    # error -- the same class of bug as the bare-string-literal encoding before
    # the strlit_<hash> fix.
    if isinstance(op, ast.In):
        return atomic("member", [left, right])
    if isinstance(op, ast.NotIn):
        return not_(atomic("member", [left, right]))
    sym = _COMPARE_OP_MAP.get(type(op))
    if sym is None:
        raise ValueError(f"unsupported comparison op: {type(op).__name__}")
    return comparison_with_none_guard(sym, left, right, emit_none_guard=False)


def _truthiness_call_head(callee: str, arity: int) -> str:
    """Build an ASCII-safe, arity-stable SMT predicate head for a truthiness
    call assertion.

    Shape: ``call_<callee>_a<arity>`` where ``<callee>`` is the callee name
    with non-alphanumeric chars replaced by ``_``, and ``<arity>`` is the
    total number of SMT arguments (receiver counts as arg 0 for method calls).

    Encoding arity in the name guarantees that the SMT emitter never sees two
    declarations with the same head at different arities (which would silently
    adopt the first arity, producing ill-sorted applications).
    """
    safe = "".join(c if (c.isascii() and c.isalnum()) else "_" for c in callee)
    return f"call_{safe}_a{arity}"


def _translate_truthiness_call_formula(
    node: ast.Call,
    translate_term_fn,  # callable: ast.expr -> Term
) -> Formula:
    """Lift a bare call expression used as a boolean assertion to an
    UNINTERPRETED predicate atom.

    Handles two shapes:
      - Method call: ``recv.method(args...)``  -> ``atomic("call_method_a<n>", [recv_term, arg_terms...])``
      - Function call: ``func(args...)``        -> ``atomic("call_func_a<n>", [arg_terms...])``

    Special case: ``isinstance(x, T)`` is LOUDLY REFUSED because it requires
    a type-lattice to be sound (``isinstance(x, int)`` and ``isinstance(x, str)``
    are disjoint-contradictory in Python, but an uninterpreted predicate would
    call them consistent = falsePass).

    Any argument that cannot be translated by ``translate_term_fn`` raises
    ValueError (LOUD REFUSE at call site, never silent).
    """
    if isinstance(node.func, ast.Name):
        callee = node.func.id
        # isinstance: loud refuse — needs type-lattice for soundness.
        if callee == "isinstance":
            raise ValueError(
                "isinstance: needs type-lattice to be sound (isinstance(x,int) and "
                "isinstance(x,str) are disjoint-contradictory in Python but an "
                "uninterpreted predicate would call them consistent = falsePass); "
                "deferred to type-lattice lifter"
            )
        # Regular function call.
        if node.keywords:
            raise ValueError(
                f"call `{callee}` with keyword args is not soundly liftable "
                "(keyword args cannot be order-stably translated without knowing "
                "the function signature)"
            )
        arg_terms = []
        for i, arg in enumerate(node.args):
            try:
                arg_terms.append(translate_term_fn(arg))
            except ValueError as e:
                raise ValueError(
                    f"call `{callee}` arg[{i}] not liftable: {e}"
                )
        head = _truthiness_call_head(callee, len(arg_terms))
        return atomic(head, arg_terms)

    if isinstance(node.func, ast.Attribute):
        method = node.func.attr
        recv_node = node.func.value
        if node.keywords:
            raise ValueError(
                f"method call `{method}` with keyword args is not soundly liftable"
            )
        try:
            recv_term = translate_term_fn(recv_node)
        except ValueError as e:
            raise ValueError(
                f"method call `{method}` receiver not liftable: {e}"
            )
        arg_terms = [recv_term]
        for i, arg in enumerate(node.args):
            try:
                arg_terms.append(translate_term_fn(arg))
            except ValueError as e:
                raise ValueError(
                    f"method call `{method}` arg[{i}] not liftable: {e}"
                )
        head = _truthiness_call_head(method, len(arg_terms))
        return atomic(head, arg_terms)

    raise ValueError(
        "call with non-Name/non-Attribute func is not liftable as a truthiness predicate"
    )


def _translate_bool_expr(node: ast.expr) -> Formula:
    """``assert <expr>``: only a single-comparison or truthiness-call
    expression is liftable.

    Handles:
      - ``assert <comparison>``        -> comparison formula
      - ``assert <call>``              -> uninterpreted predicate atom (TRUTHINESS)
      - ``assert not <call>``          -> not_(predicate atom)
      - ``assert not <comparison>``    -> not_(comparison formula)
    """
    if isinstance(node, ast.Compare):
        if len(node.ops) != 1 or len(node.comparators) != 1:
            raise ValueError("only single comparisons are liftable (no chained `a < b < c`)")
        l = _translate_term(node.left)
        r = _translate_term(node.comparators[0])
        return _comparison_from_ast_op(node.ops[0], l, r)
    if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.Not):
        inner = _translate_bool_expr(node.operand)
        return not_(inner)
    if isinstance(node, ast.Call):
        return _translate_truthiness_call_formula(node, _translate_term)
    if isinstance(node, ast.NamedExpr):
        # Walrus inside an assert: skip.
        raise ValueError("walrus operator in assert is not liftable")
    raise ValueError("assert body must be a comparison expression or a call/not-call")


def _lift_assertion_stmt(stmt: ast.stmt) -> Formula:
    """Translate a recognized assertion statement to a Formula. Raises
    ValueError if the stmt's surface looked liftable but a leaf isn't.
    """
    if isinstance(stmt, ast.Assert):
        return _translate_bool_expr(stmt.test)
    if isinstance(stmt, ast.Expr) and isinstance(stmt.value, ast.Call):
        call = stmt.value
        name = _attr_method_name(call.func)
        if name in _UNITTEST_IDENTITY_PREDICATES:
            if len(call.args) < 2:
                raise ValueError(f"{name} expects at least 2 positional args")
            l = _translate_term(call.args[0])
            r = _translate_term(call.args[1])
            return _comparison_from_identity_symbol(
                _UNITTEST_IDENTITY_PREDICATES[name],
                l,
                r,
            )
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
        skip_detail = f"; skipped: {'; '.join(skipped)}" if skipped else ""
        out.warnings.append(
            LiftWarning(source_path, test_name,
                        f"layer2 characterization: only {len(atoms)} of {len(asserts)} asserts were liftable; releasing to layer 0{skip_detail}")
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
    # Accumulate assertion formulas per call-site base so we can emit ONE
    # conjoined ::assertion contract per base (same shape as Pattern 3's
    # pre-conjoined ContractDecl). This is what makes same-subject
    # contradictions visible to the consistency pass without any CLI-side
    # conjoin logic: and(=(y,None), ≠(y,None)) lands in one memento -> UNSAT.
    # Order is preserved so the conjunction is deterministic.
    assertion_atoms_by_base: Dict[str, List[Formula]] = {}
    base_order: List[str] = []

    for stmt in body:
        if _is_assertion_stmt(stmt):
            pairs = _collect_value_scope_assertion_facts(
                stmt,
                scopes,
                test_name,
                assertion_index,
                source_path,
                decls,
                implications,
                used_names,
                assertion_atoms_by_base,
                base_order,
            )
            assertion_index += 1
            if pairs:
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

    # Emit ONE conjoined ::assertion contract per call-site base.  Multiple
    # assertions about the same subject (e.g. both `assert y is None` and
    # `assert y is not None`) are conjoined here into a single inv so the
    # consistency pass sees the full fact set and can detect contradictions.
    for base in base_order:
        atoms = assertion_atoms_by_base[base]
        conjoined_inv = atoms[0] if len(atoms) == 1 else and_(atoms)
        assertion_name = f"{base}::assertion"
        decls.append(ContractDecl(name=assertion_name, inv=conjoined_inv))

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


def _collect_value_scope_assertion_facts(
    stmt: ast.stmt,
    scopes: List[_ValueScope],
    test_name: str,
    assertion_index: int,
    source_path: str,
    decls: List[ContractDecl],
    implications: List[ImplicationDecl],
    used_names: Set[str],
    assertion_atoms_by_base: Dict[str, List[Formula]],
    base_order: List[str],
) -> int:
    """Collect ::facts contracts and implication wiring for one assertion
    statement.  The ::assertion contract itself is NOT emitted here; the
    caller (_classify_value_scope) emits ONE conjoined ::assertion per
    call-site base after processing all statements.  This keeps the kit
    dumb: it just admits facts; the conjunction (and all consistency
    checking) lives in the single conjoined invariant per base."""
    made = 0
    for scope in scopes:
        context = _assertion_callsite_context(stmt, scope)
        if context is None:
            continue
        origins, facts, assertion = context

        for origin in origins:
            base = _callsite_contract_base(origin, source_path)
            facts_name = _unique_contract_name(f"{base}::facts", used_names)
            implication_name = _unique_contract_name(
                f"{base}::facts-implies-assertion", used_names
            )
            fact_formula = facts[0] if len(facts) == 1 else and_(facts)
            decls.append(ContractDecl(name=facts_name, inv=fact_formula))
            # Accumulate assertion formula for this base; the conjoined
            # ::assertion contract is emitted once at the end of
            # _classify_value_scope so all assertions about the same
            # call-site subject land in one inv.
            if base not in assertion_atoms_by_base:
                assertion_atoms_by_base[base] = []
                base_order.append(base)
            assertion_atoms_by_base[base].append(assertion)
            # Wire the implication antecedent to the (not-yet-emitted)
            # conjoined assertion name; the contract will exist in the
            # same .proof bundle by the time the verifier loads it.
            assertion_name = f"{base}::assertion"
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
        elif name in _UNITTEST_IDENTITY_PREDICATES:
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
        if name in _UNITTEST_IDENTITY_PREDICATES:
            if len(call.args) < 2:
                raise ValueError(f"{name} expects at least 2 positional args")
            l = _translate_term_scoped(call.args[0], scope, call_vars)
            r = _translate_term_scoped(call.args[1], scope, call_vars)
            return _comparison_from_identity_symbol(
                _UNITTEST_IDENTITY_PREDICATES[name],
                l,
                r,
            )
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
    if isinstance(node, ast.Call):
        # Truthiness call (``assert h.startswith(p)``): lift as uninterpreted
        # predicate.  Use a term-translator that honours the current SSA scope.
        def _scoped_term(n: ast.expr) -> Term:
            return _translate_term_scoped(n, scope, call_vars)
        return _translate_truthiness_call_formula(node, _scoped_term)
    # Bare-var / attribute truthiness (``assert flag``, ``assert obj.ok``):
    # encode as ``eq(term, True)``.  This path is a fallback for non-call
    # expressions that are syntactically boolean; it does NOT apply to Call
    # nodes (handled above) — so isinstance and other calls are not silently
    # wrapped here.
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
    if isinstance(node, ast.Attribute):
        # Attribute access in value-scope context.  SSA-key the attribute
        # Var on the base's current SSA name so that ``out = f(x); assert
        # out.v == 1; out = g(y); assert out.v == 2`` produces two
        # independent Vars (``out$0.v`` and ``out$1.v``) rather than the
        # same ``out.v`` Var, which would look like a contradiction.
        # Without SSA the two atoms would share a Var and a conjunction
        # across the two assertions would be spuriously UNSAT.
        #
        # Algorithm: walk the dotted path; for each Name segment look it up
        # in scope.current; replace the FIRST Name segment that has an SSA
        # version.  Deep chains like ``a.b.c`` where only ``a`` is in scope
        # become ``a$N.b.c``.  If no segment is in scope, fall through to
        # the unscoped translator (raw dotted path = opaque free var) which
        # is unchanged behaviour for parameters and module-level names.
        ssa_attr_name = _ssa_dotted_attr_path(node, scope)
        if ssa_attr_name is not None:
            return make_var(ssa_attr_name)
        # Fall through: base not in scope → opaque free var by raw path.
        return _translate_term(node)
    if isinstance(node, ast.Subscript):
        # Subscript-index in value-scope context.  SSA-key the base: translate
        # the base expression under the scope so that if the base var has been
        # SSA-renamed (``parsed = f(); … parsed = g(); …``), the subscript Ctors
        # for the two generations are distinct (``subscript(parsed$0, k)`` vs
        # ``subscript(parsed$1, k)``).  This mirrors the SSA treatment of
        # attribute access: same-generation base + same key = same term = UNSAT
        # on contradiction; different generation = distinct terms = PROVEN.
        # Non-literal keys LOUDLY REFUSE via ``_subscript_key_term``.
        base_term = _translate_term_scoped(node.value, scope, call_vars)
        return _translate_subscript_term(node, base_term)
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


# ---------------------------------------------------------------------------
# PATTERN 6: mixed-body (opaque bindings + multiple asserts)
# ---------------------------------------------------------------------------
#
# Handles test methods / functions whose body is an interleaving of
# assignment statements (setup bindings) and ``assert`` statements, where the
# RHS of many bindings is NOT translatable (e.g. ``out = mint_contract(**kw)``,
# ``parsed = json.loads(...)``).  Pattern 5 requires a translatable call-result
# and emits ``::facts`` / ``::assertion`` contracts with implication wiring.
# Pattern 6 is simpler: it does NOT emit ``::facts`` contracts and does NOT
# require a translatable RHS.  Instead it:
#
#   1. Builds an SSA scope by walking each binding in order.  A translatable
#      RHS → SSA var bound to a term (same as Pattern 5).  An opaque RHS →
#      SSA var bound to a fresh free var (name$N) with NO value fact.  This
#      keeps attribute-access assertions (``assert out.cid == X``) scoped to
#      the correct SSA generation.
#
#   2. Collects liftable ``assert`` statements, translating each under the
#      current SSA scope.  Unliftable asserts → warn-and-skip (do NOT refuse
#      the whole method; partial lift is better than silence).
#
#   3. Conjoins all successfully lifted atoms into ONE ``ContractDecl`` named
#      by the test name.  This lands in the consistency pass as a whole-test
#      invariant (same path as Pattern 3), so contradictions in the conjoined
#      formula are detected by z3.
#
# SOUNDNESS RULES:
#   - Bindings are FACTS (definitions), never asserted properties.  They are
#     NOT emitted as contracts and do NOT enter the consistency pass.
#   - Opaque bindings produce a fresh SSA var with zero constraints → they
#     cannot create spurious UNSAT.  The consistency pass sees only the
#     asserted properties, which is correct.
#   - Reassignment: SSA bumps the version, so ``out = f(); assert out.x==1;
#     out = g(); assert out.x==2`` produces ``out$0.x`` and ``out$1.x``
#     (distinct Vars) → the two atoms are independent → CONSISTENT.
#   - Methods with unsupported control-flow or mutation (``with``, ``for``,
#     ``while``, ``try``, ``import``, subscript-assign) are LOUDLY REFUSED:
#     the method is claimed so Layer 0 does not retry it, a warning is
#     emitted, and zero contracts are produced.
#
# GATE: fires ONLY when the body has AT LEAST ONE binding AND at least one
# liftable assert.  Pure-assert bodies (all asserts, no binding) are Pattern 3.
# Pure-binding bodies (no asserts) are not test characterizations; they fall
# through to Layer 0.


def _mixed_body_unsupported_stmt(stmt: ast.stmt) -> Optional[str]:
    """Return a human-readable reason if ``stmt`` is an unsupported statement
    for the mixed-body pattern, or None if it is permitted.

    Permitted: ``ast.Assign`` (simple-name target only),
    ``ast.AnnAssign`` (simple-name target), ``ast.Assert``, ``ast.Pass``.
    Everything else (``with``, ``for``, ``while``, ``try``, ``import``,
    ``raise``, subscript-assign, etc.) is UNSUPPORTED → loud refusal.
    """
    if isinstance(stmt, ast.Assert):
        return None
    if isinstance(stmt, ast.Pass):
        return None
    if isinstance(stmt, ast.Assign):
        # Subscript or attribute ASSIGN target is mutation → unsupported.
        for tgt in stmt.targets:
            if not isinstance(tgt, ast.Name):
                return (
                    f"non-simple assignment target `{_unparse(tgt)[:60]}` "
                    "(subscript/attribute mutation is not soundly liftable)"
                )
        return None
    if isinstance(stmt, ast.AnnAssign):
        if not isinstance(stmt.target, ast.Name):
            return (
                f"non-simple annotated-assignment target `{_unparse(stmt.target)[:60]}`"
            )
        return None
    # Anything else is unsupported.
    kind = type(stmt).__name__
    return f"unsupported statement kind `{kind}` in mixed-body test"



def _classify_mixed_body(
    body: Sequence[ast.stmt],
    test_name: str,
    source_path: str,
    out: Layer2Output,
) -> bool:
    """Pattern 6: mixed body with opaque bindings + multiple asserts.

    Returns True if this pattern claimed the test (even on a loud refusal),
    False if the body does not fit the mixed-body shape (caller tries next).
    """
    # Pre-screen: the body must contain at least one binding AND at least one
    # assert.  Pure-assert is Pattern 3; pure-binding is not a test claim.
    has_binding = any(isinstance(s, (ast.Assign, ast.AnnAssign)) for s in body)
    has_assert = any(isinstance(s, ast.Assert) for s in body)
    if not (has_binding and has_assert):
        return False

    # Check every statement for unsupported constructs BEFORE doing any work.
    unsupported: List[str] = []
    for stmt in body:
        reason = _mixed_body_unsupported_stmt(stmt)
        if reason is not None:
            unsupported.append(reason)

    if unsupported:
        # LOUD REFUSAL: claim the test so Layer 0 does not retry it, emit
        # a warning per unsupported construct, produce zero contracts.
        out.claimed_tests.add(test_name)
        out.seen += 1
        out.mixed_body_skipped += 1
        for reason in unsupported:
            out.warnings.append(
                LiftWarning(
                    source_path,
                    test_name,
                    f"layer2 mixed-body: LOUD REFUSAL — {reason}",
                )
            )
        return True

    # --- SSA scope build + assertion collection -------------------------
    #
    # Walk the body in order.  For each binding, bump the SSA version and
    # install a fresh var in scope (opaque bindings → fresh free var with no
    # constraints; translatable bindings → term-valued var, same as P5).
    # For each assert, translate under the current scope and collect the atom.

    # SSA state: maps local name → current SSA var term.
    ssa_current: Dict[str, Term] = {}
    ssa_versions: Dict[str, int] = {}

    atoms: List[Formula] = []
    skip_reasons: List[str] = []

    for stmt in body:
        # ---- Binding ----
        if isinstance(stmt, (ast.Assign, ast.AnnAssign)):
            if isinstance(stmt, ast.Assign):
                target_name = stmt.targets[0].id  # simple name guaranteed above
                value_node = stmt.value
            else:  # AnnAssign
                target_name = stmt.target.id
                value_node = stmt.value  # may be None (bare annotation)

            version = ssa_versions.get(target_name, 0)
            ssa_versions[target_name] = version + 1
            ssa_name = f"{target_name}${version}"
            ssa_var = make_var(ssa_name)
            ssa_current[target_name] = ssa_var
            # We do NOT emit a facts contract. The SSA var is an opaque free
            # var unless the RHS is translatable (in which case a value
            # constraint would be useful, but for the consistency pass only
            # the asserted properties matter; we intentionally keep this
            # simple and sound).
            continue

        # ---- Assert ----
        if isinstance(stmt, ast.Assert):
            # Translate under the current SSA scope.
            scope = _ValueScope(current=dict(ssa_current))
            try:
                atom = _lift_assertion_stmt_scoped(stmt, scope)
            except ValueError as e:
                skip_reasons.append(f"`{_unparse(stmt)[:60]}`: {e}")
                continue
            atoms.append(atom)
            continue

        # ---- Pass ----
        # already permitted; nothing to do.

    if not atoms:
        # No assert was liftable. Warn and claim (so Layer 0 can try).
        out.claimed_tests.add(test_name)
        out.seen += 1
        out.mixed_body_skipped += 1
        out.warnings.append(
            LiftWarning(
                source_path,
                test_name,
                f"layer2 mixed-body: 0 of {len([s for s in body if isinstance(s, ast.Assert)])} "
                f"asserts were liftable; releasing claim to Layer 0. "
                f"Skipped: {'; '.join(skip_reasons)}",
            )
        )
        return True

    # Conjoin all lifted atoms into ONE contract (Pattern-3 shape).
    # A single lifted atom is emitted as-is (no redundant and-wrapper).
    inv = atoms[0] if len(atoms) == 1 else and_(atoms)
    out.claimed_tests.add(test_name)
    out.seen += 1
    out.decls.append(ContractDecl(name=test_name, inv=inv))
    out.lifted += 1
    out.mixed_body_lifted += 1

    if skip_reasons:
        out.warnings.append(
            LiftWarning(
                source_path,
                test_name,
                f"layer2 mixed-body: {len(skip_reasons)} of "
                f"{len([s for s in body if isinstance(s, ast.Assert)])} asserts skipped "
                f"(unliftable shape): {'; '.join(skip_reasons)}",
            )
        )

    return True
