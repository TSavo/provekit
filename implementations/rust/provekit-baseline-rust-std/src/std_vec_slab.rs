// SPDX-License-Identifier: Apache-2.0
//
// std::vec slab: builtins on `Vec<T>`.
//
// Coverage (10 builtins): Vec_new, Vec_with_capacity, Vec_len,
// Vec_is_empty, Vec_push, Vec_pop, Vec_clear, Vec_iter, Vec_as_slice,
// Vec_capacity.

use std::rc::Rc;

use provekit_ir_symbolic::{
    contract, eq, forall, gte, must, num, str_const, ContractArgs, String_, Term,
};

fn ctor1(name: &str, arg: Rc<Term>) -> Rc<Term> {
    Rc::new(Term::Ctor {
        name: name.into(),
        args: vec![arg],
    })
}

fn ctor2(name: &str, a: Rc<Term>, b: Rc<Term>) -> Rc<Term> {
    Rc::new(Term::Ctor {
        name: name.into(),
        args: vec![a, b],
    })
}

pub fn invariants() {
    // ---------------- Vec::new -----------------
    must(
        "Vec_new__type_signature",
        forall(String_(), |_| {
            eq(ctor1("type_of", ctor1("Vec_new", num(0))), str_const("Vec"))
        }),
    );
    must(
        "Vec_new__determinism",
        forall(String_(), |_| {
            eq(ctor1("Vec_new", num(0)), ctor1("Vec_new", num(0)))
        }),
    );
    // Structural: Vec::new() yields empty.
    contract(
        "Vec_new__starts_empty",
        ContractArgs {
            post: Some(forall(String_(), |_| {
                eq(ctor1("Vec_len", ctor1("Vec_new", num(0))), num(0))
            })),
            ..Default::default()
        },
    );

    // ---------------- Vec::with_capacity -----------------
    must(
        "Vec_with_capacity__type_signature",
        forall(String_(), |_| {
            eq(
                ctor1("type_of", ctor1("Vec_with_capacity", num(0))),
                str_const("Vec"),
            )
        }),
    );
    must(
        "Vec_with_capacity__determinism",
        forall(String_(), |_| {
            eq(
                ctor1("Vec_with_capacity", num(0)),
                ctor1("Vec_with_capacity", num(0)),
            )
        }),
    );
    // Structural: with_capacity yields an empty Vec; capacity reservation
    // doesn't pre-populate the buffer.
    contract(
        "Vec_with_capacity__starts_empty",
        ContractArgs {
            post: Some(forall(String_(), |_| {
                eq(ctor1("Vec_len", ctor1("Vec_with_capacity", num(0))), num(0))
            })),
            ..Default::default()
        },
    );

    // ---------------- Vec::len -----------------
    must(
        "Vec_len__type_signature",
        forall(String_(), |v| {
            eq(ctor1("type_of", ctor1("Vec_len", v)), str_const("usize"))
        }),
    );
    must(
        "Vec_len__determinism",
        forall(String_(), |v| {
            eq(ctor1("Vec_len", v.clone()), ctor1("Vec_len", v))
        }),
    );
    must(
        "Vec_len__nonneg",
        forall(String_(), |v| gte(ctor1("Vec_len", v), num(0))),
    );

    // ---------------- Vec::is_empty -----------------
    must(
        "Vec_is_empty__type_signature",
        forall(String_(), |v| {
            eq(
                ctor1("type_of", ctor1("Vec_is_empty", v)),
                str_const("bool"),
            )
        }),
    );
    must(
        "Vec_is_empty__determinism",
        forall(String_(), |v| {
            eq(ctor1("Vec_is_empty", v.clone()), ctor1("Vec_is_empty", v))
        }),
    );
    // Structural: agrees with len-eq-zero check.
    must(
        "Vec_is_empty__agrees_with_len_zero_check",
        forall(String_(), |v| {
            eq(
                ctor1("Vec_is_empty", v.clone()),
                ctor1("len_eq_zero", ctor1("Vec_len", v)),
            )
        }),
    );

    // ---------------- Vec::push -----------------
    must(
        "Vec_push__type_signature",
        forall(String_(), |v| {
            eq(
                ctor1("type_of", ctor2("Vec_push", v, num(0))),
                str_const("()"),
            )
        }),
    );
    must(
        "Vec_push__determinism",
        forall(String_(), |v| {
            eq(
                ctor2("Vec_push", v.clone(), num(0)),
                ctor2("Vec_push", v, num(0)),
            )
        }),
    );
    // Structural: post-state length is at least 1.
    contract(
        "Vec_push__post_state_nonempty",
        ContractArgs {
            post: Some(forall(String_(), |v| {
                gte(ctor1("Vec_len", ctor2("Vec_push_post", v, num(0))), num(1))
            })),
            ..Default::default()
        },
    );

    // ---------------- Vec::pop -----------------
    must(
        "Vec_pop__type_signature",
        forall(String_(), |v| {
            eq(ctor1("type_of", ctor1("Vec_pop", v)), str_const("Option"))
        }),
    );
    must(
        "Vec_pop__determinism",
        forall(String_(), |v| {
            eq(ctor1("Vec_pop", v.clone()), ctor1("Vec_pop", v))
        }),
    );

    // ---------------- Vec::clear -----------------
    must(
        "Vec_clear__type_signature",
        forall(String_(), |v| {
            eq(ctor1("type_of", ctor1("Vec_clear", v)), str_const("()"))
        }),
    );
    must(
        "Vec_clear__determinism",
        forall(String_(), |v| {
            eq(ctor1("Vec_clear", v.clone()), ctor1("Vec_clear", v))
        }),
    );
    // Structural: post-state length is 0.
    contract(
        "Vec_clear__post_state_empty",
        ContractArgs {
            post: Some(forall(String_(), |v| {
                eq(ctor1("Vec_len", ctor1("Vec_clear_post", v)), num(0))
            })),
            ..Default::default()
        },
    );

    // ---------------- Vec::iter -----------------
    must(
        "Vec_iter__type_signature",
        forall(String_(), |v| {
            eq(ctor1("type_of", ctor1("Vec_iter", v)), str_const("Iter"))
        }),
    );
    must(
        "Vec_iter__determinism",
        forall(String_(), |v| {
            eq(ctor1("Vec_iter", v.clone()), ctor1("Vec_iter", v))
        }),
    );

    // ---------------- Vec::as_slice -----------------
    must(
        "Vec_as_slice__type_signature",
        forall(String_(), |v| {
            eq(
                ctor1("type_of", ctor1("Vec_as_slice", v)),
                str_const("&[T]"),
            )
        }),
    );
    must(
        "Vec_as_slice__determinism",
        forall(String_(), |v| {
            eq(ctor1("Vec_as_slice", v.clone()), ctor1("Vec_as_slice", v))
        }),
    );
    // Structural: the slice has the same length as the Vec.
    must(
        "Vec_as_slice__preserves_length",
        forall(String_(), |v| {
            eq(
                ctor1("slice_len", ctor1("Vec_as_slice", v.clone())),
                ctor1("Vec_len", v),
            )
        }),
    );

    // ---------------- Vec::capacity -----------------
    must(
        "Vec_capacity__type_signature",
        forall(String_(), |v| {
            eq(
                ctor1("type_of", ctor1("Vec_capacity", v)),
                str_const("usize"),
            )
        }),
    );
    must(
        "Vec_capacity__determinism",
        forall(String_(), |v| {
            eq(ctor1("Vec_capacity", v.clone()), ctor1("Vec_capacity", v))
        }),
    );
    // Structural: capacity is always >= len.
    must(
        "Vec_capacity__bounds_len_from_above",
        forall(String_(), |v| {
            gte(ctor1("Vec_capacity", v.clone()), ctor1("Vec_len", v))
        }),
    );
}
