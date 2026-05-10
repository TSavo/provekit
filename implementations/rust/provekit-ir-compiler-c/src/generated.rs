// SPDX-License-Identifier: Apache-2.0
//
// Hand-maintained operation table for the current core subset.
// The full operation set is mint-more-realizations, not new machinery:
// each added operation is another homomorphic table entry plus its receipt.

pub const CORE_OPERATION_SUBSET: &[&str] = &[
    "seq", "if", "while", "return", "call", "break", "continue", "skip", "eq", "lt", "le", "add",
    "sub", "mul", "neg", "and", "or", "not", "var", "const", "deref", "assign",
];

pub const ALGEBRA_TO_C_TABLE: &[(&str, &str)] = &[
    ("seq(a,b)", "emit a followed by b"),
    (
        "if(c,t,e) statement",
        "if c { t } else { e } with structured blocks",
    ),
    (
        "if(c,t,e) expression",
        "(c) ? (t) : (e) with expression branches",
    ),
    ("while(c,b)", "while c { b }"),
    ("return(e)", "return (e);"),
    ("call(f,args)", "f(args)"),
    ("break(unit)", "break;"),
    ("continue(unit)", "continue;"),
    ("skip(unit)", ";"),
    ("var x", "x"),
    ("const n", "n"),
    ("eq(a,b)", "((a) == (b))"),
    ("lt(a,b)", "((a) < (b))"),
    ("le(a,b)", "((a) <= (b))"),
    ("add(a,b)", "((a) + (b))"),
    ("sub(a,b)", "((a) - (b))"),
    ("mul(a,b)", "((a) * (b))"),
    ("neg(a)", "(-(a))"),
    ("and(a,b)", "((a) && (b))"),
    ("or(a,b)", "((a) || (b))"),
    ("not(a)", "(!(a))"),
    ("deref(p)", "(*(p))"),
    (
        "assign(lv,e)",
        "lv = (e); in statement position, (lv = (e)) in expression position",
    ),
];
