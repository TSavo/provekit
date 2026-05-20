from __future__ import annotations

import ast
import json
import os
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterable

from .canonical import cid_of_json
from .ir import (
    Json,
    bool_const,
    ctor,
    fold_seq,
    function_contract,
    int_const,
    none_const,
    pass_stmt,
    source_unit_contract,
    str_const,
    var,
)

EXAM_MANIFEST_CID = (
    "blake3-512:b38426ba10ee3a6c28e9e32cae9aa65cfb5b750950464d1e67e9d669956bd40288d25c247d0ec2d638fd63e2d235d944f419055c0374c78488b4be98da040451"
)
EXAM_MANIFEST_BASENAME = f"v1.1.{EXAM_MANIFEST_CID}.json"


@dataclass
class LiftResult:
    ir: list[Json] = field(default_factory=list)
    diagnostics: list[Json] = field(default_factory=list)
    opacity_report: list[Json] = field(default_factory=list)
    refusals: list[Json] = field(default_factory=list)


@dataclass(frozen=True)
class _FunctionInfo:
    node: ast.AST
    qualname: str
    fn_name: str


class _UnsupportedSyntax(Exception):
    def __init__(
        self,
        node: ast.AST,
        reason: str,
        kind: str = "unhandled-syntax",
        question_kind: str | None = None,
        concept: str | None = None,
    ):
        self.node = node
        self.reason = reason
        self.kind = kind
        self.question_kind = question_kind
        self.concept = concept
        super().__init__(reason)


class _EffectSet:
    def __init__(self) -> None:
        self._effects: list[Json] = []
        self._seen: set[tuple[str, str]] = set()

    def add_reads(self, target: str) -> None:
        self._add(("reads", target), {"kind": "reads", "target": target})

    def add_writes(self, target: str) -> None:
        self._add(("writes", target), {"kind": "writes", "target": target})

    def add_io(self) -> None:
        self._add(("io", ""), {"kind": "io"})

    def add_panics(self) -> None:
        self._add(("panics", ""), {"kind": "panics"})

    def add_unresolved_call(self, name: str) -> None:
        self._add(("unresolved_call", name), {"kind": "unresolved_call", "name": name})

    def add_opaque_loop(self, loop_term: Json) -> None:
        loop_cid = cid_of_json(loop_term)
        self._add(
            ("opaque_loop", loop_cid),
            {"kind": "opaque_loop", "loopCid": loop_cid},
        )

    def sorted(self) -> list[Json]:
        return sorted(self._effects, key=_effect_sort_key)

    def _add(self, key: tuple[str, str], effect: Json) -> None:
        if key in self._seen:
            return
        self._seen.add(key)
        self._effects.append(effect)


def lift_source(source: str, source_path: str) -> LiftResult:
    result = LiftResult()
    try:
        tree = ast.parse(source, filename=source_path)
    except SyntaxError as exc:
        result.refusals.append(
            _refusal(
                "syntax-error",
                None,
                exc.lineno,
                exc.msg,
                result.diagnostics,
                question_kind="morphism",
                concept="concept:source-unit",
            )
        )
        result.ir.append(
            source_unit_contract(
                source_path=source_path,
                source=source,
                operational_term=pass_stmt(),
            )
        )
        return result

    module_path = _module_path(source_path)
    module_globals = _module_global_names(tree)
    collector = _DefinitionCollector(module_path)
    collector.visit(tree)

    body_terms: list[Json] = []
    contracts: list[Json] = []
    for info in collector.definitions:
        contract = _lift_function(info, source_path, module_globals, result)
        if contract is None:
            continue
        body_terms.append(contract["post"]["args"][1])
        contracts.append(contract)

    result.ir.append(
        source_unit_contract(
            source_path=source_path,
            source=source,
            operational_term=fold_seq(body_terms),
        )
    )
    result.ir.extend(contracts)
    return result


def lift_paths(workspace_root: str, source_paths: Iterable[str]) -> LiftResult:
    result = LiftResult()
    root = Path(workspace_root or ".").resolve()
    for requested in source_paths:
        path = Path(requested)
        full = path if path.is_absolute() else root / path
        try:
            resolved = full.resolve()
        except OSError as exc:
            result.refusals.append(
                _refusal(
                    "io-error",
                    None,
                    None,
                    f"cannot resolve path '{requested}': {exc}",
                    result.diagnostics,
                )
            )
            continue
        if not _is_relative_to(resolved, root):
            result.refusals.append(
                _refusal(
                    "path-traversal",
                    None,
                    None,
                    f"path '{requested}' escapes workspace root '{root}'",
                    result.diagnostics,
                )
            )
            continue
        for file_path in _iter_python_files(resolved):
            try:
                source = file_path.read_text(encoding="utf-8")
            except OSError as exc:
                result.refusals.append(
                    _refusal(
                        "io-error",
                        None,
                        None,
                        f"cannot read '{file_path}': {exc}",
                        result.diagnostics,
                    )
                )
                continue
            display_path = os.path.relpath(file_path, root)
            file_result = lift_source(source, display_path)
            result.ir.extend(file_result.ir)
            result.diagnostics.extend(file_result.diagnostics)
            result.opacity_report.extend(file_result.opacity_report)
            result.refusals.extend(file_result.refusals)
    return result


class _DefinitionCollector(ast.NodeVisitor):
    def __init__(self, module_path: str):
        self.module_path = module_path
        self.scope: list[tuple[str, str]] = []
        self.definitions: list[_FunctionInfo] = []

    def visit_ClassDef(self, node: ast.ClassDef) -> Any:
        self.scope.append(("class", node.name))
        for stmt in node.body:
            self.visit(stmt)
        self.scope.pop()

    def visit_FunctionDef(self, node: ast.FunctionDef) -> Any:
        self._record_function(node)
        self.scope.append(("function", node.name))
        for stmt in node.body:
            self.visit(stmt)
        self.scope.pop()

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> Any:
        self._record_function(node)
        self.scope.append(("function", node.name))
        for stmt in node.body:
            self.visit(stmt)
        self.scope.pop()

    def _record_function(self, node: ast.AST) -> None:
        assert isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
        qualname = _qualname(self.scope, node.name)
        self.definitions.append(
            _FunctionInfo(
                node=node,
                qualname=qualname,
                fn_name=f"{self.module_path}.{qualname}",
            )
        )


class _LocalCollector(ast.NodeVisitor):
    def __init__(self) -> None:
        self.names: set[str] = set()

    def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
        return

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
        return

    def visit_ClassDef(self, node: ast.ClassDef) -> None:
        return

    def visit_Name(self, node: ast.Name) -> None:
        if isinstance(node.ctx, (ast.Store, ast.Del)):
            self.names.add(node.id)


class _Emitter:
    def __init__(
        self,
        *,
        fn_name: str,
        locals_: set[str],
        module_globals: set[str],
        effects: _EffectSet,
    ) -> None:
        self.fn_name = fn_name
        self.locals = set(locals_)
        self.module_globals = module_globals
        self.effects = effects

    def statements(self, statements: list[ast.stmt]) -> Json:
        emitted: list[Json] = []
        for statement in statements:
            if _is_docstring_stmt(statement):
                continue
            emitted.append(self.statement(statement))
        return fold_seq(emitted)

    def statement(self, node: ast.stmt) -> Json:
        if isinstance(node, ast.Return):
            value = none_const() if node.value is None else self.expr(node.value)
            return ctor("python:return", value)
        if isinstance(node, ast.Assign):
            if len(node.targets) != 1:
                raise _UnsupportedSyntax(node, "multiple-target assignment is refused")
            target = self.target(node.targets[0])
            self._record_write_if_nonlocal(node.targets[0])
            return ctor("python:assign", target, self.expr(node.value))
        if isinstance(node, ast.If):
            return ctor(
                "python:if",
                self.expr(node.test),
                self.statements(node.body),
                self.statements(node.orelse) if node.orelse else pass_stmt(),
            )
        if isinstance(node, ast.While):
            if node.orelse:
                raise _UnsupportedSyntax(node, "while/else is refused")
            term = ctor("python:while", self.expr(node.test), self.statements(node.body))
            self.effects.add_opaque_loop(term)
            return term
        if isinstance(node, ast.For):
            if node.orelse:
                raise _UnsupportedSyntax(node, "for/else is refused")
            target = self.target(node.target)
            self._record_write_if_nonlocal(node.target)
            term = ctor(
                "python:for",
                target,
                self.expr(node.iter),
                self.statements(node.body),
            )
            self.effects.add_opaque_loop(term)
            return term
        if isinstance(node, ast.Expr):
            return ctor("python:expr", self.expr(node.value))
        if isinstance(node, ast.Pass):
            return pass_stmt()
        if isinstance(node, ast.Break):
            return ctor("python:break", none_const())
        if isinstance(node, ast.Continue):
            return ctor("python:continue", none_const())
        if isinstance(node, ast.Raise):
            self.effects.add_panics()
            value = none_const() if node.exc is None else self.expr(node.exc)
            return ctor("python:raise", value)
        raise _UnsupportedSyntax(node, f"unhandled statement kind: {type(node).__name__}")

    def target(self, node: ast.expr) -> Json:
        if isinstance(node, ast.Name):
            return var(node.id)
        if isinstance(node, ast.Attribute):
            return ctor("python:attribute", self.expr(node.value), str_const(node.attr))
        if isinstance(node, ast.Subscript):
            return ctor("python:subscript", self.expr(node.value), self.subscript_index(node))
        raise _UnsupportedSyntax(node, f"unsupported assignment target: {type(node).__name__}")

    def expr(self, node: ast.expr) -> Json:
        if isinstance(node, ast.Constant):
            return self.constant(node)
        if isinstance(node, ast.Name):
            self._record_read_if_global(node.id)
            return var(node.id)
        if isinstance(node, ast.BinOp):
            op = _BINOPS.get(type(node.op))
            if op is None:
                raise _UnsupportedSyntax(node, f"unsupported binary operator: {type(node.op).__name__}")
            return ctor(op, self.expr(node.left), self.expr(node.right))
        if isinstance(node, ast.UnaryOp):
            op = _UNARYOPS.get(type(node.op))
            if op is None:
                raise _UnsupportedSyntax(node, f"unsupported unary operator: {type(node.op).__name__}")
            return ctor(op, self.expr(node.operand))
        if isinstance(node, ast.BoolOp):
            op = "python:and" if isinstance(node.op, ast.And) else "python:or"
            if len(node.values) < 2:
                raise _UnsupportedSyntax(node, "boolean operation without two operands")
            values = [self.expr(value) for value in node.values]
            result = ctor(op, values[0], values[1])
            for value in values[2:]:
                result = ctor(op, result, value)
            return result
        if isinstance(node, ast.Compare):
            return self.compare(node)
        if isinstance(node, ast.Call):
            return self.call(node)
        if isinstance(node, ast.Attribute):
            return ctor("python:attribute", self.expr(node.value), str_const(node.attr))
        if isinstance(node, ast.Subscript):
            return ctor("python:subscript", self.expr(node.value), self.subscript_index(node))
        question_kind, concept = _citation_for_unhandled_expr(node)
        raise _UnsupportedSyntax(
            node,
            f"unhandled expression kind: {type(node).__name__}",
            question_kind=question_kind,
            concept=concept,
        )

    def constant(self, node: ast.Constant) -> Json:
        value = node.value
        if isinstance(value, bool):
            return bool_const(value)
        if isinstance(value, int):
            return int_const(value)
        if isinstance(value, str):
            return str_const(value)
        if value is None:
            return none_const()
        raise _UnsupportedSyntax(node, f"unsupported constant: {type(value).__name__}")

    def compare(self, node: ast.Compare) -> Json:
        if not node.ops or len(node.ops) != len(node.comparators):
            raise _UnsupportedSyntax(node, "malformed comparison expression")
        operands: list[ast.expr] = [node.left, *node.comparators]
        comparisons: list[Json] = []
        for index, raw_op in enumerate(node.ops):
            op = _CMPOPS.get(type(raw_op))
            if op is None:
                raise _UnsupportedSyntax(
                    node,
                    f"unsupported comparison operator: {type(raw_op).__name__}",
                )
            comparisons.append(
                ctor(
                    "python:compare",
                    str_const(op),
                    self.expr(operands[index]),
                    self.expr(operands[index + 1]),
                )
            )
        result = comparisons[0]
        for comparison in comparisons[1:]:
            result = ctor("python:and", result, comparison)
        return result

    def call(self, node: ast.Call) -> Json:
        if node.keywords:
            raise _UnsupportedSyntax(node, "keyword arguments are refused")
        for arg in node.args:
            if isinstance(arg, ast.Starred):
                raise _UnsupportedSyntax(arg, "starred call arguments are refused")
        callee = _callee_name(node.func)
        if callee == "print":
            self.effects.add_io()
        else:
            self.effects.add_unresolved_call(callee)
        return ctor("python:call", str_const(callee), *[self.expr(arg) for arg in node.args])

    def subscript_index(self, node: ast.Subscript) -> Json:
        if isinstance(node.slice, ast.Slice):
            raise _UnsupportedSyntax(node.slice, "slice subscripts are refused")
        return self.expr(node.slice)

    def _record_read_if_global(self, name: str) -> None:
        if name in self.module_globals and name not in self.locals:
            self.effects.add_reads(name)

    def _record_write_if_nonlocal(self, node: ast.expr) -> None:
        if isinstance(node, ast.Attribute):
            self.effects.add_writes(_target_text(node))
        elif isinstance(node, ast.Subscript):
            self.effects.add_writes(_target_text(node))


def _lift_function(
    info: _FunctionInfo,
    source_path: str,
    module_globals: set[str],
    result: LiftResult,
) -> Json | None:
    node = info.node
    assert isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
    try:
        if isinstance(node, ast.AsyncFunctionDef):
            raise _UnsupportedSyntax(node, "async functions are refused")
        if node.decorator_list:
            raise _UnsupportedSyntax(node, "decorated functions are refused")
        if node.args.vararg is not None or node.args.kwarg is not None:
            raise _UnsupportedSyntax(node, "*args and **kwargs are refused")
        if node.args.posonlyargs:
            raise _UnsupportedSyntax(node, "positional-only parameters are refused")
        if node.args.kwonlyargs:
            raise _UnsupportedSyntax(node, "keyword-only parameters are refused")
        if node.args.defaults or node.args.kw_defaults:
            raise _UnsupportedSyntax(node, "default parameter values are refused")
        refused = _contains_refused_control(node)
        if refused is not None:
            raise refused

        formals = [arg.arg for arg in node.args.args]
        locals_ = _function_locals(node, formals)
        effects = _EffectSet()
        emitter = _Emitter(
            fn_name=info.fn_name,
            locals_=locals_,
            module_globals=module_globals,
            effects=effects,
        )
        body = emitter.statements(node.body)
        return function_contract(
            fn_name=info.fn_name,
            formals=formals,
            body_term=body,
            effects=effects.sorted(),
            source_path=source_path,
            line=node.lineno,
        )
    except _UnsupportedSyntax as exc:
        result.refusals.append(
            _refusal(
                exc.kind,
                info.fn_name,
                getattr(exc.node, "lineno", getattr(node, "lineno", None)),
                exc.reason,
                result.diagnostics,
                question_kind=exc.question_kind,
                concept=exc.concept,
            )
        )
        return None


def _refusal(
    kind: str,
    function: str | None,
    line: int | None,
    reason: str,
    diagnostics: list[Json],
    *,
    question_kind: str | None = None,
    concept: str | None = None,
) -> Json:
    refusal: Json = {
        "kind": kind,
        "function": function,
        "line": line,
        "reason": reason,
    }
    if question_kind is None or concept is None:
        diagnostics.append(
            {
                "kind": "exam-question-citation-missing",
                "refusal_kind": kind,
                "reason": "refusal is outside the v1.1 exam vocabulary",
            }
        )
        return refusal
    question_cid = _exam_question_cid_for(question_kind, concept, "python")
    if question_cid is None:
        diagnostics.append(
            {
                "kind": "exam-question-citation-missing",
                "question_kind": question_kind,
                "concept": concept,
                "language": "python",
            }
        )
        return refusal
    refusal["exam_manifest_cid"] = EXAM_MANIFEST_CID
    refusal["exam_question_cid"] = question_cid
    return refusal


def _citation_for_unhandled_expr(node: ast.AST) -> tuple[str, str]:
    if isinstance(node, ast.ListComp):
        return ("sort-classification", "concept:List<T>")
    return ("sort-classification", "concept:Term")


def _exam_question_cid_for(kind: str, concept: str, language: str) -> str | None:
    manifest = _load_exam_manifest()
    if manifest is None:
        return None
    for question in manifest.get("header", {}).get("content", {}).get("questions", []):
        if question.get("kind") != kind or question.get("concept") != concept:
            continue
        parameters = question.get("parameters", {})
        for language_key in _exam_language_keys(kind):
            if parameters.get(language_key) == language:
                return cid_of_json(question)
    return None


def _exam_language_keys(kind: str) -> tuple[str, ...]:
    if kind == "morphism":
        return ("from_language",)
    if kind in {"boundary-realization", "realization"}:
        return ("target_language",)
    if kind in {
        "concept-realization",
        "effect-classification",
        "sort-classification",
        "effect",
        "sort",
    }:
        return ("language",)
    return ("language", "from_language", "target_language")


def _load_exam_manifest() -> Json | None:
    env_path = os.environ.get("PROVEKIT_EXAM_MANIFEST")
    paths: list[Path] = []
    if env_path:
        paths.append(Path(env_path))
    for base in [Path.cwd(), *Path(__file__).resolve().parents]:
        paths.append(base / "menagerie" / "concept-shapes" / "exams" / EXAM_MANIFEST_BASENAME)
    for path in paths:
        try:
            if path.is_file():
                return json.loads(path.read_text(encoding="utf-8"))
        except OSError:
            continue
    return None


def _contains_refused_control(fn: ast.FunctionDef) -> _UnsupportedSyntax | None:
    for child in ast.walk(fn):
        if child is fn:
            continue
        if isinstance(child, (ast.Global, ast.Nonlocal)):
            return _UnsupportedSyntax(child, "global/nonlocal declarations are refused")
        if isinstance(child, (ast.Yield, ast.YieldFrom)):
            return _UnsupportedSyntax(child, "generators are refused")
        if isinstance(child, ast.Await):
            return _UnsupportedSyntax(child, "await expressions are refused")
        if isinstance(child, ast.NamedExpr):
            return _UnsupportedSyntax(child, "walrus expressions are refused")
    return None


def _function_locals(fn: ast.FunctionDef, formals: list[str]) -> set[str]:
    collector = _LocalCollector()
    for statement in fn.body:
        collector.visit(statement)
    return set(formals) | collector.names


def _module_global_names(tree: ast.Module) -> set[str]:
    names: set[str] = set()
    for stmt in tree.body:
        if isinstance(stmt, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            names.add(stmt.name)
        elif isinstance(stmt, ast.Assign):
            for target in stmt.targets:
                names.update(_stored_names(target))
        elif isinstance(stmt, ast.AnnAssign):
            names.update(_stored_names(stmt.target))
    return names


def _stored_names(node: ast.AST) -> set[str]:
    if isinstance(node, ast.Name):
        return {node.id}
    if isinstance(node, (ast.Tuple, ast.List)):
        names: set[str] = set()
        for elt in node.elts:
            names.update(_stored_names(elt))
        return names
    return set()


def _module_path(source_path: str) -> str:
    path = Path(source_path)
    without_suffix = path.with_suffix("")
    parts = [part for part in without_suffix.parts if part not in {"", "."}]
    if parts and parts[-1] == "__init__":
        parts = parts[:-1]
    cleaned = [_clean_identifier_part(part) for part in parts]
    return ".".join(cleaned) if cleaned else "__main__"


def _clean_identifier_part(part: str) -> str:
    out = "".join(ch if ch.isidentifier() or ch.isdigit() else "_" for ch in part)
    if not out or out[0].isdigit():
        out = "_" + out
    return out


def _qualname(scope: list[tuple[str, str]], name: str) -> str:
    parts: list[str] = []
    for kind, scope_name in scope:
        if kind == "class":
            parts.append(scope_name)
        elif kind == "function":
            parts.extend([scope_name, "<locals>"])
    parts.append(name)
    return ".".join(parts)


def _is_docstring_stmt(statement: ast.stmt) -> bool:
    return (
        isinstance(statement, ast.Expr)
        and isinstance(statement.value, ast.Constant)
        and isinstance(statement.value.value, str)
    )


def _callee_name(node: ast.expr) -> str:
    if isinstance(node, ast.Name):
        return node.id
    if isinstance(node, ast.Attribute):
        base = _callee_name(node.value)
        return f"{base}.{node.attr}" if base else node.attr
    raise _UnsupportedSyntax(node, f"unsupported callee kind: {type(node).__name__}")


def _target_text(node: ast.AST) -> str:
    try:
        return ast.unparse(node)
    except Exception:
        return type(node).__name__


def _iter_python_files(path: Path) -> Iterable[Path]:
    if path.is_file():
        if path.suffix == ".py":
            yield path
        return
    if not path.is_dir():
        return
    ignored = {".git", ".venv", "venv", "__pycache__", ".mypy_cache", ".pytest_cache"}
    for dirpath, dirnames, filenames in os.walk(path):
        dirnames[:] = [dirname for dirname in dirnames if dirname not in ignored]
        for filename in filenames:
            if filename.endswith(".py"):
                yield Path(dirpath) / filename


def _is_relative_to(path: Path, root: Path) -> bool:
    try:
        path.relative_to(root)
        return True
    except ValueError:
        return False


def _effect_sort_key(effect: Json) -> str:
    kind = effect.get("kind")
    if kind == "reads":
        return f"0:reads:{effect.get('target', '')}"
    if kind == "writes":
        return f"1:writes:{effect.get('target', '')}"
    if kind == "io":
        return "2:io"
    if kind == "unsafe":
        return "3:unsafe"
    if kind == "panics":
        return "4:panics"
    if kind == "unresolved_call":
        return f"5:unresolved:{effect.get('name', '')}"
    if kind == "opaque_loop":
        return f"6:opaque_loop:{effect.get('loopCid', '')}"
    return f"99:{kind}"


_BINOPS: dict[type[ast.operator], str] = {
    ast.Add: "python:add",
    ast.Sub: "python:sub",
    ast.Mult: "python:mul",
    ast.Div: "python:div",
    ast.FloorDiv: "python:floordiv",
    ast.Mod: "python:mod",
    ast.Pow: "python:pow",
    ast.LShift: "python:lshift",
    ast.RShift: "python:rshift",
    ast.BitAnd: "python:bitand",
    ast.BitOr: "python:bitor",
    ast.BitXor: "python:bitxor",
}

_UNARYOPS: dict[type[ast.unaryop], str] = {
    ast.USub: "python:neg",
    ast.UAdd: "python:pos",
    ast.Not: "python:not",
    ast.Invert: "python:bitnot",
}

_CMPOPS: dict[type[ast.cmpop], str] = {
    ast.Eq: "==",
    ast.NotEq: "!=",
    ast.Lt: "<",
    ast.LtE: "<=",
    ast.Gt: ">",
    ast.GtE: ">=",
    ast.Is: "is",
    ast.IsNot: "is not",
    ast.In: "in",
    ast.NotIn: "not in",
}
