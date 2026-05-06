#!/usr/bin/env python3
"""Catalog-pinned cross-kit conformance harness.

This is deliberately not a user-facing CLI contract. Each adapter exists only
so the central conformance suite can force language-native kit machinery to
emit catalog-pinned protocol bytes.
"""

from __future__ import annotations

import argparse
import difflib
import json
import os
import platform
import shutil
import subprocess
import sys
import tempfile
import textwrap
import tomllib
from dataclasses import dataclass
from functools import lru_cache
from pathlib import Path
from typing import Callable


ROOT = Path(__file__).resolve().parent.parent
FIXTURES_TOML = ROOT / "conformance" / "fixtures.toml"
CATALOG_JSON = ROOT / "protocol" / "specs" / "2026-04-30-protocol-catalog.json"
PY_SRC = ROOT / "implementations" / "python" / "provekit-lift-py-tests" / "src"
ZIG = ROOT / "zig-toolchain" / "zig"

EXPECTED_CATALOG_VERSION = "v1.6.0-2026-05-05"
EXPECTED_CATALOG_CID = (
    "blake3-512:"
    "ce04a40534986a95362d5f130fd3a1a667b7a157f0554f262af11ec7a2ac8e8b80"
    "f56c36cca93d7a180535eedc99949d760fce6ab63c405de8837fa20f00e781"
)

CORE_FIXTURES = (
    "eq_atomic",
    "pattern1_bounded_loop",
    "contract_decl",
    "bridge_decl_v1_1",
)

RUST_CORE_FIXTURES = (
    "eq_atomic",
    "pattern1_bounded_loop",
    "contract_decl",
)

ALL_KITS = (
    "rust",
    "go",
    "cpp",
    "typescript",
    "csharp",
    "java",
    "python",
    "c",
    "zig",
    "php",
    "swift",
    "ruby",
)

PROFILE_REQUIRED_KITS = {
    "linux": tuple(k for k in ALL_KITS if k != "swift"),
    "swift": ("swift",),
    "all": ALL_KITS,
}


class C:
    BOLD = "\033[1m"
    DIM = "\033[2m"
    RED = "\033[0;31m"
    GREEN = "\033[0;32m"
    YELLOW = "\033[1;33m"
    CYAN = "\033[0;36m"
    RST = "\033[0m"


if os.environ.get("NO_COLOR"):
    for name in ("BOLD", "DIM", "RED", "GREEN", "YELLOW", "CYAN", "RST"):
        setattr(C, name, "")


@dataclass(frozen=True)
class Fixture:
    name: str
    capability: str
    description: str
    jcs: str
    hash: str


@dataclass(frozen=True)
class ProcResult:
    returncode: int
    stdout: str
    stderr: str


def run(cmd: list[str], cwd: Path, timeout: int = 120) -> ProcResult:
    try:
        p = subprocess.run(
            cmd,
            cwd=cwd,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        return ProcResult(p.returncode, p.stdout, p.stderr)
    except FileNotFoundError as e:
        return ProcResult(127, "", str(e))
    except subprocess.TimeoutExpired as e:
        stdout = e.stdout.decode() if isinstance(e.stdout, bytes) else (e.stdout or "")
        stderr = e.stderr.decode() if isinstance(e.stderr, bytes) else (e.stderr or "")
        return ProcResult(-1, stdout, stderr + f"\ntimeout after {timeout}s")


def require_tool(name: str) -> None:
    if name == "zig":
        if ZIG.exists() or shutil.which("zig"):
            return
    elif shutil.which(name):
        return
    raise RuntimeError(f"required tool not found on PATH: {name}")


def show_diff(got: str, want: str) -> str:
    got_lines = got.replace(",", ",\n").splitlines()
    want_lines = want.replace(",", ",\n").splitlines()
    out: list[str] = []
    for line in difflib.unified_diff(
        want_lines, got_lines, fromfile="expected", tofile="got", lineterm=""
    ):
        if line.startswith(("---", "+++", "@@")):
            continue
        if line.startswith("+"):
            out.append(f"{C.RED}{line}{C.RST}")
        elif line.startswith("-"):
            out.append(f"{C.GREEN}{line}{C.RST}")
        else:
            out.append(f"{C.DIM}{line}{C.RST}")
    return "\n".join(out)


def load_fixtures() -> tuple[dict[str, str], dict[str, Fixture]]:
    with FIXTURES_TOML.open("rb") as f:
        raw = tomllib.load(f)

    metadata = {
        "catalog_version": raw.get("catalog_version", ""),
        "catalog_cid": raw.get("catalog_cid", ""),
    }
    fixtures: dict[str, Fixture] = {}
    for entry in raw.get("fixture", []):
        fixture = Fixture(
            name=entry["name"],
            capability=entry["capability"],
            description=entry["description"],
            jcs=entry["jcs"],
            hash=entry["hash"],
        )
        fixtures[fixture.name] = fixture
    return metadata, fixtures


def assert_catalog_pin(metadata: dict[str, str]) -> None:
    import json

    if metadata["catalog_version"] != EXPECTED_CATALOG_VERSION:
        raise RuntimeError(
            f"fixtures.toml catalog_version={metadata['catalog_version']!r}; "
            f"expected {EXPECTED_CATALOG_VERSION!r}"
        )
    if metadata["catalog_cid"] != EXPECTED_CATALOG_CID:
        raise RuntimeError("fixtures.toml catalog_cid does not match v1.6.0")

    catalog = json.loads(CATALOG_JSON.read_text())
    if catalog.get("version") != EXPECTED_CATALOG_VERSION:
        raise RuntimeError(
            f"protocol catalog version={catalog.get('version')!r}; "
            f"expected {EXPECTED_CATALOG_VERSION!r}"
        )


def emit_rust(name: str) -> str:
    require_tool("cargo")
    code = f"""
use provekit_canonicalizer::{{encode_jcs, Value}};
use provekit_ir_symbolic::serialize::formula_to_value;
use provekit_ir_symbolic::{{
    and_, eq, gte, implies, lt, make_var, num, str_const, ContractArgs, Int, Term,
}};
use std::rc::Rc;

fn parse_int_arg(arg: Rc<Term>) -> Rc<Term> {{
    Rc::new(Term::Ctor {{
        name: "parse_int".into(),
        args: vec![arg],
    }})
}}

fn main() {{
    match {json.dumps(name)} {{
        "eq_atomic" => {{
            let f = eq(parse_int_arg(str_const("42")), num(42));
            print!("{{}}", encode_jcs(&formula_to_value(&f)));
        }}
        "pattern1_bounded_loop" => {{
            let x = make_var("x");
            let body = implies(
                and_(vec![gte(x.clone(), num(0)), lt(x.clone(), num(100))]),
                gte(x, num(0)),
            );
            let q = provekit_ir_symbolic::Formula::Quantifier {{
                kind: "forall".into(),
                name: "x".into(),
                sort: Int(),
                body,
            }};
            print!("{{}}", encode_jcs(&formula_to_value(&q)));
        }}
        "contract_decl" => {{
            let pre = gte(make_var("x"), num(0));
            let value = Value::array(vec![Value::object([
                ("kind", Value::string("contract")),
                ("name", Value::string("parseInt")),
                ("outBinding", Value::string("out")),
                ("pre", formula_to_value(&pre)),
            ])]);
            print!("{{}}", encode_jcs(&value));
        }}
        _ => panic!("unknown fixture"),
    }}
}}
"""
    with tempfile.TemporaryDirectory(prefix="pk_rust_conformance_") as tmp:
        d = Path(tmp)
        (d / "Cargo.toml").write_text(
            textwrap.dedent(
                f"""
                [package]
                name = "pk-rust-conformance"
                version = "0.0.0"
                edition = "2021"

                [dependencies]
                provekit-canonicalizer = {{ path = "{ROOT / 'implementations' / 'rust' / 'provekit-canonicalizer'}" }}
                provekit-ir-symbolic = {{ path = "{ROOT / 'implementations' / 'rust' / 'provekit-ir-symbolic'}" }}
                """
            )
        )
        (d / "src").mkdir()
        (d / "src" / "main.rs").write_text(textwrap.dedent(code))
        p = run(["cargo", "run", "--quiet"], cwd=d, timeout=180)
        if p.returncode != 0:
            raise RuntimeError(p.stderr or p.stdout)
        return p.stdout.strip()


def emit_python(name: str) -> str:
    sys.path.insert(0, str(PY_SRC))
    from provekit_lift_py_tests.canonicalizer import encode_jcs
    from provekit_lift_py_tests.ir import (
        BridgeDecl,
        ContractDecl,
        Int,
        _Quantifier,
        and_,
        bridge_decl_to_value,
        contract_decl_to_value,
        ctor,
        declarations_to_value,
        eq,
        formula_to_value,
        gte,
        implies,
        lt,
        make_var,
        num,
        str_const,
    )

    if name == "eq_atomic":
        return encode_jcs(formula_to_value(eq(ctor("parse_int", [str_const("42")]), num(42))))
    if name == "pattern1_bounded_loop":
        x = make_var("x")
        body = implies(and_([gte(x, num(0)), lt(x, num(100))]), gte(x, num(0)))
        return encode_jcs(formula_to_value(_Quantifier("forall", "x", Int(), body)))
    if name == "contract_decl":
        pre = gte(make_var("x"), num(0))
        return encode_jcs(declarations_to_value([ContractDecl(name="parseInt", pre=pre)]))
    if name == "bridge_decl_v1_1":
        bridge = BridgeDecl(
            name="myBridge",
            source_symbol="source",
            source_layer="c-kit",
            source_contract_cid="bafySource",
            target_contract_cid="bafyTarget",
            target_proof_cid="bafyProof",
            target_layer="coq",
            notes="some notes",
        )
        return encode_jcs(bridge_decl_to_value(bridge))
    raise KeyError(name)


def emit_go(name: str) -> str:
    require_tool("go")
    module = ROOT / "implementations" / "go" / "provekit-ir-symbolic"
    code = f"""
package main

import (
    "encoding/json"
    "fmt"
    "strings"
    canon "github.com/tsavo/provekit/go/provekit-ir-symbolic/canonicalizer"
    ir "github.com/tsavo/provekit/go/provekit-ir-symbolic/ir"
)

func emit(raw []byte) {{
    var v any
    dec := json.NewDecoder(strings.NewReader(string(raw)))
    dec.UseNumber()
    if err := dec.Decode(&v); err != nil {{
        panic(err)
    }}
    out, err := canon.EncodeJCS(v)
    if err != nil {{
        panic(err)
    }}
    fmt.Print(string(out))
}}

func main() {{
    switch {json.dumps(name)} {{
    case "eq_atomic":
        f := ir.Eq(ir.MakeCtor("parse_int", []ir.IrTerm{{ir.StrConst("42")}}, ir.Int), ir.Num(42))
        out, _ := json.Marshal(f)
        emit(out)
    case "pattern1_bounded_loop":
        f := ir.ForAllNamed("x", ir.Int, func(x ir.IrTerm) ir.IrFormula {{
            return ir.Implies(ir.And(ir.Gte(x, ir.Num(0)), ir.Lt(x, ir.Num(100))), ir.Gte(x, ir.Num(0)))
        }})
        out, _ := json.Marshal(f)
        emit(out)
    case "contract_decl":
        ir.ResetCollector()
        finish := ir.BeginCollecting()
        ir.Contract("parseInt", ir.ContractArgs{{Pre: ir.Gte(ir.MakeVar("x", ir.Int), ir.Num(0))}})
        decls := finish()
        out, _ := ir.MarshalDeclarations(decls)
        emit(out)
    case "bridge_decl_v1_1":
        b := ir.BridgeDeclaration{{
            Name: "myBridge",
            SourceSymbol: "source",
            SourceLayer: "c-kit",
            SourceContractCid: "bafySource",
            TargetContractCid: "bafyTarget",
            TargetProofCid: "bafyProof",
            TargetLayer: "coq",
            Notes: "some notes",
        }}
        out, _ := json.Marshal(b)
        emit(out)
    default:
        panic("unknown fixture")
    }}
}}
"""
    with tempfile.NamedTemporaryFile("w", suffix=".go", dir=module, delete=False) as f:
        f.write(textwrap.dedent(code))
        src = Path(f.name)
    try:
        p = run(["go", "run", src.name], cwd=module, timeout=60)
        if p.returncode != 0:
            raise RuntimeError(p.stderr or p.stdout)
        return p.stdout.strip()
    finally:
        src.unlink(missing_ok=True)


def emit_c(name: str) -> str:
    require_tool("cc")
    c_dir = ROOT / "implementations" / "c" / "provekit-ir"
    include = c_dir / "include"
    src_files = [c_dir / "src" / f for f in ("ir.c", "jcs.c")]
    body = {
        "eq_atomic": r'''
            pk_term *arg = pk_term_const_str("42", pk_sort_primitive("String"));
            pk_term *args1[] = { arg };
            pk_term *lhs = pk_term_ctor_new("parse_int", args1, 1);
            pk_term *rhs = pk_term_const_int(42, pk_sort_primitive("Int"));
            pk_term *args2[] = { lhs, rhs };
            pk_formula *f = pk_formula_atomic_new("=", args2, 2);
            pk_emit_formula(buf, f);
            pk_formula_free(f);
        ''',
        "pattern1_bounded_loop": r'''
            pk_term *x1 = pk_term_var_new("x");
            pk_term *zero1 = pk_term_const_int(0, pk_sort_primitive("Int"));
            pk_term *lower_args[] = { x1, zero1 };
            pk_formula *lower = pk_formula_atomic_new("≥", lower_args, 2);
            pk_term *x2 = pk_term_var_new("x");
            pk_term *hundred = pk_term_const_int(100, pk_sort_primitive("Int"));
            pk_term *upper_args[] = { x2, hundred };
            pk_formula *upper = pk_formula_atomic_new("<", upper_args, 2);
            pk_formula *conj[] = { lower, upper };
            pk_formula *ant = pk_formula_connective_new("and", conj, 2);
            pk_term *x3 = pk_term_var_new("x");
            pk_term *zero2 = pk_term_const_int(0, pk_sort_primitive("Int"));
            pk_term *inner_args[] = { x3, zero2 };
            pk_formula *inner = pk_formula_atomic_new("≥", inner_args, 2);
            pk_formula *impl[] = { ant, inner };
            pk_formula *body = pk_formula_connective_new("implies", impl, 2);
            pk_formula *q = pk_formula_quantifier_new("forall", "x", pk_sort_primitive("Int"), body);
            pk_emit_formula(buf, q);
            pk_formula_free(q);
        ''',
        "contract_decl": r'''
            pk_term *x = pk_term_var_new("x");
            pk_term *zero = pk_term_const_int(0, pk_sort_primitive("Int"));
            pk_term *args[] = { x, zero };
            pk_formula *pre = pk_formula_atomic_new("≥", args, 2);
            pk_decl *d = pk_decl_contract_new("parseInt", "out", pre, NULL, NULL);
            pk_decl *decls[] = { d };
            pk_emit_decls(buf, decls, 1);
            pk_decl_free(d);
        ''',
        "bridge_decl_v1_1": r'''
            pk_decl *d = pk_decl_bridge_new("myBridge", "source", "c-kit", "bafySource", "bafyTarget", "bafyProof", "coq", "some notes");
            pk_emit_decl(buf, d);
            pk_decl_free(d);
        ''',
    }[name]
    code = f"""
        #include "provekit/ir.h"
        #include <stdio.h>
        int main(void) {{
            pk_buffer *buf = pk_buffer_new();
            {body}
            printf("%s", buf->data);
            pk_buffer_free(buf);
            return 0;
        }}
    """
    with tempfile.NamedTemporaryFile("w", suffix=".c", delete=False) as f:
        f.write(textwrap.dedent(code))
        src = Path(f.name)
    out = src.with_suffix(".out")
    try:
        cmd = ["cc", "-std=c11", "-I", str(include), str(src)] + [str(s) for s in src_files] + ["-o", str(out)]
        p = run(cmd, cwd=ROOT)
        if p.returncode != 0:
            raise RuntimeError(p.stderr or p.stdout)
        p = run([str(out)], cwd=ROOT)
        if p.returncode != 0:
            raise RuntimeError(p.stderr or p.stdout)
        return p.stdout.strip()
    finally:
        src.unlink(missing_ok=True)
        out.unlink(missing_ok=True)


def emit_cpp(name: str) -> str:
    require_tool("c++")
    include = ROOT / "implementations" / "cpp" / "provekit-ir-symbolic" / "include"
    body = {
        "eq_atomic": r'''
            auto lhs = std::make_shared<Term>(Term{CtorTerm{"parse_int", {str_const("42")}}});
            auto rhs = num(42);
            auto f = std::make_shared<Formula>(Formula{AtomicFormula{"=", {lhs, rhs}}});
            write_formula(out, *f);
        ''',
        "pattern1_bounded_loop": r'''
            auto x1 = make_var("x");
            auto x2 = make_var("x");
            auto x3 = make_var("x");
            auto zero1 = num(0);
            auto zero2 = num(0);
            auto hundred = num(100);
            auto lower = std::make_shared<Formula>(Formula{AtomicFormula{"≥", {x1, zero1}}});
            auto upper = std::make_shared<Formula>(Formula{AtomicFormula{"<", {x2, hundred}}});
            auto ant = std::make_shared<Formula>(Formula{ConnectiveFormula{"and", {lower, upper}}});
            auto inner = std::make_shared<Formula>(Formula{AtomicFormula{"≥", {x3, zero2}}});
            auto body = std::make_shared<Formula>(Formula{ConnectiveFormula{"implies", {ant, inner}}});
            auto q = std::make_shared<Formula>(Formula{QuantifierFormula{"forall", "x", Int(), body}});
            write_formula(out, *q);
        ''',
        "contract_decl": r'''
            auto x = make_var("x");
            auto zero = num(0);
            auto pre = std::make_shared<Formula>(Formula{AtomicFormula{"≥", {x, zero}}});
            std::vector<ContractDecl> decls{ContractDecl{"parseInt", pre, nullptr, nullptr, "out", nullptr}};
            out << marshal_declarations(decls);
        ''',
        "bridge_decl_v1_1": r'''
            BridgeDecl b;
            b.name = "myBridge";
            b.source_symbol = "source";
            b.source_layer = "c-kit";
            b.source_contract_cid = "bafySource";
            b.target_contract_cid = "bafyTarget";
            b.target_proof_cid = "bafyProof";
            b.target_layer = "coq";
            b.notes = "some notes";
            write_bridge_decl(out, b);
        ''',
    }[name]
    code = f"""
        #include "provekit/ir.hpp"
        #include <iostream>
        #include <sstream>
        using namespace provekit::ir;
        int main() {{
            std::ostringstream out;
            {body}
            std::cout << out.str();
            return 0;
        }}
    """
    with tempfile.NamedTemporaryFile("w", suffix=".cpp", delete=False) as f:
        f.write(textwrap.dedent(code))
        src = Path(f.name)
    out = src.with_suffix(".out")
    try:
        p = run(["c++", "-std=c++17", "-I", str(include), str(src), "-o", str(out)], cwd=ROOT)
        if p.returncode != 0:
            raise RuntimeError(p.stderr or p.stdout)
        p = run([str(out)], cwd=ROOT)
        if p.returncode != 0:
            raise RuntimeError(p.stderr or p.stdout)
        return p.stdout.strip()
    finally:
        src.unlink(missing_ok=True)
        out.unlink(missing_ok=True)


def emit_zig(name: str) -> str:
    zig = ZIG if ZIG.exists() else Path(shutil.which("zig") or "")
    if not zig:
        raise RuntimeError("required tool not found: zig")

    src_dir = ROOT / "implementations" / "zig" / "provekit-ir" / "src"
    body = {
        "eq_atomic": r'''
            const ctor_args = [_]provekit.Term{provekit.Str("42")};
            const lhs = provekit.Ctor("parse_int", &ctor_args);
            const rhs = provekit.Num(42);
            const atomic_args = [_]provekit.Term{ lhs, rhs };
            const value = provekit.Atomic("=", &atomic_args);
            const jcs = try provekit.jcsStringify(std.heap.page_allocator, value);
        ''',
        "pattern1_bounded_loop": r'''
            const lower_args = [_]provekit.Term{ provekit.Var("x"), provekit.Num(0) };
            const lower = provekit.Atomic("≥", &lower_args);
            const upper_args = [_]provekit.Term{ provekit.Var("x"), provekit.Num(100) };
            const upper = provekit.Atomic("<", &upper_args);
            const conj_args = [_]provekit.Formula{ lower, upper };
            const ant = provekit.And(&conj_args);
            const inner_args = [_]provekit.Term{ provekit.Var("x"), provekit.Num(0) };
            const inner = provekit.Atomic("≥", &inner_args);
            const impl_args = [_]provekit.Formula{ ant, inner };
            const body = provekit.Implies(&impl_args);
            const value = provekit.Forall("x", provekit.Sort.Int, &body);
            const jcs = try provekit.jcsStringify(std.heap.page_allocator, value);
        ''',
        "contract_decl": r'''
            const pre_args = [_]provekit.Term{ provekit.Var("x"), provekit.Num(0) };
            const pre = provekit.Atomic("≥", &pre_args);
            const decl = provekit.Decl{ .contract = .{ .name = "parseInt", .out_binding = "out", .pre = pre } };
            const decls = [_]provekit.Decl{decl};
            const jcs = try provekit.jcsStringify(std.heap.page_allocator, &decls);
        ''',
        "bridge_decl_v1_1": r'''
            const value = provekit.Decl{ .bridge = .{
                .name = "myBridge",
                .source_symbol = "source",
                .source_layer = "c-kit",
                .source_contract_cid = "bafySource",
                .target_contract_cid = "bafyTarget",
                .target_proof_cid = "bafyProof",
                .target_layer = "coq",
                .notes = "some notes",
            } };
            const jcs = try provekit.jcsStringify(std.heap.page_allocator, value);
        ''',
    }[name]
    with tempfile.TemporaryDirectory(prefix="pk_zig_conformance_") as tmp:
        d = Path(tmp)
        shutil.copy2(src_dir / "root.zig", d / "root.zig")
        shutil.copy2(src_dir / "cross_kit_bridges.zig", d / "cross_kit_bridges.zig")
        (d / "main.zig").write_text(
            textwrap.dedent(
                f"""
                const std = @import("std");
                const provekit = @import("provekit-ir");
                pub fn main(init: std.process.Init) !void {{
                    {body}
                    defer std.heap.page_allocator.free(jcs);
                    var write_buf: [4096]u8 = undefined;
                    var stdout_file = std.Io.File.stdout().writerStreaming(init.io, &write_buf);
                    var stdout_writer = &stdout_file.interface;
                    try stdout_writer.print("{{s}}", .{{jcs}});
                    try stdout_writer.flush();
                }}
                """
            )
        )
        (d / "build.zig").write_text(
            textwrap.dedent(
                """
                const std = @import("std");
                pub fn build(b: *std.Build) void {
                    const target = b.standardTargetOptions(.{});
                    const optimize = b.standardOptimizeOption(.{});
                    const provekit_ir = b.createModule(.{
                        .root_source_file = b.path("root.zig"),
                        .target = target,
                        .optimize = optimize,
                    });
                    const exe_mod = b.createModule(.{
                        .root_source_file = b.path("main.zig"),
                        .target = target,
                        .optimize = optimize,
                        .imports = &.{
                            .{ .name = "provekit-ir", .module = provekit_ir },
                        },
                    });
                    const exe = b.addExecutable(.{
                        .name = "main",
                        .root_module = exe_mod,
                    });
                    b.installArtifact(exe);
                }
                """
            )
        )
        p = run([str(zig), "build", "--prefix", "."], cwd=d, timeout=120)
        if p.returncode != 0:
            raise RuntimeError(p.stderr or p.stdout)
        p = run([str(d / "bin" / "main")], cwd=d)
        if p.returncode != 0:
            raise RuntimeError(p.stderr or p.stdout)
        return p.stdout.strip()


def emit_csharp(name: str) -> str:
    require_tool("dotnet")
    csharp = ROOT / "implementations" / "csharp"
    body = {
        "eq_atomic": r'''
            var lhs = Terms.Ctor("parse_int", [Terms.StrConst("42")]);
            var rhs = Terms.Num(42);
            Console.Write(Jcs.Encode(Serialize.FormulaToValue(Predicates.Eq(lhs, rhs))));
        ''',
        "pattern1_bounded_loop": r'''
            var x = Terms.Var("x");
            var lower = Predicates.Gte(x, Terms.Num(0));
            var upper = Predicates.Lt(x, Terms.Num(100));
            var ant = Predicates.And(lower, upper);
            var inner = Predicates.Gte(x, Terms.Num(0));
            var q = new QuantifierFormula("forall", "x", Sort.Int, Predicates.Implies(ant, inner));
            Console.Write(Jcs.Encode(Serialize.FormulaToValue(q)));
        ''',
        "contract_decl": r'''
            var pre = Predicates.Gte(Terms.Var("x"), Terms.Num(0));
            var value = Value.Array(Value.Object(
                ("kind", Value.String("contract")),
                ("name", Value.String("parseInt")),
                ("outBinding", Value.String("out")),
                ("pre", Serialize.FormulaToValue(pre))
            ));
            Console.Write(Jcs.Encode(value));
        ''',
        "bridge_decl_v1_1": r'''
            var bridge = new BridgeDeclaration("myBridge", "source", "c-kit", "bafySource", "bafyTarget", "bafyProof", "coq", "some notes");
            Console.Write(Jcs.Encode(Serialize.BridgeDeclarationToValue(bridge)));
        ''',
    }[name]
    with tempfile.TemporaryDirectory(prefix="pk_cs_conformance_") as tmp:
        d = Path(tmp)
        (d / "pk_cs_conformance.csproj").write_text(
            textwrap.dedent(
                f"""
                <Project Sdk="Microsoft.NET.Sdk">
                  <PropertyGroup>
                    <OutputType>Exe</OutputType>
                    <TargetFramework>net10.0</TargetFramework>
                    <ImplicitUsings>enable</ImplicitUsings>
                    <Nullable>enable</Nullable>
                  </PropertyGroup>
                  <ItemGroup>
                    <ProjectReference Include="{csharp / 'Provekit.IR' / 'Provekit.IR.csproj'}" />
                    <ProjectReference Include="{csharp / 'Provekit.Canonicalizer' / 'Provekit.Canonicalizer.csproj'}" />
                  </ItemGroup>
                </Project>
                """
            )
        )
        (d / "Program.cs").write_text(
            textwrap.dedent(
                f"""
                using Provekit.Canonicalizer;
                using Provekit.IR;

                {body}
                """
            )
        )
        p = run(["dotnet", "run", "--project", "pk_cs_conformance.csproj"], cwd=d, timeout=120)
        if p.returncode != 0:
            raise RuntimeError(p.stderr or p.stdout)
        return p.stdout.strip()


def emit_ruby(name: str) -> str:
    require_tool("ruby")
    code = r'''
        require "provekit"

        case ARGV.fetch(0)
        when "eq_atomic"
          lhs = Provekit::IR.ctor("parse_int", Provekit::IR.str("42"))
          rhs = Provekit::IR.num(42)
          print Provekit::IR::Jcs.encode(Provekit::IR.eq(lhs, rhs))
        when "pattern1_bounded_loop"
          x = Provekit::IR.var(name: "x")
          body = Provekit::IR.implies(
            Provekit::IR.and(
              Provekit::IR.gte(x, Provekit::IR.num(0)),
              Provekit::IR.lt(x, Provekit::IR.num(100)),
            ),
            Provekit::IR.gte(x, Provekit::IR.num(0)),
          )
          q = Provekit::IR.forall(name: "x", sort: Provekit::IR::PrimitiveSort.Int, body: body)
          print Provekit::IR::Jcs.encode(q)
        when "contract_decl"
          pre = Provekit::IR.gte(Provekit::IR.var(name: "x"), Provekit::IR.num(0))
          d = Provekit::IR::ContractDecl.new(name: "parseInt", pre: pre)
          print Provekit::IR.marshal_declarations([d])
        when "bridge_decl_v1_1"
          d = Provekit::IR::Bridge.new(
            name: "myBridge",
            source_symbol: "source",
            source_layer: "c-kit",
            source_contract_cid: "bafySource",
            target_contract_cid: "bafyTarget",
            target_proof_cid: "bafyProof",
            target_layer: "coq",
            notes: "some notes",
          )
          print Provekit::IR.marshal_declarations([d])[1...-1]
        else
          abort "unknown fixture"
        end
    '''
    p = run(["ruby", "-Ilib", "-e", textwrap.dedent(code), name], cwd=ROOT / "implementations" / "ruby", timeout=60)
    if p.returncode != 0:
        raise RuntimeError(p.stderr or p.stdout)
    return p.stdout.strip()


def emit_php(name: str) -> str:
    require_tool("php")
    php = ROOT / "implementations" / "php"
    code = r'''
        require "provekit-ir-symbolic/src/Canonicalizer/Jcs.php";
        require "provekit-ir-symbolic/src/Ir/Term.php";
        require "provekit-ir-symbolic/src/Ir/Formula.php";
        require "provekit-ir-symbolic/src/Ir/Declaration.php";

        $name = $argv[1] ?? "";
        switch ($name) {
        case "eq_atomic":
            echo \ProvekIt\Canonicalizer\Jcs::encode(
                \ProvekIt\Ir\Eq(
                    \ProvekIt\Ir\Ctor("parse_int", \ProvekIt\Ir\Str("42")),
                    \ProvekIt\Ir\Num(42)
                )
            );
            break;
        case "pattern1_bounded_loop":
            $x = \ProvekIt\Ir\V("x");
            $body = \ProvekIt\Ir\Implies(
                \ProvekIt\Ir\And_(
                    \ProvekIt\Ir\Gte($x, \ProvekIt\Ir\Num(0)),
                    \ProvekIt\Ir\Lt($x, \ProvekIt\Ir\Num(100))
                ),
                \ProvekIt\Ir\Gte($x, \ProvekIt\Ir\Num(0))
            );
            echo \ProvekIt\Canonicalizer\Jcs::encode(\ProvekIt\Ir\ForAll("x", \ProvekIt\Ir\Sort::Int(), $body));
            break;
        case "contract_decl":
            $pre = \ProvekIt\Ir\Gte(\ProvekIt\Ir\V("x"), \ProvekIt\Ir\Num(0));
            echo \ProvekIt\Canonicalizer\Jcs::encode([new \ProvekIt\Ir\ContractDecl("parseInt", "out", $pre)]);
            break;
        case "bridge_decl_v1_1":
            echo \ProvekIt\Canonicalizer\Jcs::encode(new \ProvekIt\Ir\BridgeDecl(
                "myBridge",
                "source",
                "c-kit",
                "bafySource",
                "bafyTarget",
                "bafyProof",
                "coq",
                "some notes"
            ));
            break;
        default:
            fwrite(STDERR, "unknown fixture\n");
            exit(1);
        }
    '''
    p = run(["php", "-r", textwrap.dedent(code), name], cwd=php, timeout=60)
    if p.returncode != 0:
        raise RuntimeError(p.stderr or p.stdout)
    return p.stdout.strip()


@lru_cache(maxsize=1)
def java_ir_classpath() -> str:
    require_tool("mvn")
    require_tool("javac")
    p = run(
        ["mvn", "-q", "-f", "implementations/java/pom.xml", "-pl", "provekit-ir", "-am", "package", "-DskipTests"],
        cwd=ROOT,
        timeout=180,
    )
    if p.returncode != 0:
        raise RuntimeError(p.stderr or p.stdout)
    return str(ROOT / "implementations" / "java" / "provekit-ir" / "target" / "classes")


def emit_java(name: str) -> str:
    cp = java_ir_classpath()
    code = f"""
        import com.provekit.ir.*;

        public class PkJavaConformance {{
          public static void main(String[] args) {{
            switch ({json.dumps(name)}) {{
              case "eq_atomic" -> {{
                Term lhs = Term.ctor("parse_int", new Term[]{{ Term.const_("42", Sort.String) }}, Sort.Int);
                Term rhs = Term.const_(42, Sort.Int);
                System.out.print(Formula.atomic("=", lhs, rhs).toJson());
              }}
              case "pattern1_bounded_loop" -> {{
                Term x = Term.var_("x", Sort.Int);
                Formula lower = Formula.atomic("≥", x, Term.const_(0, Sort.Int));
                Formula upper = Formula.atomic("<", x, Term.const_(100, Sort.Int));
                Formula ant = Formula.and(lower, upper);
                Formula inner = Formula.atomic("≥", x, Term.const_(0, Sort.Int));
                System.out.print(Formula.forall("x", Sort.Int, Formula.implies(ant, inner)).toJson());
              }}
              case "contract_decl" -> {{
                Term x = Term.var_("x", Sort.Int);
                Formula pre = Formula.atomic("≥", x, Term.const_(0, Sort.Int));
                Declaration.Contract d = new Declaration.Contract("parseInt", "out", pre, null, null, null);
                System.out.print("[" + d.toJson() + "]");
              }}
              case "bridge_decl_v1_1" -> {{
                Declaration.Bridge b = new Declaration.Bridge(
                  "myBridge", "source", "c-kit", "bafySource", "bafyTarget",
                  "bafyProof", "coq", "some notes");
                System.out.print(b.toJson());
              }}
              default -> throw new IllegalArgumentException("unknown fixture");
            }}
          }}
        }}
    """
    with tempfile.TemporaryDirectory(prefix="pk_java_conformance_") as tmp:
        d = Path(tmp)
        src = d / "PkJavaConformance.java"
        src.write_text(textwrap.dedent(code))
        p = run(["javac", "-cp", cp, str(src)], cwd=d, timeout=60)
        if p.returncode != 0:
            raise RuntimeError(p.stderr or p.stdout)
        p = run(["java", "-cp", f"{cp}{os.pathsep}{d}", "PkJavaConformance"], cwd=d, timeout=60)
        if p.returncode != 0:
            raise RuntimeError(p.stderr or p.stdout)
        return p.stdout.strip()


def emit_swift(name: str) -> str:
    require_tool("swift")
    p = run(["swift", "run", "conformance", "--fixture", name], cwd=ROOT / "implementations" / "swift", timeout=180)
    if p.returncode != 0:
        raise RuntimeError(p.stderr or p.stdout)
    if not p.stdout.strip():
        raise RuntimeError(f"swift adapter emitted no bytes for fixture {name}")
    return p.stdout.strip()


@dataclass(frozen=True)
class DirectAdapter:
    kit: str
    emit: Callable[[str], str]
    fixtures: tuple[str, ...]


DIRECT_ADAPTERS = (
    DirectAdapter("rust", emit_rust, RUST_CORE_FIXTURES),
    DirectAdapter("python", emit_python, CORE_FIXTURES),
    DirectAdapter("go", emit_go, CORE_FIXTURES),
    DirectAdapter("c", emit_c, CORE_FIXTURES),
    DirectAdapter("cpp", emit_cpp, CORE_FIXTURES),
    DirectAdapter("zig", emit_zig, CORE_FIXTURES),
    DirectAdapter("csharp", emit_csharp, CORE_FIXTURES),
    DirectAdapter("ruby", emit_ruby, CORE_FIXTURES),
    DirectAdapter("php", emit_php, CORE_FIXTURES),
    DirectAdapter("java", emit_java, CORE_FIXTURES),
)

SWIFT_ADAPTERS = (
    DirectAdapter("swift", emit_swift, CORE_FIXTURES),
)


@dataclass(frozen=True)
class NativeCheck:
    kit: str
    name: str
    cmd: list[str]
    cwd: Path
    timeout: int = 300


LINUX_NATIVE_CHECKS = (
    NativeCheck(
        "rust",
        "rust bridge_v1_4 fixture",
        [
            "cargo",
            "test",
            "--release",
            "--manifest-path",
            "implementations/rust/Cargo.toml",
            "-p",
            "provekit-claim-envelope",
            "--test",
            "bridge_v14_roundtrip",
        ],
        ROOT,
        300,
    ),
    NativeCheck(
        "typescript",
        "typescript fixtures.toml golden tests",
        ["pnpm", "exec", "vitest", "run", "implementations/typescript/src/canonicalizer/cross-impl-golden.test.ts"],
        ROOT,
        180,
    ),
    NativeCheck(
        "ruby",
        "ruby bridge_v1_4 fixture",
        ["ruby", "-Ilib", "-Itest", "test/test_bridge_v14.rb"],
        ROOT / "implementations" / "ruby",
        120,
    ),
    NativeCheck(
        "java",
        "java bridge_v1_4 fixture",
        ["mvn", "test", "-q", "-f", "implementations/java/provekit-claim-envelope/pom.xml", "-Dtest=BridgeV14RoundtripTest"],
        ROOT,
        180,
    ),
    NativeCheck(
        "csharp",
        "csharp bridge_v1_4 fixture",
        [
            "dotnet",
            "test",
            "implementations/csharp/Provekit.Tests/Provekit.Tests.csproj",
            "--filter",
            "BridgeV14",
            "--nologo",
            "--verbosity",
            "quiet",
        ],
        ROOT,
        180,
    ),
)

SWIFT_NATIVE_CHECKS = (
    NativeCheck(
        "swift",
        "swift conformance runner",
        ["swift", "run", "conformance"],
        ROOT / "implementations" / "swift",
        300,
    ),
)


def run_direct_adapters(
    adapters: tuple[DirectAdapter, ...],
    fixtures: dict[str, Fixture],
) -> int:
    failures = 0
    for adapter in adapters:
        print(f"\n{C.CYAN}[{adapter.kit}] direct fixture adapter{C.RST}")
        for fixture_name in adapter.fixtures:
            fixture = fixtures[fixture_name]
            try:
                got = adapter.emit(fixture_name)
            except Exception as e:
                failures += 1
                print(f"  {C.RED}FAIL{C.RST} {fixture_name}: {e}")
                continue
            if got == fixture.jcs:
                print(f"  {C.GREEN}PASS{C.RST} {fixture_name} ({fixture.capability})")
            else:
                failures += 1
                print(f"  {C.RED}FAIL{C.RST} {fixture_name}: byte mismatch")
                print(show_diff(got, fixture.jcs))
    return failures


def run_native_checks(checks: tuple[NativeCheck, ...]) -> int:
    failures = 0
    for check in checks:
        print(f"\n{C.CYAN}[native] {check.name}{C.RST}")
        p = run(check.cmd, cwd=check.cwd, timeout=check.timeout)
        if p.returncode == 0:
            print(f"  {C.GREEN}PASS{C.RST} {' '.join(check.cmd)}")
            continue
        failures += 1
        print(f"  {C.RED}FAIL{C.RST} {' '.join(check.cmd)}")
        output = (p.stderr or "") + ("\n" + p.stdout if p.stdout else "")
        print(output.strip()[-4000:])
    return failures


def assert_profile_inventory(
    profile: str,
    direct: tuple[DirectAdapter, ...],
    native: tuple[NativeCheck, ...],
) -> None:
    required = set(PROFILE_REQUIRED_KITS[profile])
    covered = {a.kit for a in direct} | {c.kit for c in native}
    missing = sorted(required - covered)
    extra = sorted(covered - required)
    if missing:
        raise RuntimeError(f"{profile} profile leaves kit(s) uncovered: {', '.join(missing)}")
    if extra:
        raise RuntimeError(f"{profile} profile covers unexpected kit(s): {', '.join(extra)}")
    print(f"  kits: {', '.join(PROFILE_REQUIRED_KITS[profile])}")


def profile_default() -> str:
    return "all" if platform.system() == "Darwin" else "linux"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--profile",
        choices=("linux", "swift", "all"),
        default=profile_default(),
        help="host/toolchain profile to enforce; missing tools in the selected profile fail",
    )
    args = parser.parse_args()

    metadata, fixtures = load_fixtures()

    print(f"\n{C.BOLD}Catalog-pinned Cross-Kit Conformance{C.RST}")
    assert_catalog_pin(metadata)
    print(f"  catalog: {metadata['catalog_version']} {metadata['catalog_cid']}")

    missing = [name for name in CORE_FIXTURES + ("bridge_decl_v1_4",) if name not in fixtures]
    if missing:
        raise RuntimeError(f"missing conformance fixtures: {', '.join(missing)}")

    selected_direct: tuple[DirectAdapter, ...] = ()
    selected_native: tuple[NativeCheck, ...] = ()
    if args.profile in ("linux", "all"):
        selected_direct += DIRECT_ADAPTERS
        selected_native += LINUX_NATIVE_CHECKS
    if args.profile in ("swift", "all"):
        selected_direct += SWIFT_ADAPTERS
        selected_native += SWIFT_NATIVE_CHECKS

    assert_profile_inventory(args.profile, selected_direct, selected_native)

    failures = 0
    if args.profile in ("linux", "all"):
        failures += run_direct_adapters(DIRECT_ADAPTERS, fixtures)
        failures += run_native_checks(LINUX_NATIVE_CHECKS)
    if args.profile in ("swift", "all"):
        failures += run_direct_adapters(SWIFT_ADAPTERS, fixtures)
        failures += run_native_checks(SWIFT_NATIVE_CHECKS)

    print(f"\n{C.BOLD}Result{C.RST}")
    if failures:
        print(f"  {C.RED}{failures} conformance failure(s){C.RST}")
        return 1
    print(f"  {C.GREEN}all selected conformance checks passed{C.RST}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as e:
        print(f"{C.RED}fatal:{C.RST} {e}", file=sys.stderr)
        raise SystemExit(1)
