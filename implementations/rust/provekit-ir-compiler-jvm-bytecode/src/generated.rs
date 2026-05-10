// SPDX-License-Identifier: Apache-2.0
//
// Draft operation table for the hand-mapped core subset. The full
// operation set is mint-more-realizations, not new machinery: each
// added operation gets a table entry, lowering rule, and receipt.

pub const CORE_OPERATION_SUBSET: &[&str] = &[
    "seq", "if", "while", "return", "call", "break", "continue", "skip", "eq", "lt", "le", "add",
    "sub", "mul", "neg", "and", "or", "not", "deref", "assign",
];

pub const ALGEBRA_TO_JVM_TABLE: &[(&str, &str)] = &[
    ("seq(a,b,...)", "emit each statement in order"),
    (
        "if(c,t,e) statement",
        "emit c; ifeq L_else; emit t; goto L_end when t falls through; L_else: emit e; L_end:",
    ),
    (
        "if(c,t,e) expression",
        "emit c; ifeq L_else; emit t; goto L_end; L_else: emit e; L_end:",
    ),
    (
        "while(c,b)",
        "L_top: emit c; ifeq L_done; emit b; goto L_top; L_done:",
    ),
    ("return(e)", "emit e; ireturn"),
    (
        "call(f,args...)",
        "emit args left to right; invokestatic Class/f(II...)I",
    ),
    ("break", "goto the active loop done label"),
    ("continue", "goto the active loop top label"),
    ("skip", "emit no instructions"),
    ("var x", "iload slot(x)"),
    ("const n", "iconst, bipush, sipush, or ldc"),
    (
        "eq(a,b)",
        "emit a; emit b; if_icmpeq L_true; iconst_0; goto L_end; L_true: iconst_1; L_end:",
    ),
    ("lt(a,b)", "same shape with if_icmplt"),
    ("le(a,b)", "same shape with if_icmple"),
    ("add(a,b)", "emit a; emit b; iadd"),
    ("sub(a,b)", "emit a; emit b; isub"),
    ("mul(a,b)", "emit a; emit b; imul"),
    ("neg(a)", "emit a; ineg"),
    ("and(a,b)", "short-circuit to 0 or 1 with ifeq"),
    ("or(a,b)", "short-circuit to 0 or 1 with ifne"),
    (
        "not(a)",
        "emit a; ifeq L_true; iconst_0; goto L_end; L_true: iconst_1; L_end:",
    ),
    (
        "deref(p)",
        "getstatic Class/memory [I; emit p; iaload in the minimal int-memory model",
    ),
    ("assign(x,v)", "emit v; istore slot(x) for local variables"),
    (
        "assign(p,v)",
        "getstatic Class/memory [I; emit p; emit v; iastore otherwise",
    ),
];
