// SPDX-License-Identifier: Apache-2.0
//
// trybuild PASS fixture: a function annotated with the decorator,
// using the kit's primitives. Compiles and registers in the inventory
// distributed slice.

use provekit_ir_symbolic::{forall, gt, num, Int};
use provekit_macros::contract;
use provekit_macros_rt::registered_contracts;

#[contract(pre = forall(Int(), |n| gt(n, num(0))))]
fn parse_positive_int(_s: &str) -> i64 {
    1
}

#[contract(
    name = "explicit_named",
    post = forall(Int(), |n| gt(n, num(-1)))
)]
fn some_other(_s: &str) -> i64 {
    0
}

fn main() {
    // Drive both registered builders. Names default to fn ident,
    // unless overridden with `name = "..."`.
    let names: Vec<&'static str> = registered_contracts().map(|r| r.name).collect();
    assert!(
        names.contains(&"parse_positive_int"),
        "expected parse_positive_int registration, got {names:?}"
    );
    // The macro's inventory `name` field is the fn ident; the override
    // applies to the ContractDecl's `name` only. Assert via builder.
    let mut found_explicit = false;
    for r in registered_contracts() {
        let d = (r.builder)();
        if d.name == "explicit_named" {
            found_explicit = true;
        }
    }
    assert!(found_explicit, "name override did not apply");
    let _ = parse_positive_int("1");
    let _ = some_other("");
}
