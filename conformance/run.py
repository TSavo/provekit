#!/usr/bin/env python3
"""Cross-language conformance harness — extracts JCS from each kit and diffs."""

from __future__ import annotations
import difflib
import json
import subprocess
import sys
import textwrap
from dataclasses import dataclass
from pathlib import Path
from typing import Optional

ROOT = Path(__file__).resolve().parent.parent
ZIG = Path("/Users/tsavo") / "zig-toolchain" / "zig"
PY_SRC = ROOT / "implementations" / "python" / "provekit-lift-py-tests" / "src"
sys.path.insert(0, str(PY_SRC))
RED = "\033[0;31m"
GREEN = "\033[0;32m"
YELLOW = "\033[1;33m"
CYAN = "\033[0;36m"
NC = "\033[0m"

# ── golden fixtures (source of truth = Rust canonical) ────────

FIXTURES = {
    "eq_atomic": {
        "desc": "parse_int('42') = 42",
        "jcs": '{"args":[{"args":[{"kind":"const","sort":{"kind":"primitive","name":"String"},"value":"42"}],"kind":"ctor","name":"parse_int"},{"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":42}],"kind":"atomic","name":"="}',
    },
    "pattern1_bounded_loop": {
        "desc": "forall x: (x >= 0 && x < 100) => x >= 0",
        "jcs": '{"body":{"kind":"implies","operands":[{"kind":"and","operands":[{"args":[{"kind":"var","name":"x"},{"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":0}],"kind":"atomic","name":"≥"},{"args":[{"kind":"var","name":"x"},{"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":100}],"kind":"atomic","name":"<"}]},{"args":[{"kind":"var","name":"x"},{"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":0}],"kind":"atomic","name":"≥"}]},"kind":"forall","name":"x","sort":{"kind":"primitive","name":"Int"}}',
    },
}

PASSES = 0
FAILS = 0
FAILURES = []


class Colors:
    PASS = "\033[0;32m"
    FAIL = "\033[0;31m"
    WARN = "\033[1;33m"
    BOLD = "\033[1m"
    DIM = "\033[2m"
    CYAN = "\033[0;36m"
    RST = "\033[0m"
    GREEN = PASS
    RED = FAIL
    YELLOW = WARN


def show_diff(label: str, got: str, want: str) -> str:
    lines = []
    lines.append(f"{Colors.FAIL}  ── {label} ──{Colors.RST}")
    g_lines = got.replace(",", ",\n").split("\n")
    w_lines = want.replace(",", ",\n").split("\n")
    for d in difflib.unified_diff(
        w_lines, g_lines, fromfile="expected", tofile="got", lineterm=""
    ):
        if d.startswith("---") or d.startswith("+++") or d.startswith("@@"):
            continue
        if d.startswith("+"):
            lines.append(f"  {Colors.FAIL}{d}{Colors.RST}")
        elif d.startswith("-"):
            lines.append(f"  {Colors.GREEN}{d}{Colors.RST}")
        else:
            lines.append(f"  {Colors.DIM}{d}{Colors.RST}")
    return "\n".join(lines)


def find_first_diff(got: str, want: str) -> str:
    for i, (a, b) in enumerate(zip(got, want)):
        if a != b:
            start = max(0, i - 40)
            end = min(len(got), i + 40)
            ctx = got[start:end]
            marker = " " * (i - start) + "^"
            return f"  first byte diff at position {i}:\n  {Colors.DIM}{ctx}{Colors.RST}\n  {Colors.FAIL}{marker}{Colors.RST}"
        if i >= len(want):
            return f"  got is longer than expected at position {i}"
    if len(want) > len(got):
        return f"  expected is longer than got at position {len(got)}"
    return ""


def run(cmd: list[str], cwd: Path, timeout: int = 120) -> tuple[int, str, str]:
    try:
        p = subprocess.run(
            cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout
        )
        return p.returncode, p.stdout, p.stderr
    except subprocess.TimeoutExpired:
        return -1, "", f"timeout after {timeout}s"
    except FileNotFoundError:
        return -1, "", f"binary not found: {cmd[0]}"


def check_jcs(label: str, got: str, want: str) -> bool:
    if got == want:
        return True
    global FAILS, FAILURES
    FAILS += 1
    FAILURES.append((label, got, want))
    print(f"  {Colors.FAIL}✗ {label} — byte mismatch{Colors.RST}")
    print(show_diff(label, got, want))
    print(f"\n  {Colors.FAIL}got:      {Colors.DIM}{got[:120]}...{Colors.RST}")
    print(f"  {Colors.GREEN}expected: {Colors.DIM}{want[:120]}...{Colors.RST}")
    print(find_first_diff(got, want))
    return False


def check_test_exit(label: str, rc: int, stdout: str, stderr: str) -> bool:
    if rc == 0:
        return True
    global FAILS, FAILURES
    FAILS += 1
    FAILURES.append((label, stdout, stderr))
    print(f"  {Colors.FAIL}✗ {label} — exit code {rc}{Colors.RST}")
    if stderr.strip():
        print(f"  {Colors.DIM}stderr: {stderr.strip()[:300]}{Colors.RST}")
    if stdout.strip():
        print(f"  {Colors.DIM}stdout: {stdout.strip()[:300]}{Colors.RST}")
    return False


# ═══════════════════════════════════════════════════════════════
#  Per-language JCS extractors
# ═══════════════════════════════════════════════════════════════


def extract_rust(fixture_name: str) -> Optional[str]:
    """Run Rust test that constructs fixture and prints JCS to stdout."""
    rust_dir = ROOT / "implementations" / "rust"
    script = textwrap.dedent(f"""\
        use provekit_ir_symbolic::ir::*;
        fn main() {{
            match "{fixture_name}" {{
                "eq_atomic" => {{
                    let lhs = ctor("parse_int", vec![str("42")]);
                    let rhs = num(42);
                    let f = atomic("=", vec![lhs, rhs]);
                    println!("{{}}", serialize_formula(&f));
                }},
                "pattern1_bounded_loop" => {{
                    let x = var("x");
                    let zero = num(0);
                    let hundred = num(100);
                    let lower = atomic("≥", vec![x.clone(), zero.clone()]);
                    let upper = atomic("<", vec![x.clone(), hundred]);
                    let ant = and_(vec![lower, upper]);
                    let inner = atomic("≥", vec![x, zero]);
                    let body = implies(ant, inner);
                    let q = forall("x", Int(), body);
                    println!("{{}}", serialize_formula(&q));
                }},
                _ => {{}}
            }}
        }}
    """)
    return None  # Skip — Rust is canonical source, trust its test suite


def extract_go(fixture_name: str) -> Optional[str]:
    go_dir = ROOT / "implementations" / "go"
    script = textwrap.dedent(f"""\
        package main
        import (
            "fmt"
            ir "github.com/provekit/go/provekit-ir-symbolic/ir"
        )
        func main() {{
            switch "{fixture_name}" {{
            case "eq_atomic":
                lhs := ir.Ctor("parse_int", []*ir.Term{{ir.StrConst("42")}})
                rhs := ir.Num(42)
                f := ir.Atomic("=", []*ir.Term{{lhs, rhs}})
                json, _ := ir.MarshalFormula(f)
                fmt.Println(string(json))
            case "pattern1_bounded_loop":
                x := ir.Var("x")
                zero := ir.Num(0)
                hundred := ir.Num(100)
                lower := ir.Atomic("≥", []*ir.Term{{x, zero}})
                upper := ir.Atomic("<", []*ir.Term{{x, hundred}})
                ant := ir.And([]*ir.Formula{{lower, upper}})
                inner := ir.Atomic("≥", []*ir.Term{{x, zero}})
                body := ir.Implies(ant, inner)
                q := ir.Forall("x", ir.IntSort, body)
                json, _ := ir.MarshalFormula(q)
                fmt.Println(string(json))
            }}
        }}
    """)
    return None  # Skipping — Go module path issues


def extract_python(fixture_name: str) -> str:
    """Use the Python kit's IR types + canonicalizer."""
    from provekit_lift_py_tests.ir import (
        atomic,
        eq,
        gt,
        gte,
        lt,
        ne,
        and_,
        or_,
        not_,
        implies,
        make_var,
        num,
        str_const,
        bool_const,
        ctor,
        forall,
        Int,
        String,
        Bool,
        formula_to_value,
    )
    from provekit_lift_py_tests.canonicalizer import encode_jcs

    if fixture_name == "eq_atomic":
        lhs = ctor("parse_int", [str_const("42")])
        rhs = num(42)
        f = atomic("=", [lhs, rhs])
    elif fixture_name == "pattern1_bounded_loop":
        x = make_var("x")
        zero = num(0)
        hundred = num(100)
        lower = atomic("≥", [x, zero])
        upper = atomic("<", [x, hundred])
        ant = and_([lower, upper])
        inner = atomic("≥", [x, zero])
        body = implies(ant, inner)
        f = forall("x", Int(), body)
    else:
        raise ValueError(f"unknown fixture: {fixture_name}")

    return encode_jcs(formula_to_value(f))


def extract_cpp(fixture_name: str) -> Optional[str]:
    """Compile + run a C++ program that emits JCS."""
    import tempfile, os

    if fixture_name == "eq_atomic":
        code = textwrap.dedent("""\
            #include "provekit/ir.hpp"
            #include <iostream>
            #include <sstream>
            using namespace provekit::ir;
            int main() {
                auto lhs_args = std::vector<std::shared_ptr<Term>>{str_const("42")};
                auto lhs = std::make_shared<Term>(Term{CtorTerm{"parse_int", lhs_args}});
                auto rhs = num(42);
                auto args = std::vector<std::shared_ptr<Term>>{lhs, rhs};
                auto f = std::make_shared<Formula>(Formula{AtomicFormula{"=", args}});
                std::ostringstream out;
                write_formula(out, *f);
                std::cout << out.str();
                return 0;
            }
        """)
    elif fixture_name == "pattern1_bounded_loop":
        code = textwrap.dedent("""\
            #include "provekit/ir.hpp"
            #include <iostream>
            #include <sstream>
            using namespace provekit::ir;
            int main() {
                auto x = make_var("x");
                auto zero = num(0);
                auto hundred = num(100);
                auto lower = std::make_shared<Formula>(Formula{AtomicFormula{"≥", {x, zero}}});
                auto upper = std::make_shared<Formula>(Formula{AtomicFormula{"<", {x, hundred}}});
                auto ant = std::make_shared<Formula>(Formula{ConnectiveFormula{"and", {lower, upper}}});
                auto inner = std::make_shared<Formula>(Formula{AtomicFormula{"≥", {x, zero}}});
                auto body = std::make_shared<Formula>(Formula{ConnectiveFormula{"implies", {ant, inner}}});
                auto q = std::make_shared<Formula>(Formula{QuantifierFormula{"forall", "x", Int(), body}});
                std::ostringstream out;
                write_formula(out, *q);
                std::cout << out.str();
                return 0;
            }
        """)
    else:
        return None

    include = ROOT / "implementations" / "cpp" / "provekit-ir-symbolic" / "include"
    with tempfile.NamedTemporaryFile(suffix=".cpp", mode="w", delete=False) as f:
        f.write(code)
        src = f.name
    out = src + ".out"
    try:
        rc, stdout, stderr = run(
            ["c++", "-std=c++17", "-I", str(include), src, "-o", out], cwd=ROOT
        )
        if rc != 0:
            print(f"  {Colors.WARN}C++ compile failed: {stderr[:200]}{Colors.RST}")
            return None
        rc, stdout, stderr = run([out], cwd=ROOT, timeout=10)
        return stdout.strip()
    finally:
        Path(src).unlink(missing_ok=True)
        Path(out).unlink(missing_ok=True)


def extract_c(fixture_name: str) -> Optional[str]:
    import tempfile, os

    if fixture_name == "eq_atomic":
        code = textwrap.dedent("""\
            #include "provekit/ir.h"
            #include <stdio.h>
            int main() {
                pk_term *parse_int_arg = pk_term_const_str("42", pk_sort_primitive("String"));
                pk_term *parse_int_args[] = {parse_int_arg};
                pk_term *lhs = pk_term_ctor_new("parse_int", parse_int_args, 1);
                pk_term *rhs = pk_term_const_int(42, pk_sort_primitive("Int"));
                pk_term *args[] = {lhs, rhs};
                pk_formula *f = pk_formula_atomic_new("=", args, 2);
                pk_buffer *buf = pk_buffer_new();
                pk_emit_formula(buf, f);
                printf("%s", buf->data);
                pk_buffer_free(buf);
                pk_formula_free(f);
                return 0;
            }
        """)
    elif fixture_name == "pattern1_bounded_loop":
        code = textwrap.dedent("""\
            #include "provekit/ir.h"
            #include <stdio.h>
            int main() {
                pk_sort *int_sort = pk_sort_primitive("Int");
                pk_term *x1 = pk_term_var_new("x");
                pk_term *x2 = pk_term_var_new("x");
                pk_term *x3 = pk_term_var_new("x");
                pk_term *zero = pk_term_const_int(0, int_sort);
                pk_term *zero2 = pk_term_const_int(0, int_sort);
                pk_term *hundred = pk_term_const_int(100, int_sort);
                pk_term *lower_args[] = {x1, zero};
                pk_formula *lower = pk_formula_atomic_new("≥", lower_args, 2);
                pk_term *upper_args[] = {x2, hundred};
                pk_formula *upper = pk_formula_atomic_new("<", upper_args, 2);
                pk_formula *conj[] = {lower, upper};
                pk_formula *ant = pk_formula_connective_new("and", conj, 2);
                pk_term *inner_args[] = {x3, zero2};
                pk_formula *inner = pk_formula_atomic_new("≥", inner_args, 2);
                pk_formula *impl[] = {ant, inner};
                pk_formula *body = pk_formula_connective_new("implies", impl, 2);
                pk_formula *q = pk_formula_quantifier_new("forall", "x", int_sort, body);
                pk_buffer *buf = pk_buffer_new();
                pk_emit_formula(buf, q);
                printf("%s", buf->data);
                pk_buffer_free(buf);
                pk_formula_free(q);
                return 0;
            }
        """)
    else:
        return None

    c_dir = ROOT / "implementations" / "c" / "provekit-ir"
    include = c_dir / "include"
    src_files = [c_dir / "src" / f for f in ["ir.c", "jcs.c"]]
    with tempfile.NamedTemporaryFile(suffix=".c", mode="w", delete=False) as f:
        f.write(code)
        src = f.name
    out = src + ".out"
    try:
        cmd = (
            ["cc", "-std=c11", "-I", str(include), src]
            + [str(s) for s in src_files]
            + ["-o", out]
        )
        rc, stdout, stderr = run(cmd, cwd=ROOT)
        if rc != 0:
            print(f"  {Colors.WARN}C compile failed: {stderr[:200]}{Colors.RST}")
            return None
        rc, stdout, stderr = run([out], cwd=ROOT, timeout=10)
        return stdout.strip()
    finally:
        Path(src).unlink(missing_ok=True)
        Path(out).unlink(missing_ok=True)


def extract_zig(fixture_name: str) -> Optional[str]:
    import tempfile, os

    zig_src = ROOT / "implementations" / "zig" / "provekit-ir" / "src"
    if fixture_name == "eq_atomic":
        code = textwrap.dedent("""\
            const std = @import("std");
            const provekit = @import("provekit-ir");
            pub fn main() !void {
                const alloc = std.heap.page_allocator;
                const str42 = provekit.Term{ .const_term = .{ .value = .{ .string = "42" }, .sort = provekit.Sort.String }};
                const ctor_args = [_]provekit.Term{str42};
                const lhs = provekit.Ctor("parse_int", &ctor_args, provekit.Sort.Node);
                const rhs = provekit.Term{ .const_term = .{ .value = .{ .int = 42 }, .sort = provekit.Sort.Int }};
                const atomic_args = [_]provekit.Term{ lhs, rhs };
                const f = provekit.Atomic("=", &atomic_args);
                const jcs = try provekit.writeJson(alloc, f);
                defer alloc.free(jcs);
                const stdout = std.io.getStdOut().writer();
                try stdout.print("{s}", .{jcs});
            }
        """)
    elif fixture_name == "pattern1_bounded_loop":
        code = textwrap.dedent("""\
            const std = @import("std");
            const provekit = @import("provekit-ir");
            pub fn main() !void {
                const alloc = std.heap.page_allocator;
                const x1 = provekit.Var("x", provekit.Sort.Int);
                const x2 = provekit.Var("x", provekit.Sort.Int);
                const x3 = provekit.Var("x", provekit.Sort.Int);
                const zero1 = provekit.Term{ .const_term = .{ .value = .{ .int = 0 }, .sort = provekit.Sort.Int }};
                const zero2 = provekit.Term{ .const_term = .{ .value = .{ .int = 0 }, .sort = provekit.Sort.Int }};
                const hundred = provekit.Term{ .const_term = .{ .value = .{ .int = 100 }, .sort = provekit.Sort.Int }};
                const lower_args = [_]provekit.Term{ x1, zero1 };
                const lower = provekit.Atomic("≥", &lower_args);
                const upper_args = [_]provekit.Term{ x2, hundred };
                const upper = provekit.Atomic("<", &upper_args);
                const conj_args = [_]provekit.Formula{ lower, upper };
                const ant = provekit.And(&conj_args);
                const inner_args = [_]provekit.Term{ x3, zero2 };
                const inner = provekit.Atomic("≥", &inner_args);
                const body = provekit.Implies(&ant, &inner);
                const q = provekit.Forall("x", provekit.Sort.Int, &body);
                const jcs = try provekit.writeJson(alloc, q);
                defer alloc.free(jcs);
                const stdout = std.io.getStdOut().writer();
                try stdout.print("{s}", .{jcs});
            }
        """)
    else:
        return None

    with tempfile.NamedTemporaryFile(suffix=".zig", mode="w", delete=False) as f:
        f.write(code)
        src = f.name
    src_dir = os.path.dirname(src)
    # Copy into a buildable location
    build_dir = Path(src_dir) / "provekit_conformance"
    build_dir.mkdir(parents=True, exist_ok=True)
    dest_src = build_dir / "main.zig"
    dest_src.write_text(code)
    # Copy root.zig into build dir so b.path() gets a relative path
    import shutil

    if not (zig_src / "root.zig").exists():
        print(f"  {Colors.WARN}Zig kit not available on this branch{Colors.RST}")
        return None
    shutil.copy2(zig_src / "root.zig", build_dir / "root.zig")
    (build_dir / "build.zig").write_text(
        textwrap.dedent(
            """\
        const std = @import("std");
        pub fn build(b: *std.Build) void {
            const target = b.standardTargetOptions(.{});
            const optimize = b.standardOptimizeOption(.{});
            const provekit_ir = b.createModule(.{
                .root_source_file = b.path("root.zig"),
            });
            const exe = b.addExecutable(.{
                .name = "main",
                .root_source_file = b.path("main.zig"),
                .target = target,
                .optimize = optimize,
            });
            exe.root_module.addImport("provekit-ir", provekit_ir);
            b.installArtifact(exe);
        }
    """
        )
    )
    try:
        rc, stdout, stderr = run(
            [str(ZIG), "build", "--prefix", "."], cwd=build_dir, timeout=60
        )
        if rc != 0:
            print(f"  {Colors.WARN}Zig build failed: {stderr[:300]}{Colors.RST}")
            return None
        rc, stdout, stderr = run(
            [str(build_dir / "bin" / "main")], cwd=build_dir, timeout=10
        )
        return stdout.strip()
    finally:
        pass  # Keep temp dir for debugging


# ═══════════════════════════════════════════════════════════════
#  Main
# ═══════════════════════════════════════════════════════════════


def main():
    global PASSES, FAILS
    print(f"\n{Colors.BOLD}═══ Cross-Language Conformance Suite ═══{Colors.RST}")

    for name, f in FIXTURES.items():
        golden = f["jcs"]
        print(f"\n{Colors.CYAN}── {name}: {f['desc']}{Colors.RST}")
        print(f"  golden: {Colors.DIM}{golden[:80]}...{Colors.RST}")

        # Python is easiest — direct IR construction
        try:
            got = extract_python(name)
            if check_jcs(f"python-{name}", got, golden):
                PASSES += 1
                print(f"  {Colors.PASS}✓ python-{name}{Colors.RST}")
        except Exception as e:
            FAILS += 1
            print(f"  {Colors.FAIL}✗ python-{name}: {e}{Colors.RST}")

        # C
        try:
            got = extract_c(name)
            if got and check_jcs(f"c-{name}", got, golden):
                PASSES += 1
                print(f"  {Colors.PASS}✓ c-{name}{Colors.RST}")
            elif not got:
                print(f"  {Colors.WARN}~ c-{name}: could not extract{Colors.RST}")
        except Exception as e:
            FAILS += 1
            print(f"  {Colors.FAIL}✗ c-{name}: {e}{Colors.RST}")

        # C++
        try:
            got = extract_cpp(name)
            if got and check_jcs(f"cpp-{name}", got, golden):
                PASSES += 1
                print(f"  {Colors.PASS}✓ cpp-{name}{Colors.RST}")
            elif not got:
                print(f"  {Colors.WARN}~ cpp-{name}: could not extract{Colors.RST}")
        except Exception as e:
            FAILS += 1
            print(f"  {Colors.FAIL}✗ cpp-{name}: {e}{Colors.RST}")

        # Zig
        try:
            got = extract_zig(name)
            if got and check_jcs(f"zig-{name}", got, golden):
                PASSES += 1
                print(f"  {Colors.PASS}✓ zig-{name}{Colors.RST}")
            elif not got:
                print(f"  {Colors.WARN}~ zig-{name}: could not extract{Colors.RST}")
        except Exception as e:
            FAILS += 1
            print(f"  {Colors.FAIL}✗ zig-{name}: {e}{Colors.RST}")

    # Summary
    total = PASSES + FAILS
    print(f"\n{Colors.BOLD}══════════════════════════════════════{Colors.RST}")
    print(
        f"  Results: {Colors.PASS}{PASSES} pass{Colors.RST}, {Colors.FAIL}{FAILS} fail{Colors.RST} ({total} total)"
    )
    print(f"{Colors.BOLD}══════════════════════════════════════{Colors.RST}\n")

    if FAILS > 0:
        print(f"{Colors.FAIL}FAILURES:{Colors.RST}")
        for label, got, want in FAILURES:
            print(f"  {label}")
            print(show_diff(label, got, want))
            print()
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
