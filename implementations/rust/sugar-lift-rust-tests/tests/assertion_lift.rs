use sugar_ir_symbolic::{ConstValue, Formula, Term};
use sugar_lift_rust_tests::{lift_file, lift_file_with_options, LiftOptions, TargetCfg};

fn parse(src: &str) -> syn::File {
    syn::parse_file(src).expect("fixture parses")
}

fn inv_operands(decl: &sugar_ir_symbolic::ContractDecl) -> &[std::rc::Rc<Formula>] {
    match decl.inv.as_deref() {
        Some(Formula::Connective { kind, operands }) if kind == "and" => operands,
        other => panic!("expected and inv, got {other:?}"),
    }
}

fn assert_eq_atom(formula: &Formula, expected_rhs: i64) {
    match formula {
        Formula::Atomic { name, args } => {
            assert_eq!(name, "=");
            assert_eq!(args.len(), 2);
            match args[1].as_ref() {
                Term::Const {
                    value: ConstValue::Int(value),
                    ..
                } => assert_eq!(*value, expected_rhs),
                other => panic!("expected int rhs, got {other:?}"),
            }
        }
        other => panic!("expected equality atom, got {other:?}"),
    }
}

fn assert_int_call_eq_atom(
    formula: &Formula,
    expected_lhs: i64,
    expected_call: &str,
    expected_arg: i64,
) {
    match formula {
        Formula::Atomic { name, args } => {
            assert_eq!(name, "=");
            assert_eq!(args.len(), 2);
            match args[0].as_ref() {
                Term::Const {
                    value: ConstValue::Int(value),
                    ..
                } => assert_eq!(*value, expected_lhs),
                other => panic!("expected int lhs, got {other:?}"),
            }
            match args[1].as_ref() {
                Term::Ctor { name, args } => {
                    assert_eq!(name, expected_call);
                    assert_eq!(args.len(), 1);
                    match args[0].as_ref() {
                        Term::Const {
                            value: ConstValue::Int(value),
                            ..
                        } => assert_eq!(*value, expected_arg),
                        other => panic!("expected int call argument, got {other:?}"),
                    }
                }
                other => panic!("expected call term rhs, got {other:?}"),
            }
        }
        other => panic!("expected equality atom, got {other:?}"),
    }
}

fn assert_int_zero_arg_call_eq_atom(formula: &Formula, expected_call: &str, expected_rhs: i64) {
    match formula {
        Formula::Atomic { name, args } => {
            assert_eq!(name, "=");
            assert_eq!(args.len(), 2);
            match args[0].as_ref() {
                Term::Ctor { name, args } => {
                    assert_eq!(name, expected_call);
                    assert!(args.is_empty());
                }
                other => panic!("expected call term lhs, got {other:?}"),
            }
            match args[1].as_ref() {
                Term::Const {
                    value: ConstValue::Int(value),
                    ..
                } => assert_eq!(*value, expected_rhs),
                other => panic!("expected int rhs, got {other:?}"),
            }
        }
        other => panic!("expected equality atom, got {other:?}"),
    }
}

fn assert_string_call_eq_atom(formula: &Formula, expected_call: &str, expected_rhs: &str) {
    match formula {
        Formula::Atomic { name, args } => {
            assert_eq!(name, "=");
            assert_eq!(args.len(), 2);
            match args[0].as_ref() {
                Term::Ctor { name, .. } => assert_eq!(name, expected_call),
                other => panic!("expected call term lhs, got {other:?}"),
            }
            match args[1].as_ref() {
                Term::Const {
                    value: ConstValue::String(value),
                    ..
                } => assert_eq!(value, expected_rhs),
                other => panic!("expected string rhs, got {other:?}"),
            }
        }
        other => panic!("expected equality atom, got {other:?}"),
    }
}

fn assert_real_call_eq_atom(formula: &Formula, expected_call: &str, expected_rhs: &str) {
    match formula {
        Formula::Atomic { name, args } => {
            assert_eq!(name, "=");
            assert_eq!(args.len(), 2);
            match args[0].as_ref() {
                Term::Ctor { name, .. } => assert_eq!(name, expected_call),
                other => panic!("expected call term lhs, got {other:?}"),
            }
            match args[1].as_ref() {
                Term::Const {
                    value: ConstValue::Real(value),
                    sort,
                } => {
                    assert_eq!(value, expected_rhs);
                    assert_eq!(sort.name, "Real");
                }
                other => panic!("expected real rhs, got {other:?}"),
            }
        }
        other => panic!("expected equality atom, got {other:?}"),
    }
}

fn assert_float_refinement_atom(formula: &Formula, expected_name: &str, expected_call: &str) {
    match formula {
        Formula::Atomic { name, args } => {
            assert_eq!(name, expected_name);
            assert_eq!(args.len(), 1);
            match args[0].as_ref() {
                Term::Ctor { name, .. } => assert_eq!(name, expected_call),
                other => panic!("expected refined call term, got {other:?}"),
            }
        }
        other => panic!("expected float refinement atom, got {other:?}"),
    }
}

fn assert_int_call_cmp_atom(
    formula: &Formula,
    expected_op: &str,
    expected_call: &str,
    expected_rhs: i64,
) {
    match formula {
        Formula::Atomic { name, args } => {
            assert_eq!(name, expected_op);
            assert_eq!(args.len(), 2);
            match args[0].as_ref() {
                Term::Ctor { name, .. } => assert_eq!(name, expected_call),
                other => panic!("expected call term lhs, got {other:?}"),
            }
            match args[1].as_ref() {
                Term::Const {
                    value: ConstValue::Int(value),
                    ..
                } => assert_eq!(*value, expected_rhs),
                other => panic!("expected int rhs, got {other:?}"),
            }
        }
        other => panic!("expected comparison atom, got {other:?}"),
    }
}

fn assert_method_call_eq_compound_rhs(
    formula: &Formula,
    expected_call: &str,
    expected_rhs_ctor: &str,
    expected_lhs: i64,
    expected_rhs: i64,
) {
    match formula {
        Formula::Atomic { name, args } => {
            assert_eq!(name, "=");
            assert_eq!(args.len(), 2);
            match args[0].as_ref() {
                Term::Ctor { name, .. } => assert_eq!(name, expected_call),
                other => panic!("expected method call lhs, got {other:?}"),
            }
            match args[1].as_ref() {
                Term::Ctor { name, args } => {
                    assert_eq!(name, expected_rhs_ctor);
                    assert_eq!(args.len(), 2);
                    match args[0].as_ref() {
                        Term::Const {
                            value: ConstValue::Int(value),
                            ..
                        } => assert_eq!(*value, expected_lhs),
                        other => panic!("expected int compound lhs, got {other:?}"),
                    }
                    match args[1].as_ref() {
                        Term::Const {
                            value: ConstValue::Int(value),
                            ..
                        } => assert_eq!(*value, expected_rhs),
                        other => panic!("expected int compound rhs, got {other:?}"),
                    }
                }
                other => panic!("expected compound rhs, got {other:?}"),
            }
        }
        other => panic!("expected equality atom, got {other:?}"),
    }
}

fn assert_string_predicate_atom(formula: &Formula, expected_name: &str, expected_strings: &[&str]) {
    match formula {
        Formula::Atomic { name, args } => {
            assert_eq!(name, expected_name);
            assert_eq!(args.len(), expected_strings.len());
            for (arg, expected) in args.iter().zip(expected_strings) {
                match arg.as_ref() {
                    Term::Const {
                        value: ConstValue::String(value),
                        sort,
                    } => {
                        assert_eq!(value, expected);
                        assert_eq!(sort.name, "String");
                    }
                    other => panic!("expected string arg {expected:?}, got {other:?}"),
                }
            }
        }
        other => panic!("expected string predicate atom, got {other:?}"),
    }
}

fn assert_string_len_cmp_atom(
    formula: &Formula,
    expected_op: &str,
    expected_lhs: &str,
    expected_rhs: i64,
) {
    match formula {
        Formula::Atomic { name, args } => {
            assert_eq!(name, expected_op);
            assert_eq!(args.len(), 2);
            match args[0].as_ref() {
                Term::Ctor { name, args } => {
                    assert_eq!(name, "str.len");
                    assert_eq!(args.len(), 1);
                    match args[0].as_ref() {
                        Term::Const {
                            value: ConstValue::String(value),
                            sort,
                        } => {
                            assert_eq!(value, expected_lhs);
                            assert_eq!(sort.name, "String");
                        }
                        other => panic!("expected string len receiver, got {other:?}"),
                    }
                }
                other => panic!("expected str.len term lhs, got {other:?}"),
            }
            match args[1].as_ref() {
                Term::Const {
                    value: ConstValue::Int(value),
                    ..
                } => assert_eq!(*value, expected_rhs),
                other => panic!("expected int rhs, got {other:?}"),
            }
        }
        other => panic!("expected string length comparison atom, got {other:?}"),
    }
}

fn assert_type_id_cmp_atom(
    formula: &Formula,
    expected_op: &str,
    expected_static_type: &str,
    expected_cast_type: &str,
    expected_receiver: &str,
) {
    match formula {
        Formula::Atomic { name, args } => {
            assert_eq!(name, expected_op);
            assert_eq!(args.len(), 2);
            match args[0].as_ref() {
                Term::Ctor { name, args } => {
                    assert_eq!(name, &format!("type_id::{expected_static_type}"));
                    assert!(args.is_empty());
                }
                other => panic!("expected static TypeId term lhs, got {other:?}"),
            }
            match args[1].as_ref() {
                Term::Ctor { name, args } => {
                    assert_eq!(name, "method:type_id");
                    assert_eq!(args.len(), 1);
                    match args[0].as_ref() {
                        Term::Ctor { name, args } => {
                            assert_eq!(name, &format!("cast:{expected_cast_type}"));
                            assert_eq!(args.len(), 1);
                            match args[0].as_ref() {
                                Term::Ctor { name, args } => {
                                    assert_eq!(name, "ref");
                                    assert_eq!(args.len(), 1);
                                    match args[0].as_ref() {
                                        Term::Var { name } => assert_eq!(name, expected_receiver),
                                        other => panic!(
                                            "expected referenced receiver var, got {other:?}"
                                        ),
                                    }
                                }
                                other => panic!("expected ref receiver term, got {other:?}"),
                            }
                        }
                        other => panic!("expected cast receiver term, got {other:?}"),
                    }
                }
                other => panic!("expected dynamic type_id term rhs, got {other:?}"),
            }
        }
        other => panic!("expected TypeId comparison atom, got {other:?}"),
    }
}

enum ExpectedScalar {
    Int(i64),
    Bool(bool),
}

enum ExpectedOperatorArg {
    Constructor(&'static str, ExpectedScalar),
}

fn assert_scalar_const(term: &Term, expected: ExpectedScalar) {
    match (term, expected) {
        (
            Term::Const {
                value: ConstValue::Int(value),
                ..
            },
            ExpectedScalar::Int(expected),
        ) => assert_eq!(*value, expected),
        (
            Term::Const {
                value: ConstValue::Bool(value),
                ..
            },
            ExpectedScalar::Bool(expected),
        ) => assert_eq!(*value, expected),
        (other, _) => panic!("expected scalar const, got {other:?}"),
    }
}

fn assert_operator_arg(term: &Term, expected: &ExpectedOperatorArg) {
    match expected {
        ExpectedOperatorArg::Constructor(expected_name, expected_arg) => match term {
            Term::Ctor { name, args } => {
                assert_eq!(name, expected_name);
                assert_eq!(args.len(), 1);
                match expected_arg {
                    ExpectedScalar::Int(value) => {
                        assert_scalar_const(&args[0], ExpectedScalar::Int(*value));
                    }
                    ExpectedScalar::Bool(value) => {
                        assert_scalar_const(&args[0], ExpectedScalar::Bool(*value));
                    }
                }
            }
            other => panic!("expected constructor operator arg, got {other:?}"),
        },
    }
}

fn assert_operator_bool_atom(
    formula: &Formula,
    expected_call: &str,
    expected_args: &[ExpectedOperatorArg],
    expected_result: bool,
) {
    match formula {
        Formula::Atomic { name, args } => {
            assert_eq!(name, "=");
            assert_eq!(args.len(), 2);
            match args[0].as_ref() {
                Term::Ctor { name, args } => {
                    assert_eq!(name, expected_call);
                    assert_eq!(args.len(), expected_args.len());
                    for (actual, expected) in args.iter().zip(expected_args.iter()) {
                        assert_operator_arg(actual, expected);
                    }
                }
                other => panic!("expected operator call lhs, got {other:?}"),
            }
            assert_scalar_const(&args[1], ExpectedScalar::Bool(expected_result));
        }
        other => panic!("expected operator result equality atom, got {other:?}"),
    }
}

fn formula_contains_atomic_name(formula: &Formula, expected_name: &str) -> bool {
    match formula {
        Formula::Atomic { name, .. } => name == expected_name,
        Formula::Connective { operands, .. } => operands
            .iter()
            .any(|operand| formula_contains_atomic_name(operand, expected_name)),
        Formula::Quantifier { body, .. } => formula_contains_atomic_name(body, expected_name),
        _ => false,
    }
}

fn formula_contains_relation_name(formula: &Formula, expected_name: &str) -> bool {
    formula_contains_atomic_name(formula, expected_name)
}

fn formula_count_atomic_name(formula: &Formula, expected_name: &str) -> usize {
    match formula {
        Formula::Atomic { name, .. } => usize::from(name == expected_name),
        Formula::Connective { operands, .. } => operands
            .iter()
            .map(|operand| formula_count_atomic_name(operand, expected_name))
            .sum(),
        Formula::Quantifier { body, .. } => formula_count_atomic_name(body, expected_name),
        _ => 0,
    }
}

fn formula_count_connective_kind(formula: &Formula, expected_kind: &str) -> usize {
    match formula {
        Formula::Connective { kind, operands } => {
            usize::from(kind == expected_kind)
                + operands
                    .iter()
                    .map(|operand| formula_count_connective_kind(operand, expected_kind))
                    .sum::<usize>()
        }
        Formula::Quantifier { body, .. } => formula_count_connective_kind(body, expected_kind),
        _ => 0,
    }
}

fn assert_await_call_eq_atom(formula: &Formula, expected_call: &str, expected_rhs: i64) {
    match formula {
        Formula::Atomic { name, args } => {
            assert_eq!(name, "=");
            assert_eq!(args.len(), 2);
            match args[0].as_ref() {
                Term::Ctor { name, args } => {
                    assert_eq!(name, "await");
                    assert_eq!(args.len(), 1);
                    match args[0].as_ref() {
                        Term::Ctor { name, args } => {
                            assert_eq!(name, expected_call);
                            assert!(args.is_empty());
                        }
                        other => panic!("expected awaited call term, got {other:?}"),
                    }
                }
                other => panic!("expected await term lhs, got {other:?}"),
            }
            match args[1].as_ref() {
                Term::Const {
                    value: ConstValue::Int(value),
                    ..
                } => assert_eq!(*value, expected_rhs),
                other => panic!("expected int rhs, got {other:?}"),
            }
        }
        other => panic!("expected equality atom, got {other:?}"),
    }
}

#[test]
fn lifts_single_assert_eq_as_inv_only_consistency_contract() {
    let src = r#"
fn make_value() -> i32 { 6 }

#[test]
fn scalar_is_six() {
    assert_eq!(make_value(), 6);
}
"#;
    let out = lift_file(&parse(src), "src/lib.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);

    let decl = &out.decls[0];
    assert_eq!(
        decl.name,
        "make_value#euf#c:callresult_make_value_a0()::assertion"
    );
    assert!(decl.pre.is_none());
    assert!(decl.post.is_none());
    assert!(decl.evidence.is_none());
    let operands = inv_operands(decl);
    assert_eq!(operands.len(), 1);
    assert_eq_atom(&operands[0], 6);
}

#[test]
fn conjoins_contradictory_assert_eq_atoms_in_one_inv() {
    let src = r#"
fn make_value() -> i32 { 6 }

#[test]
fn scalar_contradiction() {
    assert_eq!(make_value(), 6);
    assert_eq!(make_value(), 7);
}
"#;
    let out = lift_file(&parse(src), "tests/contradiction.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);

    let decl = &out.decls[0];
    assert_eq!(
        decl.name,
        "make_value#euf#c:callresult_make_value_a0()::assertion"
    );
    let operands = inv_operands(decl);
    assert_eq!(operands.len(), 2);
    assert_eq_atom(&operands[0], 6);
    assert_eq_atom(&operands[1], 7);
}

#[test]
fn lifts_assert_binary_equality() {
    let src = r#"
fn make_value() -> i32 { 6 }

#[test]
fn scalar_assert_binary() {
    assert!(make_value() == 6);
}
"#;
    let out = lift_file(&parse(src), "src/lib.rs");
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(
        out.decls[0].name,
        "make_value#euf#c:callresult_make_value_a0()::assertion"
    );
    let operands = inv_operands(&out.decls[0]);
    assert_eq!(operands.len(), 1);
    assert_eq_atom(&operands[0], 6);
}

#[test]
fn direct_call_result_assertion_uses_euf_callsite_key_from_rhs() {
    let src = r#"
fn decoded_len_estimate(n: usize) -> usize { n - 1 }

#[test]
fn decoded_len_est() {
    assert_eq!(3, decoded_len_estimate(4));
}
"#;
    let out = lift_file(&parse(src), "src/decode.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);

    let decl = &out.decls[0];
    assert_eq!(
        decl.name,
        "decoded_len_estimate#euf#c:callresult_decoded_len_estimate_a1(i:4)::assertion"
    );
    let operands = inv_operands(decl);
    assert_eq!(operands.len(), 1);
    assert_int_call_eq_atom(&operands[0], 3, "call:decoded_len_estimate", 4);
}

#[test]
fn direct_generic_call_result_assertions_include_type_args_in_euf_key() {
    let src = r#"
fn size_of<T>() -> usize { 1 }

#[test]
fn generic_identity_is_distinct() {
    assert_eq!(size_of::<u8>(), 1);
    assert_eq!(size_of::<u16>(), 2);
}
"#;
    let out = lift_file(&parse(src), "tests/mem.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 2);

    let names = out
        .decls
        .iter()
        .map(|decl| decl.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "size_of::<u8>#euf#c:callresult_size_of___u8__a0()::assertion",
            "size_of::<u16>#euf#c:callresult_size_of___u16__a0()::assertion",
        ]
    );

    let first = inv_operands(&out.decls[0]);
    assert_eq!(first.len(), 1);
    assert_int_zero_arg_call_eq_atom(&first[0], "call:size_of::<u8>", 1);
    let second = inv_operands(&out.decls[1]);
    assert_eq!(second.len(), 1);
    assert_int_zero_arg_call_eq_atom(&second[0], "call:size_of::<u16>", 2);
}

#[test]
fn cfg_gated_test_functions_lift_only_when_active_for_explicit_target_cfg() {
    let src = r#"
fn size_of<T>() -> usize { 1 }

#[test]
#[cfg(target_pointer_width = "32")]
fn size_of_32() {
    assert_eq!(size_of::<usize>(), 4);
}

#[test]
#[cfg(target_pointer_width = "64")]
fn size_of_64() {
    assert_eq!(size_of::<usize>(), 8);
}

#[test]
#[cfg(all(unix, target_pointer_width = "64"))]
fn size_of_unix_64() {
    assert_eq!(size_of::<*const usize>(), 8);
}

#[test]
#[cfg(any(windows, target_pointer_width = "32"))]
fn size_of_inactive_any() {
    assert_eq!(size_of::<*const usize>(), 4);
}
"#;
    let cfg = TargetCfg::from_rustc_cfg_facts([
        "unix",
        "target_pointer_width=\"64\"",
        "target_arch=\"x86_64\"",
    ])
    .expect("target cfg parses");
    let out = lift_file_with_options(
        &parse(src),
        "tests/mem.rs",
        &LiftOptions::for_target_cfg(cfg),
    );

    assert_eq!(out.seen, 2);
    assert_eq!(out.lifted, 2, "warnings: {:?}", out.warnings);
    let names = out
        .decls
        .iter()
        .map(|decl| decl.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "size_of::<usize>#euf#c:callresult_size_of___usize__a0()::assertion",
            "size_of::<* const usize>#euf#c:callresult_size_of_____const_usize__a0()::assertion",
        ]
    );
    assert_int_zero_arg_call_eq_atom(&inv_operands(&out.decls[0])[0], "call:size_of::<usize>", 8);
    assert_int_zero_arg_call_eq_atom(
        &inv_operands(&out.decls[1])[0],
        "call:size_of::<* const usize>",
        8,
    );
    assert!(
        out.warnings
            .iter()
            .any(|w| w.reason.contains("inactive cfg") && w.item_name.ends_with("size_of_32")),
        "inactive cfg residual must be named: {:?}",
        out.warnings
    );
    assert!(
        out.warnings
            .iter()
            .any(|w| w.reason.contains("inactive cfg")
                && w.item_name.ends_with("size_of_inactive_any")),
        "inactive any cfg residual must be named: {:?}",
        out.warnings
    );
}

#[test]
fn cfg_gated_assertions_are_skipped_without_explicit_target_cfg() {
    let src = r#"
fn size_of<T>() -> usize { 1 }

#[test]
#[cfg(target_pointer_width = "64")]
fn size_of_64() {
    assert_eq!(size_of::<usize>(), 8);
}
"#;
    let out = lift_file(&parse(src), "tests/mem.rs");

    assert_eq!(out.seen, 0);
    assert_eq!(out.lifted, 0);
    assert!(
        out.decls.is_empty(),
        "cfg-gated claim requires explicit target cfg"
    );
    assert_eq!(out.warnings.len(), 1);
    assert!(out.warnings[0].reason.contains("ambiguous cfg"));
    assert!(out.warnings[0].reason.contains("target_pointer_width"));
}

#[test]
fn cfg_test_modules_lift_without_explicit_target_cfg() {
    let src = r#"
fn make_value() -> i32 { 6 }

#[cfg(test)]
mod tests {
    use super::make_value;

    #[test]
    fn scalar_is_six() {
        assert_eq!(make_value(), 6);
    }
}
"#;
    let out = lift_file(&parse(src), "src/lib.rs");

    assert_eq!(out.seen, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);
    assert_eq!(
        out.decls[0].name,
        "make_value#euf#c:callresult_make_value_a0()::assertion"
    );
}

#[test]
fn cfg_test_modules_are_active_without_explicit_target_cfg() {
    let src = r#"
fn decoded_len_estimate(_: usize) -> usize { 3 }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoded_len_est() {
        assert_eq!(3, decoded_len_estimate(4));
    }
}
"#;
    let out = lift_file(&parse(src), "src/decode.rs");

    assert_eq!(out.seen, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);
    assert_eq!(
        out.decls[0].name,
        "decoded_len_estimate#euf#c:callresult_decoded_len_estimate_a1(i:4)::assertion"
    );
    assert_int_call_eq_atom(
        &inv_operands(&out.decls[0])[0],
        3,
        "call:decoded_len_estimate",
        4,
    );
}

#[test]
fn cfg_gated_statement_assertions_lift_only_active_assertions() {
    let src = r#"
fn value() -> i32 { 1 }

#[test]
fn cfg_statements() {
    #[cfg(target_pointer_width = "32")]
    assert_eq!(value(), 4);

    #[cfg(target_pointer_width = "64")]
    assert_eq!(value(), 8);

    #[cfg(all(unix, not(target_pointer_width = "32")))]
    assert_eq!(value(), 9);

    #[cfg(target_feature = "definitely-not-a-real-feature")]
    assert_eq!(value(), 99);
}
"#;
    let cfg = TargetCfg::from_rustc_cfg_facts(["unix", "target_pointer_width=\"64\""])
        .expect("target cfg parses");
    let out = lift_file_with_options(
        &parse(src),
        "tests/cfg.rs",
        &LiftOptions::for_target_cfg(cfg),
    );

    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);
    let operands = inv_operands(&out.decls[0]);
    assert_eq!(operands.len(), 2);
    assert_eq_atom(&operands[0], 8);
    assert_eq_atom(&operands[1], 9);
    assert!(
        out.warnings
            .iter()
            .any(|w| w.reason.contains("inactive cfg") && w.reason.contains("32")),
        "inactive statement cfg residual must be named: {:?}",
        out.warnings
    );
    assert!(
        out.warnings
            .iter()
            .any(|w| w.reason.contains("inactive cfg")
                && w.reason.contains("definitely-not-a-real-feature")),
        "inactive target_feature statement cfg residual must be named: {:?}",
        out.warnings
    );
}

#[test]
fn type_id_of_and_dyn_any_receiver_lift_as_keyed_reflection_terms() {
    let src = r#"
use core::any::TypeId;

#[test]
fn any_fixed_vec_type_id() {
    let test = [0_u8; 3];
    assert_eq!(TypeId::of::<[u8; 3]>(), (&test as &dyn core::any::Any).type_id());
    assert!(TypeId::of::<[u8; 4]>() != (&test as &dyn core::any::Any).type_id());
}
"#;
    let out = lift_file(&parse(src), "coretests/tests/any.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert!(
        out.warnings.is_empty(),
        "unexpected lift warnings: {:?}",
        out.warnings
    );
    assert_eq!(out.decls.len(), 1);

    let decl = &out.decls[0];
    assert_eq!(decl.name, "coretests/tests/any.rs::any_fixed_vec_type_id");
    let operands = inv_operands(decl);
    assert_eq!(operands.len(), 2);
    assert_type_id_cmp_atom(&operands[0], "=", "[u8;3]", "&dyn core::any::Any", "test");
    assert_type_id_cmp_atom(
        &operands[1],
        "\u{2260}",
        "[u8;4]",
        "&dyn core::any::Any",
        "test",
    );
}

#[test]
fn direct_method_call_result_string_assertion_uses_euf_callsite_key() {
    let src = r#"
struct Name;

impl Name {
    fn to_string(&self) -> String { "hello".to_owned() }
}

#[test]
fn string_call_result() {
    let a = Name;
    assert_eq!(a.to_string(), "hello");
}
"#;
    let out = lift_file(&parse(src), "tests/fmt.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);

    let decl = &out.decls[0];
    assert_eq!(
        decl.name,
        "method:to_string#euf#c:callresult_method_to_string_a1(v:tests/fmt.rs::string_call_result::a)::assertion"
    );
    let operands = inv_operands(decl);
    assert_eq!(operands.len(), 1);
    assert_string_call_eq_atom(&operands[0], "method:to_string", "hello");
}

#[test]
fn direct_method_call_result_float_assertion_uses_euf_callsite_key() {
    let src = r#"
struct Duration;

impl Duration {
    fn div_duration_f64(&self, _other: Duration) -> f64 { 2.0 }
}

#[test]
fn float_call_result() {
    let d = Duration;
    assert_eq!(d.div_duration_f64(Duration), 2.0);
}
"#;
    let out = lift_file(&parse(src), "tests/time.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);

    let decl = &out.decls[0];
    assert_eq!(
        decl.name,
        "method:div_duration_f64#euf#c:callresult_method_div_duration_f64_a2(v:tests/time.rs::float_call_result::d,v:tests/time.rs::float_call_result::Duration)::assertion"
    );
    let operands = inv_operands(decl);
    assert_eq!(operands.len(), 1);
    assert_real_call_eq_atom(&operands[0], "method:div_duration_f64", "2.0");
}

#[test]
fn mixed_supported_and_refinement_assertions_lift_supported_rows() {
    let src = r#"
struct Duration;

impl Duration {
    fn div_duration_f64(&self, _other: Duration) -> f64 { 2.0 }
}

#[test]
fn float_mixed_refinement_gap() {
    let d = Duration;
    assert_eq!(d.div_duration_f64(Duration), 2.0);
    assert!(d.div_duration_f64(Duration).is_nan());
    assert!(!d.div_duration_f64(Duration).is_nan());
}
"#;
    let out = lift_file(&parse(src), "tests/time.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(
        out.warnings.len(),
        0,
        "width-known NaN refinements over method float results are liftable"
    );
    assert_eq!(out.decls.len(), 1);

    let decl = &out.decls[0];
    assert_eq!(
        decl.name,
        "method:div_duration_f64#euf#c:callresult_method_div_duration_f64_a2(v:tests/time.rs::float_mixed_refinement_gap::d,v:tests/time.rs::float_mixed_refinement_gap::Duration)::assertion"
    );
    let operands = inv_operands(decl);
    assert_eq!(operands.len(), 3);
    assert_real_call_eq_atom(&operands[0], "method:div_duration_f64", "2.0");
    assert_float_refinement_atom(&operands[1], "float.f64.is_nan", "method:div_duration_f64");
    match operands[2].as_ref() {
        Formula::Connective { kind, operands } if kind == "not" => {
            assert_eq!(operands.len(), 1);
            assert_float_refinement_atom(
                operands[0].as_ref(),
                "float.f64.is_nan",
                "method:div_duration_f64",
            );
        }
        other => panic!("expected negated float refinement atom, got {other:?}"),
    }
}

#[test]
fn exponent_float_literals_normalize_to_exact_real_constants() {
    let src = r#"
fn value() -> f64 { 0.001 }

#[test]
fn exponent_float_literal() {
    assert_eq!(value(), 1e-3);
    assert_eq!(value(), 12.50e+2);
}
"#;
    let out = lift_file(&parse(src), "tests/num/floats.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.warnings.len(), 0);
    assert_eq!(out.decls.len(), 1);

    let decl = &out.decls[0];
    assert_eq!(decl.name, "value#euf#c:callresult_value_a0()::assertion");
    let operands = inv_operands(decl);
    assert_eq!(operands.len(), 2);
    assert_real_call_eq_atom(&operands[0], "call:value", "0.001");
    assert_real_call_eq_atom(&operands[1], "call:value", "1250");
}

#[test]
fn width_known_infinite_refinement_lifts_as_predicate_atom() {
    let src = r#"
struct Duration;

impl Duration {
    fn div_duration_f32(&self, _other: Duration) -> f32 { f32::INFINITY }
}

#[test]
fn float_infinite_refinement() {
    let d = Duration;
    assert!(d.div_duration_f32(Duration).is_infinite());
}
"#;
    let out = lift_file(&parse(src), "tests/time.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.warnings.len(), 0);
    assert_eq!(out.decls.len(), 1);

    let decl = &out.decls[0];
    assert_eq!(
        decl.name,
        "method:div_duration_f32#euf#c:callresult_method_div_duration_f32_a2(v:tests/time.rs::float_infinite_refinement::d,v:tests/time.rs::float_infinite_refinement::Duration)::assertion"
    );
    let operands = inv_operands(decl);
    assert_eq!(operands.len(), 1);
    assert_float_refinement_atom(
        &operands[0],
        "float.f32.is_infinite",
        "method:div_duration_f32",
    );
}

#[test]
fn method_call_result_euf_keys_scope_local_receivers_to_avoid_false_collisions() {
    let src = r#"
struct Cursor;

impl Cursor {
    fn get_ref(&self) -> Vec<u8> { Vec::new() }
}

#[test]
fn first_local_cursor() {
    let c = Cursor;
    assert_eq!(c.get_ref().len(), 1);
}

#[test]
fn second_local_cursor() {
    let c = Cursor;
    assert_eq!(c.get_ref().len(), 2);
}
"#;
    let out = lift_file(&parse(src), "src/engine/tests.rs");
    assert_eq!(out.seen, 2);
    assert_eq!(out.lifted, 2, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 2);

    let names = out
        .decls
        .iter()
        .map(|decl| decl.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "method:len#euf#c:callresult_method_len_a1(c:method:get_ref(v:src/engine/tests.rs::first_local_cursor::c))::assertion",
            "method:len#euf#c:callresult_method_len_a1(c:method:get_ref(v:src/engine/tests.rs::second_local_cursor::c))::assertion",
        ]
    );
}

#[test]
fn method_chain_predicate_assertion_uses_euf_callsite_key() {
    let src = r#"
struct Layout;
struct ResultLike;

impl Layout {
    fn align_to(&self, _align: usize) -> ResultLike { ResultLike }
}

impl ResultLike {
    fn is_err(&self) -> bool { true }
}

#[test]
fn layout_errors() {
    let layout = Layout;
    assert!(layout.align_to(3).is_err());
}
"#;
    let out = lift_file(&parse(src), "tests/alloc.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);

    let decl = &out.decls[0];
    assert_eq!(
        decl.name,
        "method:is_err#euf#c:callresult_method_is_err_a1(c:method:align_to(v:tests/alloc.rs::layout_errors::layout,i:3))::assertion"
    );
    let operands = inv_operands(decl);
    assert_eq!(operands.len(), 1);
    match operands[0].as_ref() {
        Formula::Atomic { name, args } => {
            assert_eq!(name, "=");
            assert_eq!(args.len(), 2);
            match args[0].as_ref() {
                Term::Ctor { name, .. } => assert_eq!(name, "method:is_err"),
                other => panic!("expected method-chain predicate call, got {other:?}"),
            }
            match args[1].as_ref() {
                Term::Const {
                    value: ConstValue::Bool(value),
                    ..
                } => assert!(*value),
                other => panic!("expected bool true rhs, got {other:?}"),
            }
        }
        other => panic!("expected equality atom, got {other:?}"),
    }
}

#[test]
fn reassigned_receiver_versions_method_chain_subjects_to_avoid_false_coalescing() {
    let src = r#"
#[test]
fn range_rebinds() {
    let r = 1u32..5;
    assert!(!r.contains(&0));

    let r = 0u32..=u32::MAX;
    assert!(r.contains(&0));
}
"#;
    let out = lift_file(&parse(src), "tests/ops.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 2);

    let names = out
        .decls
        .iter()
        .map(|decl| decl.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "method:contains#euf#c:callresult_method_contains_a2(v:tests/ops.rs::range_rebinds::r@def1,c:ref(i:0))::assertion",
            "method:contains#euf#c:callresult_method_contains_a2(v:tests/ops.rs::range_rebinds::r@def2,c:ref(i:0))::assertion",
        ]
    );
}

#[test]
fn post_reassignment_claims_within_one_receiver_version_still_coalesce() {
    let src = r#"
#[test]
fn post_rebind_same_version() {
    let mut r = 1u32..5;
    r = 10u32..20;

    assert!(r.contains(&11));
    assert!(r.contains(&11));
}
"#;
    let out = lift_file(&parse(src), "tests/ops.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);
    assert_eq!(
        out.decls[0].name,
        "method:contains#euf#c:callresult_method_contains_a2(v:tests/ops.rs::post_rebind_same_version::r@def2,c:ref(i:11))::assertion"
    );
    let operands = inv_operands(&out.decls[0]);
    assert_eq!(operands.len(), 2);
}

#[test]
fn standalone_receiver_mutation_boundary_versions_later_method_chain_subject() {
    let src = r#"
#[test]
fn inclusive_range_after_next() {
    let mut r = 1u32..=1;
    r.next().unwrap();

    assert!(!r.contains(&1));
}
"#;
    let out = lift_file(&parse(src), "tests/ops.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);
    assert_eq!(
        out.decls[0].name,
        "method:contains#euf#c:callresult_method_contains_a2(v:tests/ops.rs::inclusive_range_after_next::r@def2,c:ref(i:1))::assertion"
    );
}

#[test]
fn conditional_receiver_reassignment_is_ambiguous_and_skipped() {
    let src = r#"
fn coin() -> bool { true }

#[test]
fn conditional_rebind() {
    let mut r = 1u32..5;
    if coin() {
        r = 10u32..20;
    }

    assert!(r.contains(&1));
}
"#;
    let out = lift_file(&parse(src), "tests/ops.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 0, "decls: {:?}", out.decls);
    assert!(out.decls.is_empty(), "decls: {:?}", out.decls);
    assert!(
        out.warnings.iter().any(|warning| {
            warning
                .reason
                .contains("ambiguous temporal identity for receiver `r`; skipped assertion")
        }),
        "warnings: {:?}",
        out.warnings
    );
}

#[test]
fn loop_receiver_mutation_is_ambiguous_and_skipped() {
    let src = r#"
#[test]
fn loop_rebind() {
    let mut r = 1u32..=1;
    for _ in 0..1 {
        r.next().unwrap();
    }

    assert!(r.contains(&1));
}
"#;
    let out = lift_file(&parse(src), "tests/ops.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 0, "decls: {:?}", out.decls);
    assert!(out.decls.is_empty(), "decls: {:?}", out.decls);
    assert!(
        out.warnings.iter().any(|warning| {
            warning
                .reason
                .contains("ambiguous temporal identity for receiver `r`; skipped assertion")
        }),
        "warnings: {:?}",
        out.warnings
    );
}

#[test]
fn alias_receiver_identity_is_ambiguous_and_skipped() {
    let src = r#"
#[test]
fn alias_rebind() {
    let mut r = 1u32..=1;
    let alias = &mut r;
    alias.next().unwrap();

    assert!(r.contains(&1));
}
"#;
    let out = lift_file(&parse(src), "tests/ops.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 0, "decls: {:?}", out.decls);
    assert!(out.decls.is_empty(), "decls: {:?}", out.decls);
    assert!(
        out.warnings.iter().any(|warning| {
            warning
                .reason
                .contains("ambiguous temporal identity for receiver `r`; skipped assertion")
        }),
        "warnings: {:?}",
        out.warnings
    );
}

#[test]
fn method_chain_predicate_range_contains_keys_bounds_and_reference_arg() {
    let src = r#"
#[test]
fn test_range_contains() {
    assert!(!(1u32..5).contains(&0u32));
    assert!((1u32..5).contains(&1u32));
}
"#;
    let out = lift_file(&parse(src), "tests/ops.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 2);

    let names = out
        .decls
        .iter()
        .map(|decl| decl.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "method:contains#euf#c:callresult_method_contains_a2(c:range(i:1,i:5),c:ref(i:0))::assertion",
            "method:contains#euf#c:callresult_method_contains_a2(c:range(i:1,i:5),c:ref(i:1))::assertion",
        ]
    );
    let first = inv_operands(&out.decls[0]);
    assert_eq!(first.len(), 1);
    match first[0].as_ref() {
        Formula::Atomic { name, args } => {
            assert_eq!(name, "=");
            assert_eq!(args.len(), 2);
            match args[1].as_ref() {
                Term::Const {
                    value: ConstValue::Bool(value),
                    ..
                } => assert!(!*value),
                other => panic!("expected bool false rhs, got {other:?}"),
            }
        }
        other => panic!("expected equality atom, got {other:?}"),
    }
}

#[test]
fn method_call_result_with_bitwise_rhs_uses_euf_callsite_key() {
    let src = r#"
struct AtomicLike;
struct SeqCst;

impl AtomicLike {
    fn load(&self, _ordering: SeqCst) -> usize { 0 }
}

#[test]
fn uint_and() {
    let x = AtomicLike;
    assert_eq!(x.load(SeqCst), 0xf731 & 0x137f);
}
"#;
    let out = lift_file(&parse(src), "tests/atomic.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);

    let decl = &out.decls[0];
    assert_eq!(
        decl.name,
        "method:load#euf#c:callresult_method_load_a2(v:tests/atomic.rs::uint_and::x,v:tests/atomic.rs::uint_and::SeqCst)::assertion"
    );
    let operands = inv_operands(decl);
    assert_eq!(operands.len(), 1);
    assert_method_call_eq_compound_rhs(&operands[0], "method:load", "bit-and", 0xf731, 0x137f);
}

#[test]
fn constructor_call_expected_value_stays_location_keyed() {
    let src = r#"
struct AtomicLike;
struct SeqCst;

impl AtomicLike {
    fn compare_exchange(
        &self,
        _current: bool,
        _new: bool,
        _success: SeqCst,
        _failure: SeqCst,
    ) -> Result<bool, bool> {
        Ok(false)
    }
}

#[test]
fn bool_compare_exchange() {
    let a = AtomicLike;
    assert_eq!(a.compare_exchange(false, true, SeqCst, SeqCst), Ok(false));
}
"#;
    let out = lift_file(&parse(src), "tests/atomic.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);
    assert_eq!(out.decls[0].name, "tests/atomic.rs::bool_compare_exchange");
    match inv_operands(&out.decls[0])[0].as_ref() {
        Formula::Atomic { name, args } => {
            assert_eq!(name, "=");
            assert_eq!(args.len(), 2);
            match args[0].as_ref() {
                Term::Ctor { name, args } => {
                    assert_eq!(name, "call:eq:Ok");
                    assert_eq!(args.len(), 2);
                    match args[0].as_ref() {
                        Term::Ctor { name, .. } => assert_eq!(name, "method:compare_exchange"),
                        other => panic!("expected compare_exchange lhs, got {other:?}"),
                    }
                    match args[1].as_ref() {
                        Term::Ctor { name, args } => {
                            assert_eq!(name, "call:Ok");
                            assert_eq!(args.len(), 1);
                            match args[0].as_ref() {
                                Term::Const {
                                    value: ConstValue::Bool(value),
                                    ..
                                } => assert!(!*value),
                                other => panic!("expected bool constructor arg, got {other:?}"),
                            }
                        }
                        other => panic!("expected Ok constructor rhs, got {other:?}"),
                    }
                }
                other => panic!("expected operator call lhs, got {other:?}"),
            }
            assert_scalar_const(&args[1], ExpectedScalar::Bool(true));
        }
        other => panic!("expected equality atom, got {other:?}"),
    }
}

#[test]
fn nullary_option_constructor_expected_value_uses_operator_dispatch() {
    // Vendor shape: rust-src library/coretests/tests/iter/range.rs::test_range_nth.
    // `None` is the nullary Option constructor, not a local variable. Keeping it
    // as a constructor lets the user-overridable equality dispatch stay
    // explicit and location-keyed instead of pretending this is scalar `=`.
    let src = r#"
#[test]
fn test_range_nth() {
    assert_eq!((10..15).nth(5), None);
}
"#;
    let out = lift_file(&parse(src), "tests/iter/range.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);
    assert_eq!(out.decls[0].name, "tests/iter/range.rs::test_range_nth");

    match inv_operands(&out.decls[0])[0].as_ref() {
        Formula::Atomic { name, args } => {
            assert_eq!(name, "=");
            assert_eq!(args.len(), 2);
            match args[0].as_ref() {
                Term::Ctor { name, args } => {
                    assert_eq!(name, "call:eq:None");
                    assert_eq!(args.len(), 2);
                    match args[0].as_ref() {
                        Term::Ctor { name, .. } => assert_eq!(name, "method:nth"),
                        other => panic!("expected nth lhs, got {other:?}"),
                    }
                    match args[1].as_ref() {
                        Term::Ctor { name, args } => {
                            assert_eq!(name, "call:None");
                            assert!(args.is_empty());
                        }
                        other => panic!("expected None constructor rhs, got {other:?}"),
                    }
                }
                other => panic!("expected operator call lhs, got {other:?}"),
            }
            assert_scalar_const(&args[1], ExpectedScalar::Bool(true));
        }
        other => panic!("expected equality atom, got {other:?}"),
    }
}

#[test]
fn option_test_and_constructor_rows_stay_location_keyed() {
    // Vendor shape: rust-src library/coretests/tests/option.rs::test_and.
    // The inputs are immutable Option values; the equality itself is still
    // Option::eq, so this is one location-keyed operator-dispatch contract.
    let src = r#"
use core::option::*;

#[test]
fn test_and() {
    let x: Option<isize> = Some(1);
    assert_eq!(x.and(Some(2)), Some(2));
    assert_eq!(x.and(None::<isize>), None);

    let x: Option<isize> = None;
    assert_eq!(x.and(Some(2)), None);
    assert_eq!(x.and(None::<isize>), None);

    const FOO: Option<isize> = Some(1);
    const A: Option<isize> = FOO.and(Some(2));
    const B: Option<isize> = FOO.and(None);
    assert_eq!(A, Some(2));
    assert_eq!(B, None);

    const BAR: Option<isize> = None;
    const C: Option<isize> = BAR.and(Some(2));
    const D: Option<isize> = BAR.and(None);
    assert_eq!(C, None);
    assert_eq!(D, None);
}
"#;
    let out = lift_file(&parse(src), "tests/option.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);
    assert_eq!(out.decls[0].name, "tests/option.rs::test_and");

    let operands = inv_operands(&out.decls[0]);
    assert_eq!(operands.len(), 8);
    for operand in operands {
        match operand.as_ref() {
            Formula::Atomic { name, args } => {
                assert_eq!(name, "=");
                assert_eq!(args.len(), 2);
                match args[0].as_ref() {
                    Term::Ctor { name, args } => {
                        assert!(
                            name == "call:eq:Some" || name == "call:eq:None",
                            "unexpected operator-dispatch call: {name}"
                        );
                        assert_eq!(args.len(), 2);
                    }
                    other => panic!("expected operator-dispatch lhs, got {other:?}"),
                }
                assert_scalar_const(&args[1], ExpectedScalar::Bool(true));
            }
            other => panic!("expected equality atom, got {other:?}"),
        }
    }
}

#[test]
fn constructor_operator_comparisons_lift_as_uninterpreted_operator_results() {
    let src = r#"
struct Int(i32);
struct RevInt(i32);
struct Fool(bool);

#[test]
fn cmp_default() {
    assert!(Int(2) > Int(1));
    assert!(Int(2) >= Int(1));
    assert!(Int(1) >= Int(1));
    assert!(Int(1) < Int(2));
    assert!(Int(1) <= Int(2));
    assert!(Int(1) <= Int(1));
    assert!(RevInt(2) < RevInt(1));
    assert!(RevInt(2) <= RevInt(1));
    assert!(RevInt(1) <= RevInt(1));
    assert!(RevInt(1) > RevInt(2));
    assert!(RevInt(1) >= RevInt(2));
    assert!(RevInt(1) >= RevInt(1));
    assert_eq!(Fool(true), Fool(false));
    assert!(Fool(true) != Fool(true));
    assert!(Fool(false) != Fool(false));
    assert_eq!(Fool(false), Fool(true));
}
"#;
    let out = lift_file(&parse(src), "tests/cmp.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);
    assert_eq!(out.decls[0].name, "tests/cmp.rs::cmp_default");

    let operands = inv_operands(&out.decls[0]);
    assert_eq!(operands.len(), 16);
    assert_operator_bool_atom(
        &operands[0],
        "call:gt:Int",
        &[
            ExpectedOperatorArg::Constructor("call:Int", ExpectedScalar::Int(2)),
            ExpectedOperatorArg::Constructor("call:Int", ExpectedScalar::Int(1)),
        ],
        true,
    );
    assert_operator_bool_atom(
        &operands[1],
        "call:ge:Int",
        &[
            ExpectedOperatorArg::Constructor("call:Int", ExpectedScalar::Int(2)),
            ExpectedOperatorArg::Constructor("call:Int", ExpectedScalar::Int(1)),
        ],
        true,
    );
    assert_operator_bool_atom(
        &operands[2],
        "call:ge:Int",
        &[
            ExpectedOperatorArg::Constructor("call:Int", ExpectedScalar::Int(1)),
            ExpectedOperatorArg::Constructor("call:Int", ExpectedScalar::Int(1)),
        ],
        true,
    );
    assert_operator_bool_atom(
        &operands[3],
        "call:lt:Int",
        &[
            ExpectedOperatorArg::Constructor("call:Int", ExpectedScalar::Int(1)),
            ExpectedOperatorArg::Constructor("call:Int", ExpectedScalar::Int(2)),
        ],
        true,
    );
    assert_operator_bool_atom(
        &operands[4],
        "call:le:Int",
        &[
            ExpectedOperatorArg::Constructor("call:Int", ExpectedScalar::Int(1)),
            ExpectedOperatorArg::Constructor("call:Int", ExpectedScalar::Int(2)),
        ],
        true,
    );
    assert_operator_bool_atom(
        &operands[5],
        "call:le:Int",
        &[
            ExpectedOperatorArg::Constructor("call:Int", ExpectedScalar::Int(1)),
            ExpectedOperatorArg::Constructor("call:Int", ExpectedScalar::Int(1)),
        ],
        true,
    );
    assert_operator_bool_atom(
        &operands[6],
        "call:lt:RevInt",
        &[
            ExpectedOperatorArg::Constructor("call:RevInt", ExpectedScalar::Int(2)),
            ExpectedOperatorArg::Constructor("call:RevInt", ExpectedScalar::Int(1)),
        ],
        true,
    );
    assert_operator_bool_atom(
        &operands[7],
        "call:le:RevInt",
        &[
            ExpectedOperatorArg::Constructor("call:RevInt", ExpectedScalar::Int(2)),
            ExpectedOperatorArg::Constructor("call:RevInt", ExpectedScalar::Int(1)),
        ],
        true,
    );
    assert_operator_bool_atom(
        &operands[8],
        "call:le:RevInt",
        &[
            ExpectedOperatorArg::Constructor("call:RevInt", ExpectedScalar::Int(1)),
            ExpectedOperatorArg::Constructor("call:RevInt", ExpectedScalar::Int(1)),
        ],
        true,
    );
    assert_operator_bool_atom(
        &operands[9],
        "call:gt:RevInt",
        &[
            ExpectedOperatorArg::Constructor("call:RevInt", ExpectedScalar::Int(1)),
            ExpectedOperatorArg::Constructor("call:RevInt", ExpectedScalar::Int(2)),
        ],
        true,
    );
    assert_operator_bool_atom(
        &operands[10],
        "call:ge:RevInt",
        &[
            ExpectedOperatorArg::Constructor("call:RevInt", ExpectedScalar::Int(1)),
            ExpectedOperatorArg::Constructor("call:RevInt", ExpectedScalar::Int(2)),
        ],
        true,
    );
    assert_operator_bool_atom(
        &operands[11],
        "call:ge:RevInt",
        &[
            ExpectedOperatorArg::Constructor("call:RevInt", ExpectedScalar::Int(1)),
            ExpectedOperatorArg::Constructor("call:RevInt", ExpectedScalar::Int(1)),
        ],
        true,
    );
    assert_operator_bool_atom(
        &operands[12],
        "call:eq:Fool",
        &[
            ExpectedOperatorArg::Constructor("call:Fool", ExpectedScalar::Bool(true)),
            ExpectedOperatorArg::Constructor("call:Fool", ExpectedScalar::Bool(false)),
        ],
        true,
    );
    assert_operator_bool_atom(
        &operands[13],
        "call:eq:Fool",
        &[
            ExpectedOperatorArg::Constructor("call:Fool", ExpectedScalar::Bool(true)),
            ExpectedOperatorArg::Constructor("call:Fool", ExpectedScalar::Bool(true)),
        ],
        false,
    );
    assert_operator_bool_atom(
        &operands[14],
        "call:eq:Fool",
        &[
            ExpectedOperatorArg::Constructor("call:Fool", ExpectedScalar::Bool(false)),
            ExpectedOperatorArg::Constructor("call:Fool", ExpectedScalar::Bool(false)),
        ],
        false,
    );
    assert_operator_bool_atom(
        &operands[15],
        "call:eq:Fool",
        &[
            ExpectedOperatorArg::Constructor("call:Fool", ExpectedScalar::Bool(false)),
            ExpectedOperatorArg::Constructor("call:Fool", ExpectedScalar::Bool(true)),
        ],
        true,
    );
}

#[test]
fn ordinary_call_result_rhs_does_not_become_a_ground_value() {
    let src = r#"
fn left() -> i32 { 1 }
fn right() -> i32 { 1 }

#[test]
fn two_calls() {
    assert_eq!(left(), right());
}
"#;
    let out = lift_file(&parse(src), "tests/calls.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);
    assert_eq!(out.decls[0].name, "tests/calls.rs::two_calls");
}

#[test]
fn literal_array_receiver_method_chain_gets_stable_euf_key() {
    // Vendor shape: rust-src library/coretests/tests/array.rs::iterator_last.
    let src = r#"
#[test]
fn iterator_last_literal_array() {
    assert_eq!(IntoIterator::into_iter([0]).last().unwrap(), 0);
}
"#;
    let out = lift_file(&parse(src), "tests/array.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert!(
        out.warnings.is_empty(),
        "literal array receiver should not leave a residual: {:?}",
        out.warnings
    );
    assert_eq!(out.decls.len(), 1);
    assert_eq!(
        out.decls[0].name,
        "method:unwrap#euf#c:callresult_method_unwrap_a1(c:method:last(c:call:IntoIterator::into_iter(v:literal:Array(i:0))))::assertion"
    );
    let operands = inv_operands(&out.decls[0]);
    assert_eq!(operands.len(), 1);
    match operands[0].as_ref() {
        Formula::Atomic { name, args } => {
            assert_eq!(name, "=");
            assert_eq!(args.len(), 2);
            match args[0].as_ref() {
                Term::Ctor { name, args } => {
                    assert_eq!(name, "method:unwrap");
                    assert_eq!(args.len(), 1);
                    match args[0].as_ref() {
                        Term::Ctor { name, args } => {
                            assert_eq!(name, "method:last");
                            assert_eq!(args.len(), 1);
                            match args[0].as_ref() {
                                Term::Ctor { name, args } => {
                                    assert_eq!(name, "call:IntoIterator::into_iter");
                                    assert_eq!(args.len(), 1);
                                    match args[0].as_ref() {
                                        Term::Var { name } => {
                                            assert_eq!(name, "literal:Array(i:0)");
                                        }
                                        other => {
                                            panic!("expected Array literal identity, got {other:?}")
                                        }
                                    }
                                }
                                other => {
                                    panic!("expected IntoIterator::into_iter term, got {other:?}")
                                }
                            }
                        }
                        other => panic!("expected last method receiver, got {other:?}"),
                    }
                }
                other => panic!("expected unwrap method term, got {other:?}"),
            }
            assert_scalar_const(&args[1], ExpectedScalar::Int(0));
        }
        other => panic!("expected equality atom, got {other:?}"),
    }
}

#[test]
fn tuple_expected_value_gets_stable_euf_key() {
    // Vendor shape: rust-src library/coretests/tests/iter/sources.rs::test_repeat_take.
    let src = r#"
#[test]
fn repeat_take_size_hint() {
    assert_eq!(repeat(42).take(3).size_hint(), (3, Some(3)));
}
"#;
    let out = lift_file(&parse(src), "tests/iter/sources.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert!(
        out.warnings.is_empty(),
        "tuple expected value should not leave a residual: {:?}",
        out.warnings
    );
    assert_eq!(out.decls.len(), 1);
    assert_eq!(
        out.decls[0].name,
        "method:size_hint#euf#c:callresult_method_size_hint_a1(c:method:take(c:call:repeat(i:42),i:3))::assertion"
    );
    let operands = inv_operands(&out.decls[0]);
    assert_eq!(operands.len(), 1);
    match operands[0].as_ref() {
        Formula::Atomic { name, args } => {
            assert_eq!(name, "=");
            assert_eq!(args.len(), 2);
            match args[0].as_ref() {
                Term::Ctor { name, args } => {
                    assert_eq!(name, "method:size_hint");
                    assert_eq!(args.len(), 1);
                }
                other => panic!("expected size_hint lhs, got {other:?}"),
            }
            match args[1].as_ref() {
                Term::Var { name } => {
                    assert_eq!(name, "literal:Tuple(i:3,c:call:Some(i:3))");
                }
                other => panic!("expected Tuple literal identity, got {other:?}"),
            }
        }
        other => panic!("expected equality atom, got {other:?}"),
    }
}

#[test]
fn const_block_wrapped_method_call_result_gets_stable_euf_key() {
    // Vendor shape: rust-src library/coretests/tests/array.rs::const_array_ops.
    let src = r#"
#[test]
fn const_array_ops() {
    const fn doubler(x: usize) -> usize {
        x * 2
    }
    assert_eq!(const { [5, 6, 1, 2].map(doubler) }, [10, 12, 2, 4]);
    assert_eq!(const { std::array::from_fn::<_, 5, _>(doubler) }, [0, 2, 4, 6, 8]);
}
"#;
    let out = lift_file(&parse(src), "tests/array.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert!(
        out.warnings.is_empty(),
        "expression-only const block should not leave a residual: {:?}",
        out.warnings
    );
    assert_eq!(out.decls.len(), 2);
    let names = out
        .decls
        .iter()
        .map(|decl| decl.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "method:map#euf#c:callresult_method_map_a2(v:literal:Array(i:5,i:6,i:1,i:2),v:tests/array.rs::const_array_ops::doubler)::assertion",
            "std::array::from_fn::<_,const:5,_>#euf#c:callresult_std__array__from_fn_____const_5____a1(v:tests/array.rs::const_array_ops::doubler)::assertion",
        ]
    );
    let operands = inv_operands(&out.decls[0]);
    assert_eq!(operands.len(), 1);
    match operands[0].as_ref() {
        Formula::Atomic { name, args } => {
            assert_eq!(name, "=");
            assert_eq!(args.len(), 2);
            match args[0].as_ref() {
                Term::Ctor { name, args } => {
                    assert_eq!(name, "method:map");
                    assert_eq!(args.len(), 2);
                }
                other => panic!("expected map method term, got {other:?}"),
            }
            match args[1].as_ref() {
                Term::Var { name } => {
                    assert_eq!(name, "literal:Array(i:10,i:12,i:2,i:4)");
                }
                other => panic!("expected Array literal identity, got {other:?}"),
            }
        }
        other => panic!("expected equality atom, got {other:?}"),
    }
}

#[test]
fn nested_const_block_arguments_stay_residual_until_keyed_deliberately() {
    let src = r#"
fn apply(_: fn(usize) -> usize) -> usize { 0 }

#[test]
fn nested_const_arg() {
    const fn doubler(x: usize) -> usize {
        x * 2
    }
    assert_eq!(apply(const { doubler }), 0);
}
"#;
    let out = lift_file(&parse(src), "tests/array.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 0);
    assert!(out.decls.is_empty());
    assert!(
        out.warnings.iter().any(|warning| warning
            .reason
            .contains("unsupported term `const { doubler }`")),
        "nested const block should stay residual, warnings: {:?}",
        out.warnings
    );
}

#[test]
fn immutable_index_in_pointer_eq_predicate_stays_location_keyed() {
    // Vendor shape: rust-src library/coretests/tests/array.rs::array_from_ref.
    // The index expression is pure identity syntax here, but pointer equality is
    // only claimed at the test locus. Cross-proof pointer identity is not a
    // federated call-result key.
    let src = r#"
#[test]
fn array_from_ref() {
    const VALUE: &&str = &"Hello World!";
    const ARR: &[&str; 1] = core::array::from_ref(VALUE);
    assert!(core::ptr::eq(VALUE, &ARR[0]));
}
"#;
    let out = lift_file(&parse(src), "tests/array.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert!(
        out.warnings.is_empty(),
        "immutable index inside pointer equality should lift: {:?}",
        out.warnings
    );
    assert_eq!(out.decls.len(), 1);
    assert_eq!(out.decls[0].name, "tests/array.rs::array_from_ref");

    let operands = inv_operands(&out.decls[0]);
    assert_eq!(operands.len(), 1);
    match operands[0].as_ref() {
        Formula::Atomic { name, args } => {
            assert_eq!(name, "=");
            assert_eq!(args.len(), 2);
            match args[0].as_ref() {
                Term::Ctor { name, args } => {
                    assert_eq!(name, "call:core::ptr::eq");
                    assert_eq!(args.len(), 2);
                    match args[1].as_ref() {
                        Term::Ctor { name, args } => {
                            assert_eq!(name, "ref");
                            assert_eq!(args.len(), 1);
                            match args[0].as_ref() {
                                Term::Ctor { name, args } => {
                                    assert_eq!(name, "index");
                                    assert_eq!(args.len(), 2);
                                }
                                other => panic!("expected index term, got {other:?}"),
                            }
                        }
                        other => panic!("expected ref indexed argument, got {other:?}"),
                    }
                }
                other => panic!("expected ptr::eq call lhs, got {other:?}"),
            }
            assert_scalar_const(&args[1], ExpectedScalar::Bool(true));
        }
        other => panic!("expected pointer equality atom, got {other:?}"),
    }
}

#[test]
fn non_pointer_index_equality_stays_residual() {
    let src = r#"
#[test]
fn indexed_value() {
    let xs = [1, 2, 3];
    assert_eq!(xs[0], 1);
}
"#;
    let out = lift_file(&parse(src), "tests/index.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 0);
    assert!(out.decls.is_empty());
    assert!(
        out.warnings
            .iter()
            .any(|warning| warning.reason.contains("unsupported term `xs [0]`")),
        "indexed equality outside pointer identity stays residual: {:?}",
        out.warnings
    );
}

#[test]
fn vendor_string_predicates_lift_to_string_theory_atoms_under_euf_keys() {
    // Vendor source: rust-src library/alloctests/tests/str.rs contains these
    // point-wise assertions in test_starts_with, test_ends_with, and contains.
    let src = r#"
#[test]
fn test_starts_with() {
    assert!("abc".starts_with("a"));
    assert!(!"a".starts_with("abc"));
}

#[test]
fn test_ends_with() {
    assert!("abc".ends_with("c"));
}

#[test]
fn contains() {
    assert!("abcde".contains("bcd"));
    assert!(!"abcde".contains("def"));
    assert!("abc".contains('b'));
}
"#;
    let out = lift_file(&parse(src), "tests/str.rs");
    assert_eq!(out.seen, 3);
    assert_eq!(out.lifted, 3, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 6);

    let names = out
        .decls
        .iter()
        .map(|decl| decl.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "method:starts_with#euf#c:callresult_method_starts_with_a2(s:\"abc\",s:\"a\")::assertion",
            "method:starts_with#euf#c:callresult_method_starts_with_a2(s:\"a\",s:\"abc\")::assertion",
            "method:ends_with#euf#c:callresult_method_ends_with_a2(s:\"abc\",s:\"c\")::assertion",
            "method:contains#euf#c:callresult_method_contains_a2(s:\"abcde\",s:\"bcd\")::assertion",
            "method:contains#euf#c:callresult_method_contains_a2(s:\"abcde\",s:\"def\")::assertion",
            "method:contains#euf#c:callresult_method_contains_a2(s:\"abc\",s:\"b\")::assertion",
        ]
    );

    assert_string_predicate_atom(&inv_operands(&out.decls[0])[0], "prefix-of", &["a", "abc"]);
    match inv_operands(&out.decls[1])[0].as_ref() {
        Formula::Connective { kind, operands } => {
            assert_eq!(kind, "not");
            assert_eq!(operands.len(), 1);
            assert_string_predicate_atom(&operands[0], "prefix-of", &["abc", "a"]);
        }
        other => panic!("expected negated prefix atom, got {other:?}"),
    }
    assert_string_predicate_atom(&inv_operands(&out.decls[2])[0], "suffix-of", &["c", "abc"]);
    assert_string_predicate_atom(
        &inv_operands(&out.decls[3])[0],
        "contains",
        &["abcde", "bcd"],
    );
    match inv_operands(&out.decls[4])[0].as_ref() {
        Formula::Connective { kind, operands } => {
            assert_eq!(kind, "not");
            assert_eq!(operands.len(), 1);
            assert_string_predicate_atom(&operands[0], "contains", &["abcde", "def"]);
        }
        other => panic!("expected negated contains atom, got {other:?}"),
    }
    assert_string_predicate_atom(&inv_operands(&out.decls[5])[0], "contains", &["abc", "b"]);
}

#[test]
fn vendor_ascii_and_len_predicates_lift_conservatively() {
    // Vendor source: rust-src library/coretests/tests/ascii.rs::test_is_ascii
    // and library/alloctests/tests/str.rs:157.
    let src = r#"
#[test]
fn test_is_ascii() {
    assert!("".is_ascii());
    assert!("banana\0\u{7F}".is_ascii());
}

#[test]
fn test_len() {
    assert_eq!("～～～～～".len(), 15);
}
"#;
    let out = lift_file(&parse(src), "tests/ascii.rs");
    assert_eq!(out.seen, 2);
    assert_eq!(out.lifted, 2, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 3);

    assert_eq!(
        out.decls[0].name,
        "method:is_ascii#euf#c:callresult_method_is_ascii_a1(s:\"\")::assertion"
    );
    assert_string_predicate_atom(&inv_operands(&out.decls[0])[0], "str.is_ascii", &[""]);
    assert_eq!(
        out.decls[1].name,
        "method:is_ascii#euf#c:callresult_method_is_ascii_a1(s:\"banana\\0\\u{7f}\")::assertion"
    );
    assert_string_predicate_atom(
        &inv_operands(&out.decls[1])[0],
        "str.is_ascii",
        &["banana\0\u{7F}"],
    );
    assert_eq!(
        out.decls[2].name,
        "method:len#euf#c:callresult_method_len_a1(s:\"～～～～～\")::assertion"
    );
    assert_string_len_cmp_atom(&inv_operands(&out.decls[2])[0], "=", "～～～～～", 15);
}

#[test]
fn vendor_char_ascii_class_lifts_and_unicode_alphabetic_stays_residual() {
    // Vendor source: rust-src library/core/src/char/methods.rs doc examples.
    let src = r#"
#[test]
fn char_ascii_classes() {
    assert!('A'.is_ascii_alphabetic());
    assert!(!'0'.is_ascii_alphabetic());
    assert!('a'.is_ascii());
    assert!('a'.is_alphabetic());
    assert!('0'.is_ascii_digit());
    assert!('f'.is_ascii_hexdigit());
    assert!('z'.is_ascii_lowercase());
    assert!('Z'.is_ascii_uppercase());
    assert!(' '.is_ascii_whitespace());
}
"#;
    let out = lift_file(&parse(src), "core/src/char/methods.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 8);
    assert_eq!(out.warnings.len(), 1);
    assert!(out.warnings[0]
        .reason
        .contains("unicode char predicate is_alphabetic is not lifted"));

    assert_eq!(
        out.decls[0].name,
        "method:is_ascii_alphabetic#euf#c:callresult_method_is_ascii_alphabetic_a1(s:\"A\")::assertion"
    );
    assert_string_predicate_atom(
        &inv_operands(&out.decls[0])[0],
        "str.is_ascii_alphabetic",
        &["A"],
    );
    match inv_operands(&out.decls[1])[0].as_ref() {
        Formula::Connective { kind, operands } => {
            assert_eq!(kind, "not");
            assert_eq!(operands.len(), 1);
            assert_string_predicate_atom(&operands[0], "str.is_ascii_alphabetic", &["0"]);
        }
        other => panic!("expected negated ascii alphabetic atom, got {other:?}"),
    }
    assert_string_predicate_atom(&inv_operands(&out.decls[2])[0], "str.is_ascii", &["a"]);
    assert_string_predicate_atom(
        &inv_operands(&out.decls[3])[0],
        "str.is_ascii_digit",
        &["0"],
    );
    assert_string_predicate_atom(
        &inv_operands(&out.decls[4])[0],
        "str.is_ascii_hexdigit",
        &["f"],
    );
    assert_string_predicate_atom(
        &inv_operands(&out.decls[5])[0],
        "str.is_ascii_lowercase",
        &["z"],
    );
    assert_string_predicate_atom(
        &inv_operands(&out.decls[6])[0],
        "str.is_ascii_uppercase",
        &["Z"],
    );
    assert_string_predicate_atom(
        &inv_operands(&out.decls[7])[0],
        "str.is_ascii_whitespace",
        &[" "],
    );
}

#[test]
fn vendor_literal_iterator_all_any_and_byte_slices_lift_soundly() {
    // Vendor source: rust-src library/coretests/tests/ascii.rs::test_is_ascii.
    let src = r#"
#[test]
fn test_is_ascii() {
    assert!(b"".is_ascii());
    assert!(b"banana\0\x7F".is_ascii());
    assert!(b"banana\0\x7F".iter().all(|b| b.is_ascii()));
    assert!(!b"Vi\xe1\xbb\x87t Nam".is_ascii());
    assert!(!b"Vi\xe1\xbb\x87t Nam".iter().all(|b| b.is_ascii()));
    assert!(!b"\xe1\xbb\x87".iter().any(|b| b.is_ascii()));
    assert!("".is_ascii());
    assert!("banana\0\x7F".is_ascii());
    assert!("banana\0\x7F".chars().all(|c| c.is_ascii()));
    assert!(!"ประเทศไทย中华Việt Nam".chars().all(|c| c.is_ascii()));
    assert!(!"ประเทศไทย中华ệ ".chars().any(|c| c.is_ascii()));
}
"#;
    let out = lift_file(&parse(src), "coretests/tests/ascii.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert!(
        out.warnings.is_empty(),
        "unexpected lift warnings: {:?}",
        out.warnings
    );
    assert_eq!(
        out.decls.len(),
        3,
        "expected two direct rows and one iterator/byte row"
    );
    let named_ascii = out
        .decls
        .iter()
        .filter(|decl| decl.name.starts_with("method:is_ascii#"))
        .count();
    assert_eq!(named_ascii, 2, "expected two direct string is_ascii rows");
    assert!(
        out.decls.iter().any(|decl| {
            decl.name == "coretests/tests/ascii.rs::test_is_ascii"
                && decl
                    .inv
                    .as_deref()
                    .is_some_and(|inv| formula_count_atomic_name(inv, "str.is_ascii") >= 10)
        }),
        "expected unrolled iterator/byte row in {:?}",
        out.decls
    );
    assert!(
        out.decls.iter().any(|decl| {
            decl.inv
                .as_deref()
                .is_some_and(|inv| formula_contains_relation_name(inv, "≥"))
        }),
        "expected byte iterator to lower to arithmetic range checks in {:?}",
        out.decls
    );
}

#[test]
fn vendor_assert_all_and_assert_none_macros_expand_to_bounded_claims() {
    // Vendor macro shape: rust-src library/coretests/tests/ascii.rs.
    let src = r#"
#[test]
fn test_is_ascii_alphabetic() {
    assert_all!(is_ascii_alphabetic, "Az",);
    assert_none!(is_ascii_digit, "aZ",);
}
"#;
    let out = lift_file(&parse(src), "coretests/tests/ascii.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert!(
        out.warnings.is_empty(),
        "unexpected lift warnings: {:?}",
        out.warnings
    );
    assert_eq!(out.decls.len(), 1);
    assert_eq!(
        out.decls[0].name,
        "coretests/tests/ascii.rs::test_is_ascii_alphabetic"
    );

    let operands = inv_operands(&out.decls[0]);
    assert_eq!(operands.len(), 8);
    let inv = out.decls[0].inv.as_deref().expect("macro row has inv");
    assert_eq!(formula_count_atomic_name(inv, "str.is_ascii_alphabetic"), 2);
    assert_eq!(formula_count_atomic_name(inv, "str.is_ascii_digit"), 2);
    assert!(
        formula_count_atomic_name(inv, "\u{2265}") >= 4,
        "expected byte predicates to lower to arithmetic ranges: {inv:?}"
    );
    assert!(
        formula_count_connective_kind(inv, "not") >= 4,
        "assert_none! must negate each bounded element: {inv:?}"
    );
}

#[test]
fn assertion_macros_refuse_dynamic_sources_and_unicode_predicates() {
    let src = r#"
#[test]
fn dynamic_or_unicode_macro_sources() {
    let dynamic = "123";
    assert_all!(is_ascii_digit, dynamic);
    assert_all!(is_alphabetic, "A");
}
"#;
    let out = lift_file(&parse(src), "coretests/tests/ascii.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 0);
    assert_eq!(out.decls.len(), 0);
    assert_eq!(out.warnings.len(), 2, "warnings: {:?}", out.warnings);
    assert!(out.warnings[0]
        .reason
        .contains("assert_all!: expected string literal source"));
    assert!(out.warnings[0]
        .reason
        .contains("unicode char predicate is_alphabetic is not lifted"));
    assert!(out.warnings[1]
        .reason
        .contains("no liftable scalar assertions"));
}

#[test]
fn call_result_comparison_assertions_use_fol_atoms_and_euf_key() {
    let src = r#"
fn value() -> i32 { 6 }

#[test]
fn comparison_atoms() {
    assert!(value() > 3);
    assert!(value() <= 9);
    assert!(value() != 7);
}
"#;
    let out = lift_file(&parse(src), "tests/compare.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);

    let decl = &out.decls[0];
    assert_eq!(decl.name, "value#euf#c:callresult_value_a0()::assertion");
    let operands = inv_operands(decl);
    assert_eq!(operands.len(), 3);
    assert_int_call_cmp_atom(&operands[0], ">", "call:value", 3);
    assert_int_call_cmp_atom(&operands[1], "\u{2264}", "call:value", 9);
    assert_int_call_cmp_atom(&operands[2], "\u{2260}", "call:value", 7);
}

#[test]
fn same_callsite_connectives_lift_as_fol_connectives_under_euf_key() {
    let src = r#"
fn value() -> i32 { 6 }

#[test]
fn connective_atoms() {
    assert!(value() > 3 && value() < 9);
    assert!(value() < 3 || value() > 5);
}
"#;
    let out = lift_file(&parse(src), "tests/compare.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);

    let decl = &out.decls[0];
    assert_eq!(decl.name, "value#euf#c:callresult_value_a0()::assertion");
    let operands = inv_operands(decl);
    assert_eq!(operands.len(), 2);
    match operands[0].as_ref() {
        Formula::Connective { kind, operands } => {
            assert_eq!(kind, "and");
            assert_eq!(operands.len(), 2);
            assert_int_call_cmp_atom(&operands[0], ">", "call:value", 3);
            assert_int_call_cmp_atom(&operands[1], "<", "call:value", 9);
        }
        other => panic!("expected and connective, got {other:?}"),
    }
    match operands[1].as_ref() {
        Formula::Connective { kind, operands } => {
            assert_eq!(kind, "or");
            assert_eq!(operands.len(), 2);
            assert_int_call_cmp_atom(&operands[0], "<", "call:value", 3);
            assert_int_call_cmp_atom(&operands[1], ">", "call:value", 5);
        }
        other => panic!("expected or connective, got {other:?}"),
    }
}

#[test]
fn negated_call_result_comparison_lifts_as_fol_not_under_euf_key() {
    let src = r#"
fn value() -> i32 { 6 }

#[test]
fn negated_comparison() {
    assert!(value() >= 3);
    assert!(!(value() < 3));
}
"#;
    let out = lift_file(&parse(src), "tests/compare.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);

    let decl = &out.decls[0];
    assert_eq!(decl.name, "value#euf#c:callresult_value_a0()::assertion");
    let operands = inv_operands(decl);
    assert_eq!(operands.len(), 2);
    assert_int_call_cmp_atom(&operands[0], "\u{2265}", "call:value", 3);
    match operands[1].as_ref() {
        Formula::Connective { kind, operands } => {
            assert_eq!(kind, "not");
            assert_eq!(operands.len(), 1);
            assert_int_call_cmp_atom(&operands[0], "<", "call:value", 3);
        }
        other => panic!("expected not connective, got {other:?}"),
    }
}

#[test]
fn non_call_assertions_stay_location_keyed() {
    let src = r#"
#[test]
fn scalar_var_is_six() {
    let value = 6;
    assert_eq!(value, 6);
}
"#;
    let out = lift_file(&parse(src), "src/lib.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);
    assert_eq!(out.decls[0].name, "src/lib.rs::scalar_var_is_six");
}

#[test]
fn lifts_tokio_async_test_assertion_across_await_boundary() {
    let src = r#"
async fn make_value() -> i32 { 6 }

#[tokio::test]
async fn async_scalar_is_six() {
    assert_eq!(make_value().await, 6);
}
"#;
    let out = lift_file(&parse(src), "src/lib.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);

    let decl = &out.decls[0];
    assert_eq!(decl.name, "src/lib.rs::async_scalar_is_six");
    let operands = inv_operands(decl);
    assert_eq!(operands.len(), 1);
    assert_await_call_eq_atom(&operands[0], "call:make_value", 6);
}

#[test]
fn conjoins_contradictory_tokio_async_assertions_across_same_await_shape() {
    let src = r#"
async fn make_value() -> i32 { 6 }

#[tokio::test]
async fn async_scalar_contradiction() {
    assert_eq!(make_value().await, 6);
    assert_eq!(make_value().await, 7);
}
"#;
    let out = lift_file(&parse(src), "tests/tokio_contradiction.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);

    let decl = &out.decls[0];
    assert_eq!(
        decl.name,
        "tests/tokio_contradiction.rs::async_scalar_contradiction"
    );
    let operands = inv_operands(decl);
    assert_eq!(operands.len(), 2);
    assert_await_call_eq_atom(&operands[0], "call:make_value", 6);
    assert_await_call_eq_atom(&operands[1], "call:make_value", 7);
}

#[test]
fn await_effect_is_syntax_derived_not_tokio_name_derived() {
    let src = r#"
async fn make_value() -> i32 { 6 }

#[demo_runtime::test]
async fn async_scalar_is_six() {
    assert_eq!(make_value().await, 6);
}
"#;
    let out = lift_file(&parse(src), "src/lib.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    let operands = inv_operands(&out.decls[0]);
    assert_eq!(operands.len(), 1);
    assert_await_call_eq_atom(&operands[0], "call:make_value", 6);
}
