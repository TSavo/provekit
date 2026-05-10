// SPDX-License-Identifier: Apache-2.0
//
// Draft operation table for the hand-mapped core subset. This is kept in a
// separate module to mirror the generated-table shape used by sibling
// compilers while the full catalog-backed emitter remains future work.

pub const ALGEBRA_TO_WASM_TABLE: &[(&str, &str)] = &[
    (
        "seq(a,b,...)",
        "emit each statement in order; drop expression results in statement position",
    ),
    (
        "if(c,t,e) statement",
        "emit c; if; emit t; else; emit e; end",
    ),
    (
        "if(c,t,e) expression",
        "emit c; if (result i32); emit t; else; emit e; end",
    ),
    (
        "while(c,b)",
        "block break; loop continue; emit c; i32.eqz; br_if break; emit b; br continue; end; end",
    ),
    ("return(e)", "emit e; return"),
    ("call(f,args...)", "emit args left to right; call $f"),
    ("break()", "br $breakN"),
    ("break(c)", "emit c; br_if $breakN"),
    ("continue()", "br $continueN"),
    ("continue(c)", "emit c; br_if $continueN"),
    ("skip", "emit no instructions"),
    ("var x", "local.get $x"),
    ("const n", "i32.const n"),
    ("eq(a,b)", "emit a; emit b; i32.eq"),
    ("lt(a,b)", "emit a; emit b; i32.lt_s"),
    ("le(a,b)", "emit a; emit b; i32.le_s"),
    ("add(a,b)", "emit a; emit b; i32.add"),
    ("sub(a,b)", "emit a; emit b; i32.sub"),
    ("mul(a,b)", "emit a; emit b; i32.mul"),
    ("neg(a)", "i32.const 0; emit a; i32.sub"),
    ("and(a,b)", "emit a; emit b; i32.and"),
    ("or(a,b)", "emit a; emit b; i32.or"),
    ("not(a)", "emit a; i32.eqz"),
    ("deref(p)", "emit p; i32.load"),
    (
        "assign(x,v)",
        "emit v; local.set $x when x is a local variable",
    ),
    ("assign(p,v)", "emit p; emit v; i32.store otherwise"),
];
