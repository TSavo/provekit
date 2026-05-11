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
    ("bop_eq(a,b)", "emit a; emit b; i32.eq"),
    ("bop_ne(a,b)", "emit a; emit b; i32.ne"),
    ("bop_lt(a,b)", "emit a; emit b; i32.lt_s"),
    ("bop_le(a,b)", "emit a; emit b; i32.le_s"),
    ("bop_gt(a,b)", "emit a; emit b; i32.gt_s"),
    ("bop_ge(a,b)", "emit a; emit b; i32.ge_s"),
    ("add(a,b)", "emit a; emit b; i32.add"),
    ("sub(a,b)", "emit a; emit b; i32.sub"),
    ("mul(a,b)", "emit a; emit b; i32.mul"),
    ("bop_add(a,b)", "emit a; emit b; i32.add"),
    ("bop_sub(a,b)", "emit a; emit b; i32.sub"),
    ("bop_mul(a,b)", "emit a; emit b; i32.mul"),
    ("bop_div(a,b)", "emit a; emit b; i32.div_s"),
    ("bop_mod(a,b)", "emit a; emit b; i32.rem_s"),
    ("bop_shl(a,b)", "emit a; emit b; i32.shl"),
    ("bop_shr(a,b)", "emit a; emit b; i32.shr_s"),
    ("bop_bitand(a,b)", "emit a; emit b; i32.and"),
    ("bop_bitor(a,b)", "emit a; emit b; i32.or"),
    ("bop_bitxor(a,b)", "emit a; emit b; i32.xor"),
    (
        "bop_logand(a,b)",
        "emit a; short-circuit false branch; emit b only when a is truthy; normalize result to 0/1",
    ),
    (
        "bop_logor(a,b)",
        "emit a; short-circuit true branch; emit b only when a is false; normalize result to 0/1",
    ),
    ("bop_comma(a,b)", "emit a; drop; emit b"),
    ("neg(a)", "i32.const 0; emit a; i32.sub"),
    ("uop_neg(a)", "i32.const 0; emit a; i32.sub"),
    ("uop_lognot(a)", "emit a; i32.eqz"),
    ("uop_bitnot(a)", "emit a; i32.const -1; i32.xor"),
    ("uop_plus(a)", "emit a"),
    ("and(a,b)", "emit a; emit b; i32.and"),
    ("or(a,b)", "emit a; emit b; i32.or"),
    ("not(a)", "emit a; i32.eqz"),
    ("deref(p)", "emit p; i32.load"),
    ("uop_deref(p)", "emit p; i32.load"),
    (
        "source-unit(bytes, term)",
        "project to term before lowering",
    ),
    (
        "cast(target_type,value)",
        "target_type is type metadata; lower value in the i32 subset",
    ),
    (
        "assign(x,v)",
        "emit v; local.set $x when x is a local variable",
    ),
    ("assign(p,v)", "emit p; emit v; i32.store otherwise"),
];
