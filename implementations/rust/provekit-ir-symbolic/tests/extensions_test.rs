//! Extension authoring + primitive-bridge registry tests for the Rust kit.
//! Mirrors the TS kit's extensions.test.ts shape so cross-kit conformance
//! reads the same tests at the same time.

use provekit_ir_symbolic::extensions::{
    extension_ctor, extension_predicate, extension_sort, list_bridges, list_extensions,
    lookup_bridge, lookup_extension, primitive_bridge, register_extension_declaration,
    ExtensionCtorInput, ExtensionDeclaration, ExtensionPredicateInput, ExtensionSortInput,
    PrimitiveBridgeInput, SemanticDeclaration, SortRef, _reset_registry,
};
use provekit_ir_symbolic::types::{sorts, IrTerm, Sort};
use std::sync::{Mutex, MutexGuard, OnceLock};

// Cargo runs tests in parallel by default; the extension registry is
// process-global. Tests serialize via this mutex so each test gets a
// clean registry for its body. The guard auto-releases at scope exit.
fn acquire_test_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn fresh() {
    _reset_registry();
}

#[test]
fn extension_sort_returns_a_named_sort_value() {
    let _guard = acquire_test_lock();
    fresh();
    let fp8 = extension_sort(ExtensionSortInput {
        name: "FixedPoint8".to_string(),
        params: vec![],
        semantics: vec![SemanticDeclaration::SmtLibTheory {
            theory: "FixedSizeBitVectors".to_string(),
            version: None,
        }],
        compilers: vec!["smt-lib".to_string()],
    });
    match fp8 {
        Sort::Primitive { name } => assert_eq!(name, "FixedPoint8"),
        _ => panic!("expected primitive sort"),
    }
}

#[test]
fn extension_sort_registers_in_the_registry() {
    let _guard = acquire_test_lock();
    fresh();
    extension_sort(ExtensionSortInput {
        name: "FixedPoint8".to_string(),
        params: vec![],
        semantics: vec![SemanticDeclaration::SmtLibTheory {
            theory: "FixedSizeBitVectors".to_string(),
            version: None,
        }],
        compilers: vec!["smt-lib".to_string()],
    });
    let decl = lookup_extension("FixedPoint8");
    assert!(decl.is_some());
    match decl.unwrap() {
        ExtensionDeclaration::Sort { name, compilers, .. } => {
            assert_eq!(name, "FixedPoint8");
            assert_eq!(compilers, vec!["smt-lib".to_string()]);
        }
        _ => panic!("expected sort declaration"),
    }
}

#[test]
fn primitive_bridge_returns_a_callable_that_emits_ctor_terms() {
    let _guard = acquire_test_lock();
    fresh();
    let parse_int = primitive_bridge(PrimitiveBridgeInput {
        ir_name: "parseInt".to_string(),
        ir_arg_sorts: vec![SortRef::Named("String".to_string())],
        ir_return_sort: SortRef::Named("Int".to_string()),
        source_layer: "rust-kit".to_string(),
        target_contract_cid: "bafy_LLVM_PARSEINT".to_string(),
        target_layer: "llvm".to_string(),
        notes: None,
    });
    let arg = IrTerm::Const {
        value: serde_json::json!("42"),
        sort: sorts::string(),
    };
    let term = parse_int.call(vec![arg]);
    match term {
        IrTerm::Ctor { name, sort, .. } => {
            assert_eq!(name, "parseInt");
            assert_eq!(sort, sorts::int());
        }
        _ => panic!("expected ctor term"),
    }
}

#[test]
fn primitive_bridge_registers_in_the_bridge_registry() {
    let _guard = acquire_test_lock();
    fresh();
    primitive_bridge(PrimitiveBridgeInput {
        ir_name: "abs".to_string(),
        ir_arg_sorts: vec![SortRef::Named("Int".to_string())],
        ir_return_sort: SortRef::Named("Int".to_string()),
        source_layer: "rust-kit".to_string(),
        target_contract_cid: "bafy_LLVM_ABS".to_string(),
        target_layer: "llvm".to_string(),
        notes: Some("LLVM intrinsic".to_string()),
    });
    let bridge = lookup_bridge("abs").expect("bridge should be registered");
    assert_eq!(bridge.target_contract_cid, "bafy_LLVM_ABS");
    assert_eq!(bridge.target_layer, "llvm");
    assert_eq!(bridge.notes, Some("LLVM intrinsic".to_string()));
}

#[test]
fn dogfood_authors_a_full_extension_set_and_uses_them() {
    let _guard = acquire_test_lock();
    fresh();
    // Author a sort.
    let fp8 = extension_sort(ExtensionSortInput {
        name: "FixedPoint8".to_string(),
        params: vec![],
        semantics: vec![SemanticDeclaration::SmtLibTheory {
            theory: "FixedSizeBitVectors".to_string(),
            version: None,
        }],
        compilers: vec!["smt-lib".to_string()],
    });

    // Author a multiplication ctor over it.
    let fp_mul = extension_ctor(ExtensionCtorInput {
        name: "fp8-mul".to_string(),
        arg_sorts: vec![SortRef::Sort(fp8.clone()), SortRef::Sort(fp8.clone())],
        return_sort: SortRef::Sort(fp8.clone()),
        semantics: vec![SemanticDeclaration::SmtLibTheory {
            theory: "FixedSizeBitVectors".to_string(),
            version: None,
        }],
        compilers: vec!["smt-lib".to_string()],
    });

    // Author an equality predicate over it.
    let _fp_eq = extension_predicate(ExtensionPredicateInput {
        name: "fp8-eq".to_string(),
        arg_sorts: vec![SortRef::Sort(fp8.clone()), SortRef::Sort(fp8.clone())],
        semantics: vec![SemanticDeclaration::SmtLibTheory {
            theory: "FixedSizeBitVectors".to_string(),
            version: None,
        }],
        compilers: vec!["smt-lib".to_string()],
    });

    // Use the ctor in an IR term.
    let a = IrTerm::Var {
        name: "a".to_string(),
        sort: fp8.clone(),
    };
    let b = IrTerm::Var {
        name: "b".to_string(),
        sort: fp8.clone(),
    };
    let term = fp_mul.call(vec![a, b]);
    match term {
        IrTerm::Ctor { name, sort, .. } => {
            assert_eq!(name, "fp8-mul");
            assert_eq!(sort, fp8);
        }
        _ => panic!("expected ctor term"),
    }

    // All three declarations live in the registry.
    assert_eq!(list_extensions().len(), 3);
}

#[test]
fn registry_collision_on_different_body_returns_error() {
    let _guard = acquire_test_lock();
    fresh();
    extension_sort(ExtensionSortInput {
        name: "FixedPoint8".to_string(),
        params: vec![],
        semantics: vec![SemanticDeclaration::SmtLibTheory {
            theory: "FixedSizeBitVectors".to_string(),
            version: None,
        }],
        compilers: vec!["smt-lib".to_string()],
    });

    let conflicting = ExtensionDeclaration::Sort {
        name: "FixedPoint8".to_string(),
        params: vec![],
        semantics: vec![SemanticDeclaration::NaturalLanguage {
            text: "different body".to_string(),
        }],
        compilers: vec!["smt-lib".to_string()],
    };

    let result = register_extension_declaration(conflicting);
    assert!(result.is_err());
}

#[test]
fn list_bridges_returns_all_registered_bridges() {
    let _guard = acquire_test_lock();
    fresh();
    primitive_bridge(PrimitiveBridgeInput {
        ir_name: "parseInt".to_string(),
        ir_arg_sorts: vec![SortRef::Named("String".to_string())],
        ir_return_sort: SortRef::Named("Int".to_string()),
        source_layer: "rust-kit".to_string(),
        target_contract_cid: "cid1".to_string(),
        target_layer: "llvm".to_string(),
        notes: None,
    });
    primitive_bridge(PrimitiveBridgeInput {
        ir_name: "abs".to_string(),
        ir_arg_sorts: vec![SortRef::Named("Int".to_string())],
        ir_return_sort: SortRef::Named("Int".to_string()),
        source_layer: "rust-kit".to_string(),
        target_contract_cid: "cid2".to_string(),
        target_layer: "llvm".to_string(),
        notes: None,
    });
    let bridges = list_bridges();
    assert_eq!(bridges.len(), 2);
}
