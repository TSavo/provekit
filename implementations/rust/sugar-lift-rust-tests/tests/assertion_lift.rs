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
    assert_eq!(decl.name, "src/lib.rs::scalar_is_six");
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
    assert_eq!(decl.name, "tests/contradiction.rs::scalar_contradiction");
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
    let operands = inv_operands(&out.decls[0]);
    assert_eq!(operands.len(), 1);
    assert_eq_atom(&operands[0], 6);
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
