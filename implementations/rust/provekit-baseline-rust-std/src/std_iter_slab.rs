// SPDX-License-Identifier: Apache-2.0
//
// std::iter slab — iterator + numeric stragglers.
//
// Coverage (6 builtins): Iter_count, Iter_collect, Iter_fold,
// Iter_map, Iter_filter, Iter_next.

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

fn ctor3(name: &str, a: Rc<Term>, b: Rc<Term>, c: Rc<Term>) -> Rc<Term> {
    Rc::new(Term::Ctor {
        name: name.into(),
        args: vec![a, b, c],
    })
}

pub fn invariants() {
    // ---------------- Iterator::count -----------------
    // Consumes the iterator; returns usize.
    must(
        "Iter_count__type_signature",
        forall(String_(), |i| {
            eq(ctor1("type_of", ctor1("Iter_count", i)), str_const("usize"))
        }),
    );
    must(
        "Iter_count__determinism",
        forall(String_(), |i| {
            eq(ctor1("Iter_count", i.clone()), ctor1("Iter_count", i))
        }),
    );
    must(
        "Iter_count__nonneg",
        forall(String_(), |i| gte(ctor1("Iter_count", i), num(0))),
    );

    // ---------------- Iterator::collect -----------------
    // Polymorphic in the destination type; we model the type tag as
    // generic Collection.
    must(
        "Iter_collect__type_signature",
        forall(String_(), |i| {
            eq(
                ctor1("type_of", ctor1("Iter_collect", i)),
                str_const("Collection"),
            )
        }),
    );
    must(
        "Iter_collect__determinism",
        forall(String_(), |i| {
            eq(ctor1("Iter_collect", i.clone()), ctor1("Iter_collect", i))
        }),
    );

    // ---------------- Iterator::fold -----------------
    must(
        "Iter_fold__type_signature",
        forall(String_(), |i| {
            eq(
                ctor1("type_of", ctor3("Iter_fold", i, num(0), str_const("f"))),
                str_const("B"),
            )
        }),
    );
    must(
        "Iter_fold__determinism",
        forall(String_(), |i| {
            eq(
                ctor3("Iter_fold", i.clone(), num(0), str_const("f")),
                ctor3("Iter_fold", i, num(0), str_const("f")),
            )
        }),
    );

    // ---------------- Iterator::map -----------------
    // Lazy adaptor; returns a Map iterator.
    must(
        "Iter_map__type_signature",
        forall(String_(), |i| {
            eq(
                ctor1("type_of", ctor2("Iter_map", i, str_const("f"))),
                str_const("Map"),
            )
        }),
    );
    must(
        "Iter_map__determinism",
        forall(String_(), |i| {
            eq(
                ctor2("Iter_map", i.clone(), str_const("f")),
                ctor2("Iter_map", i, str_const("f")),
            )
        }),
    );
    // Structural: map preserves length when consumed.
    must(
        "Iter_map__count_preserved",
        forall(String_(), |i| {
            eq(
                ctor1("Iter_count", ctor2("Iter_map", i.clone(), str_const("f"))),
                ctor1("Iter_count", i),
            )
        }),
    );

    // ---------------- Iterator::filter -----------------
    must(
        "Iter_filter__type_signature",
        forall(String_(), |i| {
            eq(
                ctor1("type_of", ctor2("Iter_filter", i, str_const("p"))),
                str_const("Filter"),
            )
        }),
    );
    must(
        "Iter_filter__determinism",
        forall(String_(), |i| {
            eq(
                ctor2("Iter_filter", i.clone(), str_const("p")),
                ctor2("Iter_filter", i, str_const("p")),
            )
        }),
    );
    // Structural: filter never grows the count.
    must(
        "Iter_filter__count_does_not_grow",
        forall(String_(), |i| {
            gte(
                ctor1("Iter_count", i.clone()),
                ctor1("Iter_count", ctor2("Iter_filter", i, str_const("p"))),
            )
        }),
    );

    // ---------------- Iterator::next -----------------
    // Mutating: advances the iterator. Models functionally; the
    // returned Option is Some(item) until exhaustion.
    must(
        "Iter_next__type_signature",
        forall(String_(), |i| {
            eq(ctor1("type_of", ctor1("Iter_next", i)), str_const("Option"))
        }),
    );
    must(
        "Iter_next__determinism",
        forall(String_(), |i| {
            eq(ctor1("Iter_next", i.clone()), ctor1("Iter_next", i))
        }),
    );
}
