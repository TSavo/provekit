// SPDX-License-Identifier: Apache-2.0
//
// Hand-maintained operation table for the current core subset. The full
// C11 operation set should mint more realization rows, not new machinery.

pub const CORE_OPERATION_SUBSET: &[&str] = &[
    "seq", "if", "while", "return", "call", "break", "continue", "skip", "eq", "lt", "le", "add",
    "sub", "mul", "neg", "and", "or", "not", "deref", "assign",
];

pub const LANGUAGE_MORPHISM_TABLE: &[(&str, &str)] = &[
    ("seq(a, b)", "emit a followed by b"),
    (
        "if(c, a, b)",
        "compile c to eax, cmp eax, 0, branch with je",
    ),
    (
        "while(c, b)",
        "loop label, condition compare, exit branch, body, back edge",
    ),
    ("return(e)", "compile e to eax, ret"),
    (
        "call(f, args)",
        "SysV integer argument registers, call f, result in eax",
    ),
    ("break", "jump to innermost loop exit label"),
    ("continue", "jump to innermost loop head label"),
    ("skip", "emit no instruction"),
    ("eq(a, b)", "cmp a, b followed by sete"),
    ("lt(a, b)", "cmp a, b followed by setl"),
    ("le(a, b)", "cmp a, b followed by setle"),
    ("add(a, b)", "add eax, ecx"),
    ("sub(a, b)", "sub eax, ecx"),
    ("mul(a, b)", "imul eax, ecx"),
    ("neg(a)", "neg eax"),
    ("and(a, b)", "normalize to 0 or 1, and eax, ecx"),
    ("or(a, b)", "normalize to 0 or 1, or eax, ecx"),
    ("not(a)", "cmp eax, 0 followed by sete"),
    ("var(x)", "move from assigned SysV argument register"),
    ("const(i)", "mov eax, immediate"),
    ("deref(p)", "load DWORD PTR [address]"),
    (
        "assign(x, e)",
        "compile e and store into x or DWORD PTR [address]",
    ),
];
