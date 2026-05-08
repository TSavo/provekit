// SPDX-License-Identifier: Apache-2.0
//
// std::result slab — builtins on `Result<T, E>`.
//
// Coverage (8 builtins): Result_is_ok, Result_is_err, Result_unwrap,
// Result_unwrap_or, Result_unwrap_err, Result_map, Result_map_err,
// Result_ok.

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
    // ---------------- Result::is_ok -----------------
    must(
        "Result_is_ok__type_signature",
        forall(String_(), |r| {
            eq(
                ctor1("type_of", ctor1("Result_is_ok", r)),
                str_const("bool"),
            )
        }),
    );
    must(
        "Result_is_ok__determinism",
        forall(String_(), |r| {
            eq(ctor1("Result_is_ok", r.clone()), ctor1("Result_is_ok", r))
        }),
    );
    // Structural: is_ok and is_err are complements.
    must(
        "Result_is_ok__complements_is_err",
        forall(String_(), |r| {
            eq(
                ctor2(
                    "xor_bool",
                    ctor1("Result_is_ok", r.clone()),
                    ctor1("Result_is_err", r),
                ),
                str_const("true"),
            )
        }),
    );

    // ---------------- Result::is_err -----------------
    must(
        "Result_is_err__type_signature",
        forall(String_(), |r| {
            eq(
                ctor1("type_of", ctor1("Result_is_err", r)),
                str_const("bool"),
            )
        }),
    );
    must(
        "Result_is_err__determinism",
        forall(String_(), |r| {
            eq(ctor1("Result_is_err", r.clone()), ctor1("Result_is_err", r))
        }),
    );

    // ---------------- Result::unwrap -----------------
    must(
        "Result_unwrap__type_signature",
        forall(String_(), |r| {
            eq(ctor1("type_of", ctor1("Result_unwrap", r)), str_const("T"))
        }),
    );
    must(
        "Result_unwrap__determinism",
        forall(String_(), |r| {
            eq(ctor1("Result_unwrap", r.clone()), ctor1("Result_unwrap", r))
        }),
    );
    // Structural: unwrap is partial (panics on Err).
    must(
        "Result_unwrap__is_partial",
        forall(String_(), |_| {
            eq(
                ctor1("is_partial", str_const("Result_unwrap")),
                str_const("true"),
            )
        }),
    );

    // ---------------- Result::unwrap_or -----------------
    must(
        "Result_unwrap_or__type_signature",
        forall(String_(), |r| {
            eq(
                ctor1("type_of", ctor2("Result_unwrap_or", r, num(0))),
                str_const("T"),
            )
        }),
    );
    must(
        "Result_unwrap_or__determinism",
        forall(String_(), |r| {
            eq(
                ctor2("Result_unwrap_or", r.clone(), num(0)),
                ctor2("Result_unwrap_or", r, num(0)),
            )
        }),
    );
    must(
        "Result_unwrap_or__is_total",
        forall(String_(), |_| {
            eq(
                ctor1("is_partial", str_const("Result_unwrap_or")),
                str_const("false"),
            )
        }),
    );

    // ---------------- Result::unwrap_err -----------------
    must(
        "Result_unwrap_err__type_signature",
        forall(String_(), |r| {
            eq(
                ctor1("type_of", ctor1("Result_unwrap_err", r)),
                str_const("E"),
            )
        }),
    );
    must(
        "Result_unwrap_err__determinism",
        forall(String_(), |r| {
            eq(
                ctor1("Result_unwrap_err", r.clone()),
                ctor1("Result_unwrap_err", r),
            )
        }),
    );
    must(
        "Result_unwrap_err__is_partial",
        forall(String_(), |_| {
            eq(
                ctor1("is_partial", str_const("Result_unwrap_err")),
                str_const("true"),
            )
        }),
    );

    // ---------------- Result::map -----------------
    must(
        "Result_map__type_signature",
        forall(String_(), |r| {
            eq(
                ctor1("type_of", ctor2("Result_map", r, str_const("f"))),
                str_const("Result"),
            )
        }),
    );
    must(
        "Result_map__determinism",
        forall(String_(), |r| {
            eq(
                ctor2("Result_map", r.clone(), str_const("f")),
                ctor2("Result_map", r, str_const("f")),
            )
        }),
    );
    // Structural: Result::map preserves Ok/Err tag.
    must(
        "Result_map__preserves_ok_tag",
        forall(String_(), |r| {
            eq(
                ctor1(
                    "Result_is_ok",
                    ctor2("Result_map", r.clone(), str_const("f")),
                ),
                ctor1("Result_is_ok", r),
            )
        }),
    );

    // ---------------- Result::map_err -----------------
    must(
        "Result_map_err__type_signature",
        forall(String_(), |r| {
            eq(
                ctor1("type_of", ctor2("Result_map_err", r, str_const("f"))),
                str_const("Result"),
            )
        }),
    );
    must(
        "Result_map_err__determinism",
        forall(String_(), |r| {
            eq(
                ctor2("Result_map_err", r.clone(), str_const("f")),
                ctor2("Result_map_err", r, str_const("f")),
            )
        }),
    );
    // Structural: map_err preserves the Ok/Err tag.
    must(
        "Result_map_err__preserves_err_tag",
        forall(String_(), |r| {
            eq(
                ctor1(
                    "Result_is_err",
                    ctor2("Result_map_err", r.clone(), str_const("f")),
                ),
                ctor1("Result_is_err", r),
            )
        }),
    );

    // ---------------- Result::ok -----------------
    must(
        "Result_ok__type_signature",
        forall(String_(), |r| {
            eq(ctor1("type_of", ctor1("Result_ok", r)), str_const("Option"))
        }),
    );
    must(
        "Result_ok__determinism",
        forall(String_(), |r| {
            eq(ctor1("Result_ok", r.clone()), ctor1("Result_ok", r))
        }),
    );
    // Structural: r.ok().is_some() iff r.is_ok().
    must(
        "Result_ok__some_iff_is_ok",
        forall(String_(), |r| {
            eq(
                ctor1("Option_is_some", ctor1("Result_ok", r.clone())),
                ctor1("Result_is_ok", r),
            )
        }),
    );
}
