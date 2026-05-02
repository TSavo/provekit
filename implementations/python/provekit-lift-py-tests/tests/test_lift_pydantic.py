# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import pytest

from provekit_lift_py_tests.lift.pydantic import lift_pydantic_model


# Only run these if pydantic is installed.
pydantic = pytest.importorskip("pydantic")


class TestPydanticLift:
    def test_pydantic_v2_field_constraints(self):
        from pydantic import BaseModel, Field

        class User(BaseModel):
            name: str = Field(min_length=1, max_length=100)
            age: int = Field(ge=0, le=150)
            email: str = Field(pattern=r"^[^@]+@[^@]+$")

        decls = lift_pydantic_model(User)
        names = [d.name for d in decls]
        assert "User.name" in names
        assert "User.age" in names
        assert "User.email" in names

    def test_pydantic_v2_numeric_range(self):
        from pydantic import BaseModel, Field

        class Score(BaseModel):
            value: int = Field(ge=0, le=100)

        decls = lift_pydantic_model(Score)
        assert len(decls) == 1
        decl = decls[0]
        assert decl.name == "Score.value"
        assert decl.pre is not None

    def test_pydantic_v2_annotated_types(self):
        from pydantic import BaseModel
        from typing import Annotated
        from annotated_types import Gt, Lt

        class Item(BaseModel):
            quantity: Annotated[int, Gt(0), Lt(100)]

        decls = lift_pydantic_model(Item)
        assert len(decls) == 1
        assert decls[0].name == "Item.quantity"
