# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import pytest

from provekit_lift_py_tests.decorators import (
    contract,
    ContractViolation,
    collect_module,
)
from provekit_lift_py_tests.ir import (
    atomic,
    eq,
    gt,
    gte,
    make_var,
    num,
    str_const,
    bool_const,
)


class TestContractDecorator:
    def test_precondition_string_expr(self):
        @contract(pre="x >= 0")
        def abs_(x: int) -> int:
            return x if x >= 0 else -x

        decl = abs_._provekit_contract
        assert (
            decl.name
            == "TestContractDecorator.test_precondition_string_expr.<locals>.abs_"
        )
        assert decl.pre is not None

    def test_postcondition_string_expr(self):
        @contract(pre="x >= 0", post="out >= 0")
        def sqrt(x: float) -> float:
            return x**0.5

        decl = sqrt._provekit_contract
        assert decl.pre is not None
        assert decl.post is not None
        assert decl.out_binding == "out"

    def test_runtime_precondition_passes(self):
        @contract(pre=lambda x: x >= 0)
        def abs_(x: int) -> int:
            return x if x >= 0 else -x

        assert abs_(5) == 5

    def test_runtime_precondition_fails(self):
        @contract(pre=lambda x: x >= 0)
        def abs_(x: int) -> int:
            return x if x >= 0 else -x

        with pytest.raises(ContractViolation):
            abs_(-1)

    def test_collect_module(self):
        # Build a temporary module-like namespace with decorated functions.
        import types

        mod = types.ModuleType("test_mod")

        @contract(pre="x >= 0")
        def func_a(x: int) -> int:
            return x

        @contract(pre="y > 0", post="out > 0")
        def func_b(y: int) -> int:
            return y

        mod.func_a = func_a
        mod.func_b = func_b

        decls = collect_module(mod)
        names = [d.name for d in decls]
        assert any("func_a" in n for n in names)
        assert any("func_b" in n for n in names)
