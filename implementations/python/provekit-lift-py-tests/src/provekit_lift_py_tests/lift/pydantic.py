# SPDX-License-Identifier: Apache-2.0
#
# provekit.lift.pydantic: lift Pydantic BaseModel field constraints to
# canonical IR.
#
# Cross-domain equivalence guarantee: a Pydantic model constraint that
# expresses the same runtime check as another annotation family MUST
# produce byte-for-byte identical IR. For example:
#
#   class User(BaseModel):
#       name: Annotated[str, Field(min_length=1)]
#
# must produce the same IR as Bean Validation @NotEmpty or JML
# //@ requires name != null && strlen(name) > 0.

from __future__ import annotations

import inspect
import sys
from dataclasses import dataclass
from typing import Any, Callable, Dict, List, Optional, Type, Union, get_type_hints

from ..ir import (
    ContractDecl,
    Formula,
    Term,
    Int,
    String,
    Bool,
    atomic,
    eq,
    ne,
    gt,
    gte,
    lt,
    lte,
    make_var,
    num,
    str_const,
    bool_const,
    ctor,
    and_,
    or_,
)


# ---------------------------------------------------------------------------
# Pydantic lift adapter
# ---------------------------------------------------------------------------


def lift_pydantic_model(model_cls: Type) -> List[ContractDecl]:
    """Lift all field constraints from a Pydantic BaseModel into IR contracts.

    Returns one ContractDecl per field that has recognizable constraints.
    The contract name is ``<ModelName>.<field_name>``.
    """
    decls: List[ContractDecl] = []
    model_name = model_cls.__name__

    # Pydantic v2 uses model_fields; v1 uses __fields__.
    fields = _get_fields(model_cls)
    if fields is None:
        return decls

    for field_name, field_info in fields.items():
        pres = _lift_field_constraints(field_name, field_info)
        if pres:
            # Build a single contract with all preconditions ANDed.
            folded = _fold_and(pres)
            decls.append(
                ContractDecl(
                    name=f"{model_name}.{field_name}",
                    pre=folded,
                )
            )

    return decls


def _get_fields(model_cls: Type) -> Optional[Dict[str, Any]]:
    """Return a dict of field_name -> field_info, or None if not a model."""
    # Pydantic v2
    if hasattr(model_cls, "model_fields"):
        return dict(model_cls.model_fields)
    # Pydantic v1
    if hasattr(model_cls, "__fields__"):
        return dict(model_cls.__fields__)
    return None


def _lift_field_constraints(field_name: str, field_info: Any) -> List[Formula]:
    """Extract constraints from a Pydantic field info object."""
    pres: List[Formula] = []
    var = make_var(field_name)

    # Pydantic v2: metadata list + field info attributes.
    metadata = getattr(field_info, "metadata", None) or []
    for item in metadata:
        f = _lift_metadata_item(var, item)
        if f is not None:
            pres.append(f)

    # Direct attributes on FieldInfo (v2) or Field (v1).
    ge = getattr(field_info, "ge", None)
    if ge is not None:
        pres.append(gte(var, num(ge)))
    le = getattr(field_info, "le", None)
    if le is not None:
        pres.append(lte(var, num(le)))
    gt_ = getattr(field_info, "gt", None)
    if gt_ is not None:
        pres.append(gt(var, num(gt_)))
    lt_ = getattr(field_info, "lt", None)
    if lt_ is not None:
        pres.append(lt(var, num(lt_)))
    min_length = getattr(field_info, "min_length", None)
    if min_length is not None:
        pres.append(gte(ctor("strlen", [var]), num(min_length)))
    max_length = getattr(field_info, "max_length", None)
    if max_length is not None:
        pres.append(lte(ctor("strlen", [var]), num(max_length)))
    pattern = getattr(field_info, "pattern", None)
    if pattern is not None:
        pres.append(atomic("matches", [var, str_const(str(pattern))]))

    # Annotated types with constraint objects.
    for item in metadata:
        if hasattr(item, "min_length") and item.min_length is not None:
            pres.append(gte(ctor("strlen", [var]), num(item.min_length)))
        if hasattr(item, "max_length") and item.max_length is not None:
            pres.append(lte(ctor("strlen", [var]), num(item.max_length)))
        if hasattr(item, "ge") and item.ge is not None:
            pres.append(gte(var, num(item.ge)))
        if hasattr(item, "le") and item.le is not None:
            pres.append(lte(var, num(item.le)))
        if hasattr(item, "gt") and item.gt is not None:
            pres.append(gt(var, num(item.gt)))
        if hasattr(item, "lt") and item.lt is not None:
            pres.append(lt(var, num(item.lt)))
        if hasattr(item, "pattern") and item.pattern is not None:
            pres.append(atomic("matches", [var, str_const(str(item.pattern))]))

    return pres


def _lift_metadata_item(var: Term, item: Any) -> Optional[Formula]:
    """Lift a single Pydantic metadata constraint object."""
    # Pydantic v2 uses Annotated metadata objects like:
    #   annotated_types.Gt, annotated_types.Ge, annotated_types.Len, etc.
    cls_name = type(item).__name__
    if cls_name in ("Gt", "GreaterThan"):
        return gt(var, num(getattr(item, "gt", getattr(item, "value", 0))))
    if cls_name in ("Ge", "GreaterThanOrEqual"):
        return gte(var, num(getattr(item, "ge", getattr(item, "value", 0))))
    if cls_name in ("Lt", "LessThan"):
        return lt(var, num(getattr(item, "lt", getattr(item, "value", 0))))
    if cls_name in ("Le", "LessThanOrEqual"):
        return lte(var, num(getattr(item, "le", getattr(item, "value", 0))))
    if cls_name in ("Len", "Length"):
        min_l = getattr(item, "min_length", None)
        max_l = getattr(item, "max_length", None)
        parts: List[Formula] = []
        if min_l is not None:
            parts.append(gte(ctor("strlen", [var]), num(min_l)))
        if max_l is not None:
            parts.append(lte(ctor("strlen", [var]), num(max_l)))
        return _fold_and(parts) if parts else None
    if cls_name in ("Pattern", "Regex"):
        return atomic(
            "matches",
            [var, str_const(str(getattr(item, "pattern", getattr(item, "value", ""))))],
        )
    # Skip unrecognized metadata.
    return None


def _fold_and(parts: List[Formula]) -> Formula:
    if len(parts) == 1:
        return parts[0]
    return and_(parts)
