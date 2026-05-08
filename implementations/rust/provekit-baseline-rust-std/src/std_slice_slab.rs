// SPDX-License-Identifier: Apache-2.0
//
// std::slice slab — builtins on `[T]`.
//
// Coverage (8 builtins): slice_len, slice_is_empty, slice_iter,
// slice_get, slice_first, slice_last, slice_contains, slice_to_vec.

use std::rc::Rc;

use provekit_ir_symbolic::{eq, forall, gte, must, num, str_const, String_, Term};

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
    // ---------------- slice::len -----------------
    must(
        "slice_len__type_signature",
        forall(String_(), |s| {
            eq(ctor1("type_of", ctor1("slice_len", s)), str_const("usize"))
        }),
    );
    must(
        "slice_len__determinism",
        forall(String_(), |s| {
            eq(ctor1("slice_len", s.clone()), ctor1("slice_len", s))
        }),
    );
    must(
        "slice_len__nonneg",
        forall(String_(), |s| gte(ctor1("slice_len", s), num(0))),
    );

    // ---------------- slice::is_empty -----------------
    must(
        "slice_is_empty__type_signature",
        forall(String_(), |s| {
            eq(
                ctor1("type_of", ctor1("slice_is_empty", s)),
                str_const("bool"),
            )
        }),
    );
    must(
        "slice_is_empty__determinism",
        forall(String_(), |s| {
            eq(
                ctor1("slice_is_empty", s.clone()),
                ctor1("slice_is_empty", s),
            )
        }),
    );
    must(
        "slice_is_empty__agrees_with_len_zero_check",
        forall(String_(), |s| {
            eq(
                ctor1("slice_is_empty", s.clone()),
                ctor1("len_eq_zero", ctor1("slice_len", s)),
            )
        }),
    );

    // ---------------- slice::iter -----------------
    must(
        "slice_iter__type_signature",
        forall(String_(), |s| {
            eq(ctor1("type_of", ctor1("slice_iter", s)), str_const("Iter"))
        }),
    );
    must(
        "slice_iter__determinism",
        forall(String_(), |s| {
            eq(ctor1("slice_iter", s.clone()), ctor1("slice_iter", s))
        }),
    );

    // ---------------- slice::get -----------------
    must(
        "slice_get__type_signature",
        forall(String_(), |s| {
            eq(
                ctor1("type_of", ctor2("slice_get", s, num(0))),
                str_const("Option"),
            )
        }),
    );
    must(
        "slice_get__determinism",
        forall(String_(), |s| {
            eq(
                ctor2("slice_get", s.clone(), num(0)),
                ctor2("slice_get", s, num(0)),
            )
        }),
    );
    // Structural: get is total (returns Option, never panics).
    must(
        "slice_get__is_total",
        forall(String_(), |_| {
            eq(
                ctor1("is_partial", str_const("slice_get")),
                str_const("false"),
            )
        }),
    );

    // ---------------- slice::first -----------------
    must(
        "slice_first__type_signature",
        forall(String_(), |s| {
            eq(
                ctor1("type_of", ctor1("slice_first", s)),
                str_const("Option"),
            )
        }),
    );
    must(
        "slice_first__determinism",
        forall(String_(), |s| {
            eq(ctor1("slice_first", s.clone()), ctor1("slice_first", s))
        }),
    );

    // ---------------- slice::last -----------------
    must(
        "slice_last__type_signature",
        forall(String_(), |s| {
            eq(
                ctor1("type_of", ctor1("slice_last", s)),
                str_const("Option"),
            )
        }),
    );
    must(
        "slice_last__determinism",
        forall(String_(), |s| {
            eq(ctor1("slice_last", s.clone()), ctor1("slice_last", s))
        }),
    );

    // ---------------- slice::contains -----------------
    must(
        "slice_contains__type_signature",
        forall(String_(), |s| {
            eq(
                ctor1("type_of", ctor2("slice_contains", s, num(0))),
                str_const("bool"),
            )
        }),
    );
    must(
        "slice_contains__determinism",
        forall(String_(), |s| {
            eq(
                ctor2("slice_contains", s.clone(), num(0)),
                ctor2("slice_contains", s, num(0)),
            )
        }),
    );

    // ---------------- slice::to_vec -----------------
    must(
        "slice_to_vec__type_signature",
        forall(String_(), |s| {
            eq(ctor1("type_of", ctor1("slice_to_vec", s)), str_const("Vec"))
        }),
    );
    must(
        "slice_to_vec__determinism",
        forall(String_(), |s| {
            eq(ctor1("slice_to_vec", s.clone()), ctor1("slice_to_vec", s))
        }),
    );
    // Structural: to_vec preserves length.
    must(
        "slice_to_vec__preserves_length",
        forall(String_(), |s| {
            eq(
                ctor1("Vec_len", ctor1("slice_to_vec", s.clone())),
                ctor1("slice_len", s),
            )
        }),
    );
}
