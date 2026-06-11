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
                # No family matched: not a candidate, no refusal owed.
                return None, None
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
