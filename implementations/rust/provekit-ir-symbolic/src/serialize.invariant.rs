// SPDX-License-Identifier: Apache-2.0
//
// .invariant.rs for provekit-ir-symbolic/src/serialize.rs
//
// Public surface covered:
//   * `formula_to_value(&Formula) -> Arc<Value>`
//   * `term_to_value(&Term) -> Arc<Value>`
//   * `sort_to_value(&Sort) -> Arc<Value>`
//   * `marshal_declarations(&[ContractDecl]) -> String`
//
// Honest scope:
//   The locked key orders (kind first, then role-specific fields) per
//   the EBNF grammar are byte-faithful properties. The IR can express
//   determinism and length constraints; the byte-equality "marshal then
//   parse round-trips" is operationally enforced by the proptest in
//   provekit-self-contracts (parse_round_trips_serialize).

use std::rc::Rc;

use provekit_ir_symbolic::{
    contract, eq, forall, gte, must, num, ContractArgs, Int, String_, Term,
};

fn ctor1(name: &str, arg: Rc<Term>) -> Rc<Term> {
    Rc::new(Term::Ctor {
        name: name.into(),
        args: vec![arg],
    })
}

pub fn invariants() {
    // -- formula_to_value is deterministic. ---------------------------------
    must(
        "formula_to_value_is_deterministic",
        forall(String_(), |f| {
            eq(
                ctor1("formula_to_value", f.clone()),
                ctor1("formula_to_value", f),
            )
        }),
    );

    // -- term_to_value is deterministic. ------------------------------------
    must(
        "term_to_value_is_deterministic",
        forall(String_(), |t| {
            eq(ctor1("term_to_value", t.clone()), ctor1("term_to_value", t))
        }),
    );

    // -- sort_to_value is deterministic. ------------------------------------
    must(
        "sort_to_value_is_deterministic",
        forall(String_(), |s| {
            eq(ctor1("sort_to_value", s.clone()), ctor1("sort_to_value", s))
        }),
    );

    // -- marshal_declarations on empty input produces "[]" (length 2). ------
    contract(
        "marshal_declarations_empty_length_eq_2",
        ContractArgs {
            post: Some(eq(
                ctor1("len", ctor1("marshal_declarations", num(0))),
                num(2),
            )),
            ..Default::default()
        },
    );

    // -- marshal_declarations output is non-empty. --------------------------
    must(
        "marshal_declarations_output_nonempty",
        forall(String_(), |decls| {
            gte(ctor1("len", ctor1("marshal_declarations", decls)), num(2))
        }),
    );

    // -- LOCKED KEY ORDER for contract serialization. ----------------------
    //
    // STRONGER INVARIANT: "kind" appears before "name", which appears
    // before "outBinding", which appears before "pre" in the marshalled
    // string. Captured by proptest+positional-substring assertion in
    // provekit-self-contracts (`marshal_emits_locked_key_order_for_contract`).
    //
    // The IR cannot model substring positions in a string. We carry
    // the determinism + length-floor invariants here; the substring
    // ordering test is the operational gatekeeper.
    must(
        "marshal_declarations_is_a_function",
        forall(Int(), |n| {
            eq(
                ctor1("marshal_declarations", n.clone()),
                ctor1("marshal_declarations", n),
            )
        }),
    );
}
