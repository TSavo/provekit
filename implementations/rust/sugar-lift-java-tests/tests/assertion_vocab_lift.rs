use std::fs;

use sugar_ir_symbolic::{ConstValue, Formula, Term};
use sugar_lift_java_tests::{
    derive_vocab_from_source, learn_vocab_from_exception_dirs, lift_source_with_vocab,
    AssertCategory,
};

const ASSERT_LIB: &str = r#"
package demo.assertions;

public final class LearnedAssertions {
    public static void assertSameValue(int expected, int actual) {
        if (expected != actual) {
            throw new AssertionError("not equal");
        }
    }
    public static void assertEquals(int expected, int actual) {
        record(expected, actual);
    }
    public static void assertEquals(double expected, double actual, double delta) {
        if (Math.abs(expected - actual) > delta) {
            throw new AssertionError("not close");
        }
    }
    public static void assertTruth(boolean condition) {
        if (!condition) {
            throw new AssertionError("not true");
        }
    }
    public static void assertMystery(int expected, int actual, Object mode) {}
    public static void assertOpaque(int expected, int actual) {}
    private static void record(Object expected, Object actual) {}
}
"#;

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

#[test]
fn derive_vocab_splits_exact_approx_and_other_from_signatures() {
    let vocab =
        derive_vocab_from_source("demo.assertions.LearnedAssertions", ASSERT_LIB, &[]).unwrap();
    eprintln!("derived-vocab: {}", vocab.dump_lines().join("; "));

    let exact = vocab
        .classify_call(
            "assertSameValue",
            2,
            &["6".to_string(), "makeValue()".to_string()],
        )
        .expect("exact overload is learned");
    assert_eq!(exact.category, AssertCategory::Equality);

    let opaque_name = vocab
        .classify_call(
            "assertEquals",
            2,
            &["6".to_string(), "makeValue()".to_string()],
        )
        .expect("opaque overload is learned");
    assert_eq!(opaque_name.category, AssertCategory::Other);

    let approx = vocab
        .classify_call(
            "assertEquals",
            3,
            &[
                "6.0".to_string(),
                "makeValue()".to_string(),
                "0.01".to_string(),
            ],
        )
        .expect("delta overload is learned");
    assert_eq!(approx.category, AssertCategory::Approx);

    let truth = vocab
        .classify_call("assertTruth", 1, &["makeValue() == 6".to_string()])
        .expect("truth overload is learned");
    assert_eq!(truth.category, AssertCategory::Truth);

    let other = vocab
        .classify_call(
            "assertMystery",
            3,
            &[
                "6".to_string(),
                "makeValue()".to_string(),
                "mode".to_string(),
            ],
        )
        .expect("unclear overload is learned");
    assert_eq!(other.category, AssertCategory::Other);
}

#[test]
fn exact_assert_lifts_using_learned_vocab() {
    let vocab =
        derive_vocab_from_source("demo.assertions.LearnedAssertions", ASSERT_LIB, &[]).unwrap();
    let src = r#"
package demo;

import demo.assertions.LearnedAssertions;
import org.junit.jupiter.api.Test;

class ScalarTest {
    static int makeValue() { return 6; }

    @Test
    void scalarIsSix() {
        LearnedAssertions.assertSameValue(6, makeValue());
    }
}
"#;

    let out = lift_source_with_vocab(src, "src/test/java/demo/ScalarTest.java", &vocab);
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);
    let decl = &out.decls[0];
    assert_eq!(
        decl.name,
        "src/test/java/demo/ScalarTest.java::demo.ScalarTest.scalarIsSix"
    );
    assert!(decl.pre.is_none());
    assert!(decl.post.is_none());
    assert!(decl.evidence.is_none());
    let operands = inv_operands(decl);
    assert_eq!(operands.len(), 1);
    assert_eq_atom(&operands[0], 6);
}

#[test]
fn tolerance_signature_is_approx_and_not_exact_lifted() {
    let vocab =
        derive_vocab_from_source("demo.assertions.LearnedAssertions", ASSERT_LIB, &[]).unwrap();
    let src = r#"
package demo;

import static demo.assertions.LearnedAssertions.assertEquals;
import org.junit.jupiter.api.Test;

class ScalarTest {
    static double makeValue() { return 6.1; }

    @Test
	    void scalarApproximatelySix() {
	        assertEquals(6.0, makeValue(), 0.25);
	    }
	}
	"#;

    let out = lift_source_with_vocab(src, "src/test/java/demo/ScalarTest.java", &vocab);
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 0);
    assert!(
        out.decls.is_empty(),
        "approx must not become exact equality"
    );
    assert!(
        out.warnings
            .iter()
            .any(|w| w.reason.contains("approximate assertion")
                && w.reason.contains("refused to avoid false-pass")),
        "warnings: {:?}",
        out.warnings
    );
}

#[test]
fn equality_is_body_derived_not_name_derived() {
    let vocab =
        derive_vocab_from_source("demo.assertions.LearnedAssertions", ASSERT_LIB, &[]).unwrap();

    assert_eq!(
        vocab
            .classify_call(
                "assertSameValue",
                2,
                &["6".to_string(), "makeValue()".to_string()]
            )
            .unwrap()
            .category,
        AssertCategory::Equality,
        "non-assertEquals method whose body delegates equality must be equality"
    );
    assert_eq!(
        vocab
            .classify_call(
                "assertEquals",
                2,
                &["6".to_string(), "makeValue()".to_string()]
            )
            .unwrap()
            .category,
        AssertCategory::Other,
        "assertEquals name with opaque body must not be name-lifted as equality"
    );
}

#[test]
fn unclear_assert_method_loud_refuses_not_silently_drops() {
    let vocab =
        derive_vocab_from_source("demo.assertions.LearnedAssertions", ASSERT_LIB, &[]).unwrap();
    let src = r#"
package demo;

import static demo.assertions.LearnedAssertions.assertMystery;
import org.junit.jupiter.api.Test;

class ScalarTest {
    static int makeValue() { return 6; }

    @Test
    void scalarMystery() {
        assertMystery(6, makeValue(), mode);
    }
}
"#;

    let out = lift_source_with_vocab(src, "src/test/java/demo/ScalarTest.java", &vocab);
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 0);
    assert!(out.decls.is_empty());
    assert!(
        out.warnings
            .iter()
            .any(|w| w.reason.contains("LOUD REFUSAL")
                && w.reason.contains("not structurally clear")),
        "warnings: {:?}",
        out.warnings
    );
}

#[test]
fn external_exception_file_is_the_human_remainder_not_code() {
    let tmp = tempfile::tempdir().unwrap();
    let exc_dir = tmp.path().join(".sugar").join("vocab-exceptions");
    fs::create_dir_all(&exc_dir).unwrap();
    fs::write(
        exc_dir.join("demo.assertions.LearnedAssertions.json"),
        r#"{"overrides":{"equality":["assertOpaque"]}}"#,
    )
    .unwrap();

    let bare =
        derive_vocab_from_source("demo.assertions.LearnedAssertions", ASSERT_LIB, &[]).unwrap();
    assert_eq!(
        bare.classify_call(
            "assertOpaque",
            2,
            &["6".to_string(), "makeValue()".to_string()]
        )
        .unwrap()
        .category,
        AssertCategory::Other
    );

    let learned = learn_vocab_from_exception_dirs(
        "demo.assertions.LearnedAssertions",
        ASSERT_LIB,
        &[exc_dir.to_string_lossy().to_string()],
    )
    .unwrap();
    assert_eq!(
        learned
            .classify_call(
                "assertOpaque",
                2,
                &["6".to_string(), "makeValue()".to_string()]
            )
            .unwrap()
            .category,
        AssertCategory::Equality
    );
}
