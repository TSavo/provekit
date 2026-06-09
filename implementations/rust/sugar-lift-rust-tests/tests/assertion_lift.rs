use sugar_ir_symbolic::{ConstValue, Formula, Term};
use sugar_lift_rust_tests::lift_file;

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

fn formula_contains_atomic_name(formula: &Formula, expected_name: &str) -> bool {
    match formula {
        Formula::Atomic { name, .. } => name == expected_name,
        Formula::Connective { operands, .. } => {
            operands
                .iter()
                .any(|operand| formula_contains_atomic_name(operand, expected_name))
        }
        Formula::Quantifier { body, .. } => {
            formula_contains_atomic_name(body, expected_name)
        }
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
}
"#;
    let out = lift_file(&parse(src), "tests/time.rs");
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(
        out.warnings.len(),
        1,
        "unsupported refinement assertion should stay loud"
    );
    assert_eq!(out.decls.len(), 1);

    let decl = &out.decls[0];
    assert_eq!(
        decl.name,
        "method:div_duration_f64#euf#c:callresult_method_div_duration_f64_a2(v:tests/time.rs::float_mixed_refinement_gap::d,v:tests/time.rs::float_mixed_refinement_gap::Duration)::assertion"
    );
    let operands = inv_operands(decl);
    assert_eq!(operands.len(), 1);
    assert_real_call_eq_atom(&operands[0], "method:div_duration_f64", "2.0");
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
    assert_string_predicate_atom(&inv_operands(&out.decls[3])[0], "str.is_ascii_digit", &["0"]);
    assert_string_predicate_atom(&inv_operands(&out.decls[4])[0], "str.is_ascii_hexdigit", &["f"]);
    assert_string_predicate_atom(&inv_operands(&out.decls[5])[0], "str.is_ascii_lowercase", &["z"]);
    assert_string_predicate_atom(&inv_operands(&out.decls[6])[0], "str.is_ascii_uppercase", &["Z"]);
    assert_string_predicate_atom(&inv_operands(&out.decls[7])[0], "str.is_ascii_whitespace", &[" "]);
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
    assert!(out.warnings.is_empty(), "unexpected lift warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 3, "expected two direct rows and one iterator/byte row");
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
