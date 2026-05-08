// SPDX-License-Identifier: Apache-2.0
//
// std::collections::{HashMap, BTreeMap} slab.
//
// HashMap and BTreeMap share most public-API shape; we author per-map
// contracts so the predicates are kit-callable on either type. The
// disambiguating prefix is `Map_` (HashMap) and `BMap_` (BTreeMap)
// respectively.
//
// Coverage (8 builtins): Map_new, Map_len, Map_is_empty, Map_get,
// Map_insert, Map_contains_key, Map_iter, Map_remove. (Each contract
// mirrors a BTreeMap analog when the structural predicate transfers.)

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

fn ctor3(name: &str, a: Rc<Term>, b: Rc<Term>, c: Rc<Term>) -> Rc<Term> {
    Rc::new(Term::Ctor {
        name: name.into(),
        args: vec![a, b, c],
    })
}

pub fn invariants() {
    // ---------------- HashMap::new -----------------
    must(
        "Map_new__type_signature",
        forall(String_(), |_| {
            eq(
                ctor1("type_of", ctor1("Map_new", num(0))),
                str_const("HashMap"),
            )
        }),
    );
    must(
        "Map_new__determinism",
        forall(String_(), |_| {
            eq(ctor1("Map_new", num(0)), ctor1("Map_new", num(0)))
        }),
    );
    contract(
        "Map_new__starts_empty",
        ContractArgs {
            post: Some(forall(String_(), |_| {
                eq(ctor1("Map_len", ctor1("Map_new", num(0))), num(0))
            })),
            ..Default::default()
        },
    );

    // ---------------- HashMap::len -----------------
    must(
        "Map_len__type_signature",
        forall(String_(), |m| {
            eq(ctor1("type_of", ctor1("Map_len", m)), str_const("usize"))
        }),
    );
    must(
        "Map_len__determinism",
        forall(String_(), |m| {
            eq(ctor1("Map_len", m.clone()), ctor1("Map_len", m))
        }),
    );
    must(
        "Map_len__nonneg",
        forall(String_(), |m| gte(ctor1("Map_len", m), num(0))),
    );

    // ---------------- HashMap::is_empty -----------------
    must(
        "Map_is_empty__type_signature",
        forall(String_(), |m| {
            eq(
                ctor1("type_of", ctor1("Map_is_empty", m)),
                str_const("bool"),
            )
        }),
    );
    must(
        "Map_is_empty__determinism",
        forall(String_(), |m| {
            eq(ctor1("Map_is_empty", m.clone()), ctor1("Map_is_empty", m))
        }),
    );
    must(
        "Map_is_empty__agrees_with_len_zero_check",
        forall(String_(), |m| {
            eq(
                ctor1("Map_is_empty", m.clone()),
                ctor1("len_eq_zero", ctor1("Map_len", m)),
            )
        }),
    );

    // ---------------- HashMap::get -----------------
    must(
        "Map_get__type_signature",
        forall(String_(), |m| {
            eq(
                ctor1("type_of", ctor2("Map_get", m, str_const("k"))),
                str_const("Option"),
            )
        }),
    );
    must(
        "Map_get__determinism",
        forall(String_(), |m| {
            eq(
                ctor2("Map_get", m.clone(), str_const("k")),
                ctor2("Map_get", m, str_const("k")),
            )
        }),
    );
    must(
        "Map_get__is_total",
        forall(String_(), |_| {
            eq(
                ctor1("is_partial", str_const("Map_get")),
                str_const("false"),
            )
        }),
    );

    // ---------------- HashMap::insert -----------------
    must(
        "Map_insert__type_signature",
        forall(String_(), |m| {
            eq(
                ctor1("type_of", ctor3("Map_insert", m, str_const("k"), num(0))),
                str_const("Option"),
            )
        }),
    );
    must(
        "Map_insert__determinism",
        forall(String_(), |m| {
            eq(
                ctor3("Map_insert", m.clone(), str_const("k"), num(0)),
                ctor3("Map_insert", m, str_const("k"), num(0)),
            )
        }),
    );
    // Structural: after insert, the key is present.
    must(
        "Map_insert__post_state_contains_key",
        forall(String_(), |m| {
            eq(
                ctor2(
                    "Map_contains_key",
                    ctor3("Map_insert_post", m, str_const("k"), num(0)),
                    str_const("k"),
                ),
                str_const("true"),
            )
        }),
    );

    // ---------------- HashMap::contains_key -----------------
    must(
        "Map_contains_key__type_signature",
        forall(String_(), |m| {
            eq(
                ctor1("type_of", ctor2("Map_contains_key", m, str_const("k"))),
                str_const("bool"),
            )
        }),
    );
    must(
        "Map_contains_key__determinism",
        forall(String_(), |m| {
            eq(
                ctor2("Map_contains_key", m.clone(), str_const("k")),
                ctor2("Map_contains_key", m, str_const("k")),
            )
        }),
    );
    // Structural: contains_key agrees with `get(k).is_some()`.
    must(
        "Map_contains_key__agrees_with_get_is_some",
        forall(String_(), |m| {
            eq(
                ctor2("Map_contains_key", m.clone(), str_const("k")),
                ctor1("Option_is_some", ctor2("Map_get", m, str_const("k"))),
            )
        }),
    );

    // ---------------- HashMap::iter -----------------
    must(
        "Map_iter__type_signature",
        forall(String_(), |m| {
            eq(ctor1("type_of", ctor1("Map_iter", m)), str_const("Iter"))
        }),
    );
    must(
        "Map_iter__determinism",
        forall(String_(), |m| {
            eq(ctor1("Map_iter", m.clone()), ctor1("Map_iter", m))
        }),
    );

    // ---------------- HashMap::remove -----------------
    must(
        "Map_remove__type_signature",
        forall(String_(), |m| {
            eq(
                ctor1("type_of", ctor2("Map_remove", m, str_const("k"))),
                str_const("Option"),
            )
        }),
    );
    must(
        "Map_remove__determinism",
        forall(String_(), |m| {
            eq(
                ctor2("Map_remove", m.clone(), str_const("k")),
                ctor2("Map_remove", m, str_const("k")),
            )
        }),
    );
    // Structural: after remove, contains_key returns false.
    must(
        "Map_remove__post_state_lacks_key",
        forall(String_(), |m| {
            eq(
                ctor2(
                    "Map_contains_key",
                    ctor2("Map_remove_post", m, str_const("k")),
                    str_const("k"),
                ),
                str_const("false"),
            )
        }),
    );
}
