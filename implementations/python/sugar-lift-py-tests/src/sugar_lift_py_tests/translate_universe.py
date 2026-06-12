"""Translate-table universe walk (python generalization layer, rung 2a).

A vendor body of the shape ``return <call>.translate(TABLE)`` where TABLE
is a stable module-level ``bytes.maketrans(b"...", b"...")`` binding swears
a complement universe: translate is total, so the output contains NONE of
the surviving from-side characters. Emitted as
``str.chars-not-in-set(subject, forbidden)`` — one more conjunct under the
callee's ``#euf#`` base. The encoder being delegated to (C or python) is
never walked; the claim rests entirely on the translate literals, which is
the whole point: the seam is provable even when the encoder is not.

Soundness gates (each refusal is named, never silent):
- the vendor body must be exactly docstring? + return <call>.translate(NAME)
  (any other statement could affect the result; refuse the walk);
- NAME must have exactly one module-level binding and no global-declaration
  puncture (the same teeth as sugar_lift_python_source.value_pins);
- the binding must be ``bytes.maketrans(<bytes literal>, <bytes literal>)``
  with equal lengths (CPython raises otherwise) and ASCII-only bytes;
- the forbidden set is from_set MINUS to_set: a translate that maps '+'->'/'
  REINTRODUCES '/', so swapped tables yield an empty set and NO universe.
"""

from __future__ import annotations

import ast
import functools
import importlib.util
from dataclasses import dataclass
from typing import Optional, Tuple

try:
    from sugar_lift_python_source.value_pins import (
        _admission_failure,
        _binding_events,
        _Candidate,
        _global_declarations,
    )
except ModuleNotFoundError:  # repo checkout without editable installs
    import sys
    from pathlib import Path

    _SIBLING_SRC = (
        Path(__file__).resolve().parents[3] / "sugar-lift-python-source" / "src"
    )
    if str(_SIBLING_SRC) not in sys.path:
        sys.path.insert(0, str(_SIBLING_SRC))
    # The binding-stability teeth live in ONE place (value_pins); a local
    # mirror here would be the Java lesson's latent-hole scan all over again.
    from sugar_lift_python_source.value_pins import (
        _admission_failure,
        _binding_events,
        _Candidate,
        _global_declarations,
    )


@dataclass(frozen=True)
class TranslateUniverse:
    """A walked complement universe over the callee's output. Two kinds:

    - ``chars-not-in-set``: the output CONTAINS none of ``forbidden``
      (derived from a total bytes.translate over a maketrans table);
    - ``no-suffix-chars``: the output ENDS WITH none of ``forbidden``
      (derived from a total .rstrip of a bytes literal -- the
      token-padding family: rstrip(b"=") means no trailing padding,
      ever, for any input).

    Provenance pins the claim to the vendor source."""

    forbidden: str
    module: str
    qualname: str
    source_path: str
    lineno: int
    table_name: str
    kind: str = "chars-not-in-set"
    # ∀⊨sample evidence: how many VENDOR vectors (from the vendor's own test
    # corpus, the same party that swore the body) were evaluated against the
    # walked set, and where they came from. Zero with a None source means no
    # vendor test corpus resolved -- licensed by the body walk alone, said
    # plainly rather than implied.
    vendor_vectors_checked: int = 0
    vendor_vector_source: Optional[str] = None
    # member-of-values payload: the pinned tuple's string elements.
    values: tuple = ()


@dataclass(frozen=True)
class TranslateWalkRefusal:
    callee: str
    reason: str


@dataclass(frozen=True)
class GuardClause:
    """One walked precondition: `if <param> <op> <literal>: raise` means a
    RETURNED value exists only when the guard did NOT fire. param_index is
    positional in the vendor signature; op is the ir comparison symbol of
    the guard test itself (the universe conjunct is its negation,
    instantiated at the callsite's concrete argument)."""

    param_index: int
    param_name: str
    op: str
    literal: object


@dataclass(frozen=True)
class GuardUniverse:
    clauses: tuple
    module: str
    qualname: str
    source_path: str
    lineno: int
    vendor_vectors_checked: int = 0
    vendor_vector_source: Optional[str] = None


_CMP_SYMBOL = {
    ast.Lt: "<",
    ast.LtE: "≤",
    ast.Gt: ">",
    ast.GtE: "≥",
    ast.Eq: "=",
    ast.NotEq: "≠",
}

_CMP_EVAL = {
    "<": lambda a, b: a < b,
    "≤": lambda a, b: a <= b,
    ">": lambda a, b: a > b,
    "≥": lambda a, b: a >= b,
    "=": lambda a, b: a == b,
    "≠": lambda a, b: a != b,
}


@functools.lru_cache(maxsize=None)
def guard_universe_for_callee(
    callee: str,
) -> Tuple[Optional[GuardUniverse], Optional[TranslateWalkRefusal]]:
    """Walk the vendor body's guard-then-raise prefix: every leading
    `if <param> <cmpop> <literal>: raise ...` swears that a RETURN value
    only exists when the guard did not fire. A sworn equality about
    callee(<concrete args>) therefore carries the negated guard,
    instantiated at those arguments, as one more conjunct: assert a value
    for a guarded-out input and the conjunction is UNSAT -- you swore a
    return from a call the vendor's own source says raises."""
    resolved = _resolve_vendor_function(callee)
    if resolved is None:
        return None, None
    tree, fn, spec_origin, module_name, fn_name = resolved

    def refuse(reason: str) -> Tuple[None, TranslateWalkRefusal]:
        return None, TranslateWalkRefusal(callee=callee, reason=reason)

    params = [a.arg for a in fn.args.args]
    body = [
        stmt
        for stmt in fn.body
        if not (
            isinstance(stmt, ast.Expr)
            and isinstance(stmt.value, ast.Constant)
            and isinstance(stmt.value.value, str)
        )
    ]
    clauses = []
    for stmt in body:
        if not (
            isinstance(stmt, ast.If)
            and len(stmt.body) == 1
            and isinstance(stmt.body[0], ast.Raise)
            and not stmt.orelse
        ):
            break
        clause = _guard_clause(stmt.test, params)
        if clause is None:
            # Guards are pure tests: the per-guard claim "a value implies
            # this guard did not fire" holds independently of siblings, so
            # an unreadable guard is skipped (its claim is simply not made)
            # rather than poisoning the readable ones.
            continue
        clauses.append(clause)
    if not clauses:
        return None, None

    vectors, vector_source = _vendor_call_vectors(module_name, fn_name)
    checked = 0
    for args in vectors:
        for c in clauses:
            if c.param_index < len(args):
                arg = args[c.param_index]
                try:
                    fired = _CMP_EVAL[c.op](arg, c.literal)
                except TypeError:
                    continue
                checked += 1
                if fired:
                    return refuse(
                        "sample-gate: vendor vector args "
                        f"{args!r} fire the guard "
                        f"({c.param_name} {c.op} {c.literal!r}) yet the "
                        "vendor swears a return value; the walk misread "
                        "the body or the vendor contradicts their own source"
                    )

    return (
        GuardUniverse(
            clauses=tuple(clauses),
            module=module_name,
            qualname=f"{module_name}.{fn_name}",
            source_path=spec_origin,
            lineno=fn.lineno,
            vendor_vectors_checked=checked,
            vendor_vector_source=vector_source,
        ),
        None,
    )


@dataclass(frozen=True)
class ConstantUniverse:
    """A function that unconditionally returns one literal: the strongest
    universal there is -- not membership, EQUALITY. The output IS the
    literal for every input that returns at all (a guard-then-raise prefix
    only affects whether it returns, never WHAT). value_kind is 'str' /
    'int' / 'bool' / 'none' / 'bytes' so the emitter builds the right term;
    value is the python literal."""

    value: object
    value_kind: str
    module: str
    qualname: str
    source_path: str
    lineno: int
    vendor_vectors_checked: int = 0
    vendor_vector_source: Optional[str] = None


def _constant_return_value(body: list):
    """If the body (sans docstring, sans any leading guard-then-raise
    prefix) is a SINGLE `return <literal>` with NO other return/yield
    anywhere, the function unconditionally returns that literal. Returns
    (value, kind) or None. Guards before the return only gate whether it
    returns; they never change the value, so the equality still holds for
    every input that returns."""
    # strip a leading guard-then-raise prefix (if X: raise ...)
    rest = []
    seen_nonguard = False
    for stmt in body:
        is_guard = (
            isinstance(stmt, ast.If)
            and len(stmt.body) == 1
            and isinstance(stmt.body[0], ast.Raise)
            and not stmt.orelse
        )
        if is_guard and not seen_nonguard:
            continue
        seen_nonguard = True
        rest.append(stmt)
    if len(rest) != 1 or not isinstance(rest[0], ast.Return):
        return None
    # no OTHER return/yield in the whole body would let a different value
    # escape; a single return of a literal is the whole story.
    returns = sum(
        1
        for stmt in body
        for n in ast.walk(stmt)
        if isinstance(n, (ast.Return, ast.Yield, ast.YieldFrom))
    )
    if returns != 1:
        return None
    return _literal_value_kind(rest[0].value)


def _literal_value_kind(node):
    if isinstance(node, ast.Constant):
        v = node.value
        if isinstance(v, bool):
            return (v, "bool")
        if isinstance(v, int):
            return (v, "int")
        if isinstance(v, str):
            return (v, "str")
        if v is None:
            return (None, "none")
        if isinstance(v, bytes):
            try:
                v.decode("ascii")
            except UnicodeDecodeError:
                return None
            return (v, "bytes")
    if (
        isinstance(node, ast.UnaryOp)
        and isinstance(node.op, ast.USub)
        and isinstance(node.operand, ast.Constant)
        and type(node.operand.value) is int
    ):
        return (-node.operand.value, "int")
    return None


@dataclass(frozen=True)
class PredicateUniverse:
    """Census family return-predicate (24k bodies): a body that returns a
    PURE boolean expression over its parameters. At a CONCRETE callsite the
    expression evaluates to a ground bool -- evaluating the vendor's own
    body at the consumer's input (recompute, not solver-invention) -- so the
    output EQUALS that bool. expr is the ast.expr template; params the
    positional names; the emitter binds args->params and evaluates."""

    expr: object  # ast.expr (boolean over params/literals)
    params: tuple
    module: str
    qualname: str
    source_path: str
    lineno: int
    vendor_vectors_checked: int = 0
    vendor_vector_source: Optional[str] = None


_MISSING = object()


def _is_pure_predicate(node, params) -> bool:
    """The return expression is a boolean over ONLY parameter names and
    literals, combined with comparisons / and / or / not. Anything else
    (calls, attributes, subscripts, free names) is not purely evaluable at a
    concrete callsite, so it is not a candidate."""
    if isinstance(node, ast.Compare):
        return all(
            _is_pure_operand(x, params)
            for x in [node.left, *node.comparators]
        ) and all(
            isinstance(op, (ast.Lt, ast.LtE, ast.Gt, ast.GtE, ast.Eq, ast.NotEq))
            for op in node.ops
        )
    if isinstance(node, ast.BoolOp):
        return all(_is_pure_predicate(v, params) for v in node.values)
    if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.Not):
        return _is_pure_predicate(node.operand, params)
    return False


def _is_pure_operand(node, params) -> bool:
    if isinstance(node, ast.Name):
        return node.id in params
    if isinstance(node, ast.Constant):
        return isinstance(node.value, (int, str, bool)) or node.value is None
    if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.USub):
        return (
            isinstance(node.operand, ast.Constant)
            and type(node.operand.value) is int
        )
    return False


def _operand_value(node, env):
    if isinstance(node, ast.Name):
        return env.get(node.id, _MISSING)
    if isinstance(node, ast.Constant):
        return node.value
    if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.USub):
        if isinstance(node.operand, ast.Constant):
            return -node.operand.value
    return _MISSING


def eval_predicate(node, env):
    """Ground-evaluate a pure predicate AST against env (param name -> python
    value). Returns a bool, or None when a value is missing / types mismatch
    (then no universe is emitted at that callsite -- conservative)."""
    try:
        if isinstance(node, ast.Compare):
            left = _operand_value(node.left, env)
            if left is _MISSING:
                return None
            for op, comp in zip(node.ops, node.comparators):
                right = _operand_value(comp, env)
                if right is _MISSING:
                    return None
                if not _CMP_EVAL[_CMP_SYMBOL[type(op)]](left, right):
                    return False
                left = right
            return True
        if isinstance(node, ast.BoolOp):
            vals = [eval_predicate(v, env) for v in node.values]
            if any(v is None for v in vals):
                return None
            return all(vals) if isinstance(node.op, ast.And) else any(vals)
        if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.Not):
            v = eval_predicate(node.operand, env)
            return None if v is None else (not v)
    except TypeError:
        return None
    return None


@functools.lru_cache(maxsize=None)
def predicate_universe_for_callee(
    callee: str,
) -> Tuple[Optional[PredicateUniverse], Optional[TranslateWalkRefusal]]:
    resolved = _resolve_vendor_function(callee)
    if resolved is None:
        return None, None
    tree, fn, spec_origin, module_name, fn_name = resolved
    params = tuple(a.arg for a in fn.args.args)
    body = [
        stmt
        for stmt in fn.body
        if not (
            isinstance(stmt, ast.Expr)
            and isinstance(stmt.value, ast.Constant)
            and isinstance(stmt.value.value, str)
        )
    ]
    rest = []
    seen = False
    for stmt in body:
        if (
            not seen
            and isinstance(stmt, ast.If)
            and len(stmt.body) == 1
            and isinstance(stmt.body[0], ast.Raise)
            and not stmt.orelse
        ):
            continue
        seen = True
        rest.append(stmt)
    if (
        len(rest) != 1
        or not isinstance(rest[0], ast.Return)
        or rest[0].value is None
    ):
        return None, None
    expr = rest[0].value
    if not _is_pure_predicate(expr, params):
        return None, None
    returns = sum(
        1
        for stmt in body
        for n in ast.walk(stmt)
        if isinstance(n, (ast.Return, ast.Yield, ast.YieldFrom))
    )
    if returns != 1:
        return None, None
    return (
        PredicateUniverse(
            expr=expr,
            params=params,
            module=module_name,
            qualname=f"{module_name}.{fn_name}",
            source_path=spec_origin,
            lineno=fn.lineno,
        ),
        None,
    )


@functools.lru_cache(maxsize=None)
def constant_universe_for_callee(
    callee: str,
) -> Tuple[Optional[ConstantUniverse], Optional[TranslateWalkRefusal]]:
    """Census family return-constant (34k bodies, the largest backlog item).
    A body that unconditionally returns one literal swears the EQUALITY
    universal: callee(<anything>) == that literal. A consumer asserting any
    other value for any input refutes against it."""
    resolved = _resolve_vendor_function(callee)
    if resolved is None:
        return None, None
    tree, fn, spec_origin, module_name, fn_name = resolved

    def refuse(reason):
        return None, TranslateWalkRefusal(callee=callee, reason=reason)

    body = [
        stmt
        for stmt in fn.body
        if not (
            isinstance(stmt, ast.Expr)
            and isinstance(stmt.value, ast.Constant)
            and isinstance(stmt.value.value, str)
        )
    ]
    vk = _constant_return_value(body)
    if vk is None:
        return None, None
    value, kind = vk

    # ∀⊨sample: every vendor vector (expected return) must EQUAL the literal
    # -- it must, since the body always returns it; a mismatch means we
    # misread the body (e.g. a return we missed) -> refuse, never guess.
    vectors, vector_source = _vendor_vectors(module_name, fn_name)
    for vector in vectors:
        if kind in ("str", "bytes") and isinstance(vector, str):
            want = value.decode("ascii") if kind == "bytes" else value
            if vector != want:
                return refuse(
                    f"sample-gate: vendor vector {vector!r} from "
                    f"{vector_source} != the constant {value!r}; the walk "
                    "misread the body or the vendor contradicts their source"
                )

    return (
        ConstantUniverse(
            value=value,
            value_kind=kind,
            module=module_name,
            qualname=f"{module_name}.{fn_name}",
            source_path=spec_origin,
            lineno=fn.lineno,
            vendor_vectors_checked=len(vectors),
            vendor_vector_source=vector_source,
        ),
        None,
    )


def _guard_clause(test: ast.expr, params: list) -> Optional[GuardClause]:
    if not (
        isinstance(test, ast.Compare)
        and len(test.ops) == 1
        and len(test.comparators) == 1
    ):
        return None
    op = _CMP_SYMBOL.get(type(test.ops[0]))
    if op is None:
        return None
    left, right = test.left, test.comparators[0]
    if (
        isinstance(left, ast.Name)
        and left.id in params
        and isinstance(right, ast.Constant)
        and isinstance(right.value, (int, str))
        and not isinstance(right.value, bool)
    ):
        return GuardClause(params.index(left.id), left.id, op, right.value)
    # literal-on-the-left mirror: `if 0 > x: raise` == `x < 0`
    if (
        isinstance(right, ast.Name)
        and right.id in params
        and isinstance(left, ast.Constant)
        and isinstance(left.value, (int, str))
        and not isinstance(left.value, bool)
    ):
        mirror = {"<": ">", ">": "<", "≤": "≥", "≥": "≤", "=": "=", "≠": "≠"}
        return GuardClause(
            params.index(right.id), right.id, mirror[op], left.value
        )
    return None


# Non-determinism markers: a body that (transitively, within its module)
# reaches any of these does NOT return the same value for the same args, so
# EUF same-args unification would manufacture a false contradiction (two
# salted hashes of "secret" are unequal). Detection is EVIDENCE-BASED and
# conservative: we drop unification only when we SEE a marker; unknown or
# unresolvable bodies stay "pure" (the current sound-conservative behavior).
_NONDET_ATTRS = frozenset(
    {
        "random", "uniform", "randint", "randrange", "choice", "choices",
        "sample", "shuffle", "getrandbits", "seed", "token_bytes",
        "token_hex", "token_urlsafe", "urandom", "uuid1", "uuid4",
        "now", "utcnow", "today", "time", "monotonic", "perf_counter",
        "gen_salt",
    }
)
_NONDET_MODULES = frozenset({"random", "secrets", "uuid"})


@functools.lru_cache(maxsize=None)
def callee_is_nondeterministic(callee: str) -> bool:
    """True iff the resolved vendor body transitively (same module, bounded
    depth) reaches a non-determinism source. Evidence-based: False on any
    body we cannot resolve, so EUF unification keeps its current
    sound-conservative behavior where we have no evidence."""
    resolved = _resolve_vendor_function(callee)
    if resolved is None:
        return False
    tree, fn, _origin, module_name, _fn_name = resolved
    module_fns = {
        s.name: s
        for s in tree.body
        if isinstance(s, (ast.FunctionDef, ast.AsyncFunctionDef))
    }
    return _body_reaches_nondet(fn, module_fns, depth=3, seen=set())


def _body_reaches_nondet(fn, module_fns, depth, seen) -> bool:
    if fn.name in seen or depth <= 0:
        return False
    seen.add(fn.name)
    for node in ast.walk(fn):
        if isinstance(node, ast.Call):
            f = node.func
            if isinstance(f, ast.Attribute):
                if f.attr in _NONDET_ATTRS:
                    return True
                if (
                    isinstance(f.value, ast.Name)
                    and f.value.id in _NONDET_MODULES
                ):
                    return True
            elif isinstance(f, ast.Name):
                if f.id in _NONDET_ATTRS:
                    return True
                callee_fn = module_fns.get(f.id)
                if callee_fn is not None and _body_reaches_nondet(
                    callee_fn, module_fns, depth - 1, seen
                ):
                    return True
    return False


def _resolve_vendor_function(callee: str):
    if "." not in callee:
        return None
    module_name, fn_name = callee.rsplit(".", 1)
    try:
        spec = importlib.util.find_spec(module_name)
    except (ImportError, ValueError):
        return None
    if spec is None or spec.origin in (None, "built-in", "frozen"):
        return None
    if not spec.origin.endswith(".py"):
        return None
    try:
        tree = ast.parse(
            open(spec.origin, encoding="utf-8").read(), filename=spec.origin
        )
    except (OSError, SyntaxError):
        return None
    fn = next(
        (
            stmt
            for stmt in tree.body
            if isinstance(stmt, ast.FunctionDef) and stmt.name == fn_name
        ),
        None,
    )
    if fn is None:
        return None
    return tree, fn, spec.origin, module_name, fn_name


def _vendor_call_vectors(
    module_name: str, fn_name: str
) -> Tuple[list, Optional[str]]:
    """All-literal argument tuples from the vendor corpus' calls of the
    callee (the guard gate's evidence: a vendor-sworn call must not fire
    a walked guard)."""
    for candidate in (f"test.test_{module_name}", f"test_{module_name}"):
        try:
            spec = importlib.util.find_spec(candidate)
        except (ImportError, ValueError):
            continue
        if spec is None or not spec.origin or not spec.origin.endswith(".py"):
            continue
        try:
            tree = ast.parse(
                open(spec.origin, encoding="utf-8").read(), filename=spec.origin
            )
        except (OSError, SyntaxError):
            continue
        vectors = []
        for node in ast.walk(tree):
            if _is_callee_call(node, fn_name):
                args = []
                ok = True
                for a in node.args:
                    if isinstance(a, ast.Constant) and isinstance(
                        a.value, (int, str)
                    ):
                        args.append(a.value)
                    elif (
                        isinstance(a, ast.UnaryOp)
                        and isinstance(a.op, ast.USub)
                        and isinstance(a.operand, ast.Constant)
                        and type(a.operand.value) is int
                    ):
                        args.append(-a.operand.value)
                    else:
                        ok = False
                        break
                if ok and args:
                    vectors.append(tuple(args))
        return vectors, spec.origin
    return [], None


@functools.lru_cache(maxsize=None)
def translate_universe_for_callee(
    callee: str,
) -> Tuple[Optional[TranslateUniverse], Optional[TranslateWalkRefusal]]:
    """Resolve a dotted callee against installed vendor source and walk the
    translate shape. Returns (universe, None) on success, (None, refusal)
    when the callee matched the family but a gate refused, and (None, None)
    when the callee is simply not translate-shaped (not a candidate; no
    refusal owed)."""
    if "." not in callee:
        return None, None
    module_name, fn_name = callee.rsplit(".", 1)
    try:
        spec = importlib.util.find_spec(module_name)
    except (ImportError, ValueError):
        return None, None
    if spec is None or spec.origin in (None, "built-in", "frozen"):
        return None, None
    if not spec.origin.endswith(".py"):
        # Compiled extension: nothing to walk. Not a candidate.
        return None, None
    try:
        source = open(spec.origin, encoding="utf-8").read()
    except OSError:
        return None, None
    try:
        tree = ast.parse(source, filename=spec.origin)
    except SyntaxError:
        return None, None

    fn = next(
        (
            stmt
            for stmt in tree.body
            if isinstance(stmt, ast.FunctionDef) and stmt.name == fn_name
        ),
        None,
    )
    if fn is None:
        return None, None

    body = [
        stmt
        for stmt in fn.body
        if not (
            isinstance(stmt, ast.Expr)
            and isinstance(stmt.value, ast.Constant)
            and isinstance(stmt.value.value, str)
        )
    ]
    def refuse(reason: str) -> Tuple[None, TranslateWalkRefusal]:
        return None, TranslateWalkRefusal(callee=callee, reason=reason)

    shape = _translate_return_shape(body)
    if shape is None:
        # return-replace-literals (census family): a SINGLE-char
        # .replace(FROM, TO) as the last op removes FROM from the output
        # entirely (replace is total over its from-string), unless TO is FROM
        # -- the chars-not-in-set complement, same as translate, literals
        # inline so no binding scan.
        replace_pair = _replace_return_shape(body)
        if replace_pair is not None:
            frm, to = replace_pair
            if frm == to:
                return refuse(
                    f"replace({frm!r}, {to!r}) is a no-op; no complement claim"
                )
            forbidden = frm
            vectors, vector_source = _vendor_vectors(module_name, fn_name)
            for vector in vectors:
                if forbidden in vector:
                    return refuse(
                        f"sample-gate: vendor vector {vector!r} from "
                        f"{vector_source} contains the replaced char "
                        f"{forbidden!r}; the walk misread the body or the "
                        "vendor contradicts their own source"
                    )
            return (
                TranslateUniverse(
                    forbidden=forbidden,
                    module=module_name,
                    qualname=f"{module_name}.{fn_name}",
                    source_path=spec.origin,
                    lineno=fn.lineno,
                    table_name="<inline replace from-char>",
                    kind="chars-not-in-set",
                    vendor_vectors_checked=len(vectors),
                    vendor_vector_source=vector_source,
                ),
                None,
            )
        # Second family: a total .rstrip of a bytes literal as the LAST
        # operation -- the token-padding shape (itsdangerous.base64_encode:
        # return urlsafe_b64encode(s).rstrip(b"=")). The claim "output never
        # ends with a stripped char" derives from rstrip totality ALONE, so
        # preceding statements are irrelevant and no binding scan is needed
        # (the literal is inline).
        strip_literal = _rstrip_return_shape(body)
        if strip_literal is None:
            # Third family: return TABLE[<expr>] over a stable module-level
            # tuple of string literals -- the membership universe (the
            # corpus census's cheapest swearable shape, 15,943 bodies in the
            # top-1000). The returned value is ALWAYS an element of the
            # pinned tuple: subscript either yields an element or raises
            # (no value escapes the table), so membership holds for every
            # input that returns at all. Index expression is irrelevant.
            sub_table = _table_subscript_shape(body)
            if sub_table is None:
                # Fourth family: the table-loop (census: 17,781 bodies).
                # acc = []/"" ; for ...: acc.append(TABLE[i]) / acc += ... ;
                # return sep.join(acc) / acc. Every accumulated piece is an
                # element of a pinned table (or a literal), so every output
                # char is in the union of their chars -- the POSITIVE
                # chars-in-set universe. Widening the set is the safe
                # direction (output ⊆ S stays true for any S ⊇ truth), so
                # join separators and literal appends just add their chars.
                loop_result = _table_loop_charset(body, tree)
                if loop_result is None:
                    # No family matched: not a candidate, no refusal owed.
                    return None, None
                allowed, loop_refusal = loop_result
                if loop_refusal is not None:
                    return refuse(loop_refusal)
                vectors, vector_source = _vendor_vectors(module_name, fn_name)
                for vector in vectors:
                    stray = sorted(set(vector) - set(allowed))
                    if stray:
                        return refuse(
                            f"sample-gate: vendor vector {vector!r} from "
                            f"{vector_source} contains chars {stray!r} "
                            "outside the walked table union; the walk "
                            "misread the body or the vendor contradicts "
                            "their own source"
                        )
                return (
                    TranslateUniverse(
                        forbidden=allowed,
                        module=module_name,
                        qualname=f"{module_name}.{fn_name}",
                        source_path=spec.origin,
                        lineno=fn.lineno,
                        table_name="<table-loop union>",
                        kind="chars-in-set",
                        vendor_vectors_checked=len(vectors),
                        vendor_vector_source=vector_source,
                    ),
                    None,
                )
            values_node, _line = _module_binding_tuple(tree, sub_table)
            if values_node is None:
                return refuse(
                    f"subscript table '{sub_table}' has no single "
                    "module-level tuple-literal binding (mutable or computed "
                    "tables cannot pin)"
                )
            tbl_candidate = _Candidate(
                name=sub_table, value=values_node, line=_line, confession=None
            )
            tbl_events = [
                e for e in _binding_events(tree) if e.name == sub_table
            ]
            tbl_failure = _admission_failure(
                tbl_candidate,
                tbl_events,
                _global_declarations(tree).get(sub_table),
            )
            if tbl_failure is not None:
                return refuse(
                    f"subscript table '{sub_table}' is not stable: {tbl_failure}"
                )
            values = []
            for el in values_node.elts:
                if isinstance(el, ast.Constant) and isinstance(el.value, str):
                    values.append(el.value)
                else:
                    return refuse(
                        f"subscript table '{sub_table}' is not all-string "
                        "literals (mixed/non-string tables are vNext, refused "
                        "by name)"
                    )
            if not values:
                return refuse(f"subscript table '{sub_table}' is empty")
            vectors, vector_source = _vendor_vectors(module_name, fn_name)
            for vector in vectors:
                if vector not in values:
                    return refuse(
                        f"sample-gate: vendor vector {vector!r} from "
                        f"{vector_source} is not in the walked table; the "
                        "walk misread the body or the vendor contradicts "
                        "their own source"
                    )
            return (
                TranslateUniverse(
                    forbidden="",
                    module=module_name,
                    qualname=f"{module_name}.{fn_name}",
                    source_path=spec.origin,
                    lineno=fn.lineno,
                    table_name=sub_table,
                    kind="member-of-values",
                    vendor_vectors_checked=len(vectors),
                    vendor_vector_source=vector_source,
                    values=tuple(values),
                ),
                None,
            )
        try:
            strip_chars = strip_literal.decode("ascii")
        except UnicodeDecodeError:
            return refuse("rstrip bytes are not ASCII; charset atom needs ASCII")
        forbidden = "".join(sorted(set(strip_chars)))
        if not forbidden:
            return refuse("rstrip literal is empty; no claim exists")
        vectors, vector_source = _vendor_vectors(module_name, fn_name)
        for vector in vectors:
            if vector and vector[-1] in forbidden:
                return refuse(
                    f"sample-gate: vendor vector {vector!r} from "
                    f"{vector_source} ends with a stripped char; the walk "
                    "misread the body or the vendor contradicts their own "
                    "source"
                )
        return (
            TranslateUniverse(
                forbidden=forbidden,
                module=module_name,
                qualname=f"{module_name}.{fn_name}",
                source_path=spec.origin,
                lineno=fn.lineno,
                table_name="<inline rstrip literal>",
                kind="no-suffix-chars",
                vendor_vectors_checked=len(vectors),
                vendor_vector_source=vector_source,
            ),
            None,
        )
    table_name = shape

    table_call, table_line = _module_binding_call(tree, table_name)
    if table_call is None:
        return refuse(
            f"translate table '{table_name}' has no single module-level "
            "call binding"
        )
    candidate = _Candidate(
        name=table_name, value=table_call, line=table_line, confession=None
    )
    events = [e for e in _binding_events(tree) if e.name == table_name]
    failure = _admission_failure(
        candidate, events, _global_declarations(tree).get(table_name)
    )
    if failure is not None:
        return refuse(f"translate table '{table_name}' is not stable: {failure}")

    pair = _maketrans_byte_literals(table_call)
    if pair is None:
        return refuse(
            f"translate table '{table_name}' is not "
            "bytes.maketrans(<bytes literal>, <bytes literal>)"
        )
    frm, to = pair
    if len(frm) != len(to):
        return refuse("maketrans from/to lengths differ")
    try:
        frm_s = frm.decode("ascii")
        to_s = to.decode("ascii")
    except UnicodeDecodeError:
        return refuse("maketrans bytes are not ASCII; charset atom needs ASCII")

    # from MINUS to: a mapped-away char that some other mapping writes back
    # is NOT forbidden in the output. Swapped tables yield the empty set.
    forbidden = "".join(sorted(set(frm_s) - set(to_s)))
    if not forbidden:
        return refuse(
            "translate table reintroduces every mapped char (swap-shaped); "
            "no complement universe exists"
        )

    # ∀⊨SAMPLE (the gate): the walked universe must be consistent with every
    # vector the VENDOR's own test corpus swears at this surface -- the same
    # party that wrote the body, evaluated by ground computation, no solver.
    # Consumer claims are NOT gate evidence: a consumer contradicting the
    # universe is the refutation working, decided by check. A violating
    # VENDOR vector means our walk misread the body (or the vendor
    # contradicts their own source); the universe is refused.
    vectors, vector_source = _vendor_vectors(module_name, fn_name)
    for vector in vectors:
        violating = sorted(set(ch for ch in forbidden if ch in vector))
        if violating:
            return refuse(
                f"sample-gate: vendor vector {vector!r} from "
                f"{vector_source} contains forbidden {violating!r}; the walk "
                "misread the body or the vendor contradicts their own source"
            )

    return (
        TranslateUniverse(
            forbidden=forbidden,
            module=module_name,
            qualname=f"{module_name}.{fn_name}",
            source_path=spec.origin,
            lineno=fn.lineno,
            table_name=table_name,
            vendor_vectors_checked=len(vectors),
            vendor_vector_source=vector_source,
        ),
        None,
    )


def _vendor_vectors(
    module_name: str, fn_name: str
) -> Tuple[list, Optional[str]]:
    """Sworn expected-value vectors for the callee surface, extracted from
    the vendor's own test corpus (CPython convention: test.test_<module>;
    sibling convention: test_<module>). A vector is the str/bytes literal a
    vendor assertion equates with a call of the callee. Extraction is an
    ast scan (calls of fn_name compared/assertEqual'd against a literal);
    what it cannot read it does not invent -- the gate runs over what is
    sworn AND extractable, and reports the count."""
    for candidate in (f"test.test_{module_name}", f"test_{module_name}"):
        try:
            spec = importlib.util.find_spec(candidate)
        except (ImportError, ValueError):
            continue
        if spec is None or not spec.origin or not spec.origin.endswith(".py"):
            continue
        try:
            tree = ast.parse(
                open(spec.origin, encoding="utf-8").read(), filename=spec.origin
            )
        except (OSError, SyntaxError):
            continue
        return _extract_vectors(tree, fn_name), spec.origin
    return [], None


def _is_callee_call(node: ast.AST, fn_name: str) -> bool:
    if not isinstance(node, ast.Call):
        return False
    func = node.func
    if isinstance(func, ast.Name):
        return func.id == fn_name
    if isinstance(func, ast.Attribute):
        return func.attr == fn_name
    return False


def _literal_text(node: ast.AST) -> Optional[str]:
    if isinstance(node, ast.Constant):
        if isinstance(node.value, str):
            return node.value
        if isinstance(node.value, bytes):
            try:
                return node.value.decode("ascii")
            except UnicodeDecodeError:
                return None
    return None


def _extract_vectors(tree: ast.Module, fn_name: str) -> list:
    vectors = []
    for node in ast.walk(tree):
        operands: list = []
        if isinstance(node, ast.Compare) and len(node.comparators) == 1:
            if isinstance(node.ops[0], (ast.Eq, ast.NotEq)) and isinstance(
                node.ops[0], ast.Eq
            ):
                operands = [node.left, node.comparators[0]]
        elif isinstance(node, ast.Call) and len(node.args) >= 2:
            # assertEqual / assertEquals, plus the aliased-helper pattern
            # CPython itself uses (eq = self.assertEqual; eq(call, expected)):
            # ANY 2+-arg call pairing a callee-call with a literal is read as
            # an expected-value vector. Over-extraction is the SAFE direction
            # here -- a spurious vector can only make the gate stricter
            # (refuse a universe), never license one.
            operands = list(node.args[:2])
        if len(operands) != 2:
            continue
        a, b = operands
        if _is_callee_call(a, fn_name):
            literal = _literal_text(b)
        elif _is_callee_call(b, fn_name):
            literal = _literal_text(a)
        else:
            continue
        if literal is not None:
            vectors.append(literal)
    return vectors


def _table_loop_charset(body: list, tree: ast.Module):
    """Match: acc-init, one for-loop accumulating table elements / literals,
    return join(acc) or acc. Returns (allowed_charset, None) on success,
    (None, reason) when the shape matched but a piece refuses, or None when
    the body is not table-loop shaped at all."""
    if len(body) != 3:
        return None
    init, loop, ret = body
    if not (
        isinstance(init, ast.Assign)
        and len(init.targets) == 1
        and isinstance(init.targets[0], ast.Name)
    ):
        return None
    acc = init.targets[0].id
    is_list = isinstance(init.value, ast.List) and not init.value.elts
    is_str = isinstance(init.value, ast.Constant) and init.value.value == ""
    if not (is_list or is_str):
        return None
    if not isinstance(loop, (ast.For,)):
        return None
    if not isinstance(ret, ast.Return) or ret.value is None:
        return None

    chars: set = set()

    # the return: "sep".join(acc) for list accumulators, bare acc for str
    rv = ret.value
    if (
        isinstance(rv, ast.Call)
        and isinstance(rv.func, ast.Attribute)
        and rv.func.attr == "join"
        and isinstance(rv.func.value, ast.Constant)
        and isinstance(rv.func.value.value, str)
        and len(rv.args) == 1
        and isinstance(rv.args[0], ast.Name)
        and rv.args[0].id == acc
    ):
        chars.update(rv.func.value.value)
    elif isinstance(rv, ast.Name) and rv.id == acc and is_str:
        pass
    else:
        return None

    # every write to acc inside the loop must be append(TABLE[i]) /
    # += TABLE[i] / a string literal; ANY other write anywhere refuses
    writes = []
    for node in ast.walk(loop):
        if (
            isinstance(node, ast.Expr)
            and isinstance(node.value, ast.Call)
            and isinstance(node.value.func, ast.Attribute)
            and isinstance(node.value.func.value, ast.Name)
            and node.value.func.value.id == acc
        ):
            if node.value.func.attr != "append" or len(node.value.args) != 1:
                return None, f"accumulator method '{node.value.func.attr}' is not a readable append"
            writes.append(node.value.args[0])
        elif (
            isinstance(node, ast.AugAssign)
            and isinstance(node.target, ast.Name)
            and node.target.id == acc
        ):
            if not isinstance(node.op, ast.Add):
                return None, "accumulator augmented with a non-concatenation op"
            writes.append(node.value)
        elif (
            isinstance(node, ast.Assign)
            and any(
                isinstance(t, ast.Name) and t.id == acc for t in node.targets
            )
        ):
            return None, "accumulator reassigned inside the loop"
    if not writes:
        return None

    for src in writes:
        piece = _table_piece_chars(src, tree)
        if piece is None:
            return None, "accumulated piece is not a pinned-table element or string literal"
        ok, payload = piece
        if not ok:
            return None, payload
        chars.update(payload)
    return "".join(sorted(chars)), None


def _table_piece_chars(src: ast.expr, tree: ast.Module):
    """Chars contributed by one accumulated piece: a string literal, or
    TABLE[<expr>] over a stable pinned str/tuple-of-str table. Returns
    (True, chars) / (False, refusal-reason) / None (not piece-shaped)."""
    if isinstance(src, ast.Constant) and isinstance(src.value, str):
        return True, src.value
    if (
        isinstance(src, ast.Subscript)
        and isinstance(src.value, ast.Name)
        and not isinstance(src.slice, ast.Slice)
    ):
        name = src.value.id
        binding = None
        for stmt in tree.body:
            if (
                isinstance(stmt, ast.Assign)
                and len(stmt.targets) == 1
                and isinstance(stmt.targets[0], ast.Name)
                and stmt.targets[0].id == name
            ):
                if binding is not None:
                    return False, f"table '{name}' bound more than once"
                binding = stmt
        if binding is None:
            return False, f"table '{name}' has no module-level binding"
        candidate = _Candidate(
            name=name, value=binding.value, line=binding.lineno, confession=None
        )
        events = [e for e in _binding_events(tree) if e.name == name]
        failure = _admission_failure(
            candidate, events, _global_declarations(tree).get(name)
        )
        if failure is not None:
            return False, f"table '{name}' is not stable: {failure}"
        v = binding.value
        if isinstance(v, ast.Constant) and isinstance(v.value, str):
            return True, v.value
        if isinstance(v, ast.Tuple) and all(
            isinstance(el, ast.Constant) and isinstance(el.value, str)
            for el in v.elts
        ):
            return True, "".join(el.value for el in v.elts)
        return False, f"table '{name}' is not a str or tuple-of-str literal"
    return None


def _table_subscript_shape(body: list) -> Optional[str]:
    """Match exactly one statement: return NAME[<expr>]. Returns NAME."""
    if len(body) != 1 or not isinstance(body[0], ast.Return):
        return None
    value = body[0].value
    if (
        isinstance(value, ast.Subscript)
        and isinstance(value.value, ast.Name)
        and not isinstance(value.slice, ast.Slice)
    ):
        return value.value.id
    return None


def _module_binding_tuple(
    tree: ast.Module, name: str
) -> Tuple[Optional[ast.Tuple], int]:
    binding: Optional[ast.Tuple] = None
    line = 0
    for stmt in tree.body:
        if (
            isinstance(stmt, ast.Assign)
            and len(stmt.targets) == 1
            and isinstance(stmt.targets[0], ast.Name)
            and stmt.targets[0].id == name
        ):
            if binding is not None:
                return None, 0
            if isinstance(stmt.value, ast.Tuple):
                binding = stmt.value
                line = stmt.lineno
            else:
                return None, 0
    return binding, line


def _rstrip_return_shape(body: list) -> Optional[bytes]:
    """Match a body whose LAST statement is return <expr>.rstrip(<bytes
    literal>). rstrip totality makes preceding statements irrelevant to the
    no-trailing-chars claim. Returns the stripped bytes literal."""
    if not body or not isinstance(body[-1], ast.Return):
        return None
    value = body[-1].value
    if (
        isinstance(value, ast.Call)
        and isinstance(value.func, ast.Attribute)
        and value.func.attr == "rstrip"
        and len(value.args) == 1
        and not value.keywords
        and isinstance(value.args[0], ast.Constant)
        and isinstance(value.args[0].value, bytes)
    ):
        return value.args[0].value
    return None


def _replace_return_shape(body: list):
    """Match a body whose LAST statement is return <expr>.replace(FROM, TO)
    with FROM, TO single-character string literals. replace is total over
    its from-string, so a SINGLE-char replace removes that char from the
    output entirely (unless TO reintroduces it) -- the same complement
    guarantee as translate, no binding scan needed (literals inline).
    Returns (from_char, to_char) or None."""
    if not body or not isinstance(body[-1], ast.Return):
        return None
    value = body[-1].value
    if (
        isinstance(value, ast.Call)
        and isinstance(value.func, ast.Attribute)
        and value.func.attr == "replace"
        and len(value.args) == 2
        and not value.keywords
        and all(
            isinstance(a, ast.Constant) and isinstance(a.value, str)
            and len(a.value) == 1
            for a in value.args
        )
    ):
        return value.args[0].value, value.args[1].value
    return None


def _translate_return_shape(body: list) -> Optional[str]:
    """Match exactly one statement: return <call>.translate(NAME).
    Returns NAME, or None when the body is not translate-shaped."""
    if len(body) != 1 or not isinstance(body[0], ast.Return):
        return None
    value = body[0].value
    if (
        isinstance(value, ast.Call)
        and isinstance(value.func, ast.Attribute)
        and value.func.attr == "translate"
        and isinstance(value.func.value, ast.Call)
        and len(value.args) == 1
        and not value.keywords
        and isinstance(value.args[0], ast.Name)
    ):
        return value.args[0].id
    return None


def _module_binding_call(
    tree: ast.Module, name: str
) -> Tuple[Optional[ast.Call], int]:
    binding: Optional[ast.Call] = None
    line = 0
    for stmt in tree.body:
        if (
            isinstance(stmt, ast.Assign)
            and len(stmt.targets) == 1
            and isinstance(stmt.targets[0], ast.Name)
            and stmt.targets[0].id == name
        ):
            if binding is not None:
                return None, 0
            if isinstance(stmt.value, ast.Call):
                binding = stmt.value
                line = stmt.lineno
            else:
                return None, 0
    return binding, line


def _maketrans_byte_literals(
    call: ast.Call,
) -> Optional[Tuple[bytes, bytes]]:
    func = call.func
    is_maketrans = (
        isinstance(func, ast.Attribute)
        and func.attr == "maketrans"
        and isinstance(func.value, ast.Name)
        and func.value.id in ("bytes", "bytearray")
    )
    if not is_maketrans or len(call.args) != 2 or call.keywords:
        return None
    frm, to = call.args
    if (
        isinstance(frm, ast.Constant)
        and isinstance(frm.value, bytes)
        and isinstance(to, ast.Constant)
        and isinstance(to.value, bytes)
    ):
        return frm.value, to.value
    return None
