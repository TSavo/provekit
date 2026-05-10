// SPDX-License-Identifier: Apache-2.0
//
// std::option slab: builtins on `Option<T>`.
//
// Coverage (8 builtins): Option_is_some, Option_is_none, Option_unwrap,
// Option_unwrap_or, Option_map, Option_and_then, Option_ok_or,
// Option_take.

use std::rc::Rc;

use provekit_ir_symbolic::{eq, forall, must, num, str_const, String_, Term};

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
    // ---------------- Option::is_some -----------------
    must(
        "Option_is_some__type_signature",
        forall(String_(), |o| {
            eq(
                ctor1("type_of", ctor1("Option_is_some", o)),
                str_const("bool"),
            )
        }),
    );
    must(
        "Option_is_some__determinism",
        forall(String_(), |o| {
            eq(
                ctor1("Option_is_some", o.clone()),
                ctor1("Option_is_some", o),
            )
        }),
    );
    // Structural: is_some & is_none agree (sum to true).
    must(
        "Option_is_some__complements_is_none",
        forall(String_(), |o| {
            eq(
                ctor2(
                    "xor_bool",
                    ctor1("Option_is_some", o.clone()),
                    ctor1("Option_is_none", o),
                ),
                str_const("true"),
            )
        }),
    );

    // ---------------- Option::is_none -----------------
    must(
        "Option_is_none__type_signature",
        forall(String_(), |o| {
            eq(
                ctor1("type_of", ctor1("Option_is_none", o)),
                str_const("bool"),
            )
        }),
    );
    must(
        "Option_is_none__determinism",
        forall(String_(), |o| {
            eq(
                ctor1("Option_is_none", o.clone()),
                ctor1("Option_is_none", o),
            )
        }),
    );

    // ---------------- Option::unwrap -----------------
    // Partial: panics on None. The IR doesn't model panic semantics; we
    // record `is_partial = true` via a kit-defined predicate.
    must(
        "Option_unwrap__type_signature",
        forall(String_(), |o| {
            eq(ctor1("type_of", ctor1("Option_unwrap", o)), str_const("T"))
        }),
    );
    must(
        "Option_unwrap__determinism",
        forall(String_(), |o| {
            eq(ctor1("Option_unwrap", o.clone()), ctor1("Option_unwrap", o))
        }),
    );
    // Structural: unwrap is partial (panics on None).
    must(
        "Option_unwrap__is_partial",
        forall(String_(), |_| {
            eq(
                ctor1("is_partial", str_const("Option_unwrap")),
                str_const("true"),
            )
        }),
    );

    // ---------------- Option::unwrap_or -----------------
    must(
        "Option_unwrap_or__type_signature",
        forall(String_(), |o| {
            eq(
                ctor1("type_of", ctor2("Option_unwrap_or", o, num(0))),
                str_const("T"),
            )
        }),
    );
    must(
        "Option_unwrap_or__determinism",
        forall(String_(), |o| {
            eq(
                ctor2("Option_unwrap_or", o.clone(), num(0)),
                ctor2("Option_unwrap_or", o, num(0)),
            )
        }),
    );
    // Structural: unwrap_or is total (never panics).
    must(
        "Option_unwrap_or__is_total",
        forall(String_(), |_| {
            eq(
                ctor1("is_partial", str_const("Option_unwrap_or")),
                str_const("false"),
            )
        }),
    );

    // ---------------- Option::map -----------------
    must(
        "Option_map__type_signature",
        forall(String_(), |o| {
            eq(
                ctor1("type_of", ctor2("Option_map", o, str_const("f"))),
                str_const("Option"),
            )
        }),
    );
    must(
        "Option_map__determinism",
        forall(String_(), |o| {
            eq(
                ctor2("Option_map", o.clone(), str_const("f")),
                ctor2("Option_map", o, str_const("f")),
            )
        }),
    );
    // Structural: Option::map preserves Some/None tag.
    must(
        "Option_map__preserves_some_tag",
        forall(String_(), |o| {
            eq(
                ctor1(
                    "Option_is_some",
                    ctor2("Option_map", o.clone(), str_const("f")),
                ),
                ctor1("Option_is_some", o),
            )
        }),
    );

    // ---------------- Option::and_then -----------------
    must(
        "Option_and_then__type_signature",
        forall(String_(), |o| {
            eq(
                ctor1("type_of", ctor2("Option_and_then", o, str_const("f"))),
                str_const("Option"),
            )
        }),
    );
    must(
        "Option_and_then__determinism",
        forall(String_(), |o| {
            eq(
                ctor2("Option_and_then", o.clone(), str_const("f")),
                ctor2("Option_and_then", o, str_const("f")),
            )
        }),
    );

    // ---------------- Option::ok_or -----------------
    must(
        "Option_ok_or__type_signature",
        forall(String_(), |o| {
            eq(
                ctor1("type_of", ctor2("Option_ok_or", o, str_const("err"))),
                str_const("Result"),
            )
        }),
    );
    must(
        "Option_ok_or__determinism",
        forall(String_(), |o| {
            eq(
                ctor2("Option_ok_or", o.clone(), str_const("err")),
                ctor2("Option_ok_or", o, str_const("err")),
            )
        }),
    );

    // ---------------- Option::take -----------------
    // Mutating: replaces the receiver with None and returns the previous
    // value. Modeled functionally as `Option_take(o) -> Option<T>`.
    must(
        "Option_take__type_signature",
        forall(String_(), |o| {
            eq(
                ctor1("type_of", ctor1("Option_take", o)),
                str_const("Option"),
            )
        }),
    );
    must(
        "Option_take__determinism",
        forall(String_(), |o| {
            eq(ctor1("Option_take", o.clone()), ctor1("Option_take", o))
        }),
    );
    // Structural: post-state is None.
    must(
        "Option_take__post_state_none",
        forall(String_(), |o| {
            eq(
                ctor1("Option_is_none", ctor1("Option_take_post", o)),
                str_const("true"),
            )
        }),
    );
}
