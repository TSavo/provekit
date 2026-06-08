use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

use sugar_ir_symbolic::{ConstValue, Formula, Term};
use sugar_lift_java_tests::{
    derive_vocab_from_javap, derive_vocab_from_source, learn_vocab_from_exception_dirs,
    lift_source_with_vocab, AssertCategory,
};

// Unit-test fixture only. The showcase learns real JUnit from javap instead of
// using this demo assertion class as its vocab source.
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

static JUNIT_JAR_FETCH_LOCK: Mutex<()> = Mutex::new(());
static TESTNG_JAR_FETCH_LOCK: Mutex<()> = Mutex::new(());

fn real_junit_console_jar() -> String {
    if let Ok(path) = std::env::var("SUGAR_JUNIT_CONSOLE_JAR") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return path.to_string_lossy().to_string();
        }
    }

    let version = std::env::var("JUNIT_VERSION").unwrap_or_else(|_| "1.10.2".to_string());
    let path = PathBuf::from(format!(
        "/tmp/sugar-junit/junit-platform-console-standalone-{version}.jar"
    ));
    if path.is_file() {
        return path.to_string_lossy().to_string();
    }

    let _guard = JUNIT_JAR_FETCH_LOCK.lock().unwrap();
    if path.is_file() {
        return path.to_string_lossy().to_string();
    }

    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let url = format!(
        "https://repo1.maven.org/maven2/org/junit/platform/junit-platform-console-standalone/{version}/junit-platform-console-standalone-{version}.jar"
    );
    let status = if command_exists("curl") {
        Command::new("curl")
            .args(["-fsSL", &url, "-o"])
            .arg(&path)
            .status()
            .expect("spawn curl")
    } else {
        Command::new("wget")
            .args(["-q", &url, "-O"])
            .arg(&path)
            .status()
            .expect("spawn wget")
    };
    assert!(status.success(), "fetch real JUnit console jar from {url}");
    path.to_string_lossy().to_string()
}

fn real_testng_jar() -> String {
    if let Ok(path) = std::env::var("SUGAR_TESTNG_JAR") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return path.to_string_lossy().to_string();
        }
    }

    let version = std::env::var("TESTNG_VERSION").unwrap_or_else(|_| "7.10.2".to_string());
    let path = PathBuf::from(format!("/tmp/sugar-testng/testng-{version}.jar"));
    if path.is_file() {
        return path.to_string_lossy().to_string();
    }

    let _guard = TESTNG_JAR_FETCH_LOCK.lock().unwrap();
    if path.is_file() {
        return path.to_string_lossy().to_string();
    }

    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let url =
        format!("https://repo1.maven.org/maven2/org/testng/testng/{version}/testng-{version}.jar");
    let status = if command_exists("curl") {
        Command::new("curl")
            .args(["-fsSL", &url, "-o"])
            .arg(&path)
            .status()
            .expect("spawn curl")
    } else {
        Command::new("wget")
            .args(["-q", &url, "-O"])
            .arg(&path)
            .status()
            .expect("spawn wget")
    };
    assert!(status.success(), "fetch real TestNG jar from {url}");
    path.to_string_lossy().to_string()
}

fn command_exists(bin: &str) -> bool {
    Command::new(bin)
        .arg("--version")
        .output()
        .is_ok_and(|out| out.status.success())
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

fn assert_method_category(
    vocab: &sugar_lift_java_tests::AssertionVocab,
    name: &str,
    params: &[&str],
    category: AssertCategory,
    source: &str,
) {
    let method = vocab
        .methods
        .iter()
        .find(|method| {
            method.name == name
                && method
                    .params
                    .iter()
                    .map(|param| param.ty.as_str())
                    .eq(params.iter().copied())
        })
        .unwrap_or_else(|| panic!("missing method {name}({})", params.join(", ")));
    assert_eq!(method.category, category);
    assert_eq!(method.source, source);
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
fn real_junit_javap_derives_delta_overload_as_approx_from_signature() {
    let jar = real_junit_console_jar();
    let vocab = derive_vocab_from_javap("org.junit.jupiter.api.Assertions", &jar, &[]).unwrap();
    let dump = vocab
        .dump_lines()
        .into_iter()
        .filter(|line| line.contains("assertEquals") || line.contains("assertTrue"))
        .collect::<Vec<_>>();
    eprintln!("real-junit-derived-vocab: {}", dump.join("; "));

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
        .expect("real JUnit delta overload is learned");
    assert_eq!(approx.category, AssertCategory::Approx);
    assert_eq!(approx.source, "javap-signature");
}

#[test]
fn real_junit_external_override_supplies_body_gap_without_changing_approx_split() {
    let jar = real_junit_console_jar();
    let tmp = tempfile::tempdir().unwrap();
    let exc_dir = tmp.path().join(".sugar").join("vocab-exceptions");
    fs::create_dir_all(&exc_dir).unwrap();
    fs::write(
        exc_dir.join("org.junit.jupiter.api.Assertions.json"),
        r#"{"overrides":{"equality":["assertEquals"],"truth":["assertTrue"]}}"#,
    )
    .unwrap();

    let bare = derive_vocab_from_javap("org.junit.jupiter.api.Assertions", &jar, &[]).unwrap();
    let learned = derive_vocab_from_javap(
        "org.junit.jupiter.api.Assertions",
        &jar,
        &[exc_dir.to_string_lossy().to_string()],
    )
    .unwrap();
    eprintln!(
        "real-junit-derived-vocab-with-overrides: {}",
        learned
            .dump_lines()
            .into_iter()
            .filter(|line| line.contains("assertEquals") || line.contains("assertTrue"))
            .collect::<Vec<_>>()
            .join("; ")
    );

    let bare_approx = bare
        .classify_call(
            "assertEquals",
            3,
            &[
                "6.0".to_string(),
                "makeValue()".to_string(),
                "0.01".to_string(),
            ],
        )
        .expect("bare real JUnit delta overload is learned");
    let learned_approx = learned
        .classify_call(
            "assertEquals",
            3,
            &[
                "6.0".to_string(),
                "makeValue()".to_string(),
                "0.01".to_string(),
            ],
        )
        .expect("overridden real JUnit delta overload remains learned");
    assert_eq!(bare_approx.category, AssertCategory::Approx);
    assert_eq!(learned_approx.category, AssertCategory::Approx);
    assert_eq!(learned_approx.source, "javap-signature");

    let learned_approx_with_message = learned
        .classify_call(
            "assertEquals",
            4,
            &[
                "6.0".to_string(),
                "makeValue()".to_string(),
                "0.01".to_string(),
                "\"close enough\"".to_string(),
            ],
        )
        .expect("overridden real JUnit delta+message overload remains learned");
    assert_eq!(learned_approx_with_message.category, AssertCategory::Approx);
    assert_eq!(learned_approx_with_message.source, "javap-signature");

    let bare_exact = bare
        .classify_call(
            "assertEquals",
            2,
            &["6".to_string(), "makeValue()".to_string()],
        )
        .expect("bare exact overload is present from real JUnit signature");
    assert_ne!(bare_exact.category, AssertCategory::Equality);

    let learned_exact = learned
        .classify_call(
            "assertEquals",
            2,
            &["6".to_string(), "makeValue()".to_string()],
        )
        .expect("override marks exact overload as equality");
    assert_eq!(learned_exact.category, AssertCategory::Equality);
    assert_eq!(learned_exact.source, "external-exception");

    let learned_truth = learned
        .classify_call("assertTrue", 1, &["makeValue() == 6".to_string()])
        .expect("override marks assertTrue as truth");
    assert_eq!(learned_truth.category, AssertCategory::Truth);
    assert_eq!(learned_truth.source, "external-exception");
}

#[test]
fn real_testng_javap_derives_delta_overload_as_approx_from_signature_without_classifier_changes() {
    let jar = real_testng_jar();
    let vocab = derive_vocab_from_javap("org.testng.Assert", &jar, &[]).unwrap();
    let dump = vocab
        .dump_lines()
        .into_iter()
        .filter(|line| line.contains("assertEquals") || line.contains("assertTrue"))
        .collect::<Vec<_>>();
    eprintln!("real-testng-derived-vocab: {}", dump.join("; "));

    let approx = vocab
        .classify_call(
            "assertEquals",
            3,
            &[
                "makeValue()".to_string(),
                "6.0".to_string(),
                "0.01".to_string(),
            ],
        )
        .expect("real TestNG delta overload is learned");
    assert_eq!(approx.category, AssertCategory::Approx);
    assert_eq!(approx.source, "javap-signature");
}

#[test]
fn real_testng_external_override_supplies_body_gap_without_changing_approx_split() {
    let jar = real_testng_jar();
    let tmp = tempfile::tempdir().unwrap();
    let exc_dir = tmp.path().join(".sugar").join("vocab-exceptions");
    fs::create_dir_all(&exc_dir).unwrap();
    fs::write(
        exc_dir.join("org.testng.Assert.json"),
        r#"{"overrides":{"equality":["assertEquals"],"truth":["assertTrue"]}}"#,
    )
    .unwrap();

    let bare = derive_vocab_from_javap("org.testng.Assert", &jar, &[]).unwrap();
    let learned = derive_vocab_from_javap(
        "org.testng.Assert",
        &jar,
        &[exc_dir.to_string_lossy().to_string()],
    )
    .unwrap();
    eprintln!(
        "real-testng-derived-vocab-with-overrides: {}",
        learned
            .dump_lines()
            .into_iter()
            .filter(|line| line.contains("assertEquals") || line.contains("assertTrue"))
            .collect::<Vec<_>>()
            .join("; ")
    );

    let bare_approx = bare
        .classify_call(
            "assertEquals",
            3,
            &[
                "makeValue()".to_string(),
                "6.0".to_string(),
                "0.01".to_string(),
            ],
        )
        .expect("bare real TestNG delta overload is learned");
    let learned_approx = learned
        .classify_call(
            "assertEquals",
            3,
            &[
                "makeValue()".to_string(),
                "6.0".to_string(),
                "0.01".to_string(),
            ],
        )
        .expect("overridden real TestNG delta overload remains learned");
    assert_eq!(bare_approx.category, AssertCategory::Approx);
    assert_eq!(learned_approx.category, AssertCategory::Approx);
    assert_eq!(learned_approx.source, "javap-signature");

    assert_method_category(
        &learned,
        "assertEquals",
        &["double", "double", "double", "java.lang.String"],
        AssertCategory::Approx,
        "javap-signature",
    );

    let bare_exact = bare
        .classify_call(
            "assertEquals",
            2,
            &["makeValue()".to_string(), "6".to_string()],
        )
        .expect("bare exact overload is present from real TestNG signature");
    assert_ne!(bare_exact.category, AssertCategory::Equality);

    let learned_exact = learned
        .classify_call(
            "assertEquals",
            2,
            &["makeValue()".to_string(), "6".to_string()],
        )
        .expect("override marks exact overload as equality");
    assert_eq!(learned_exact.category, AssertCategory::Equality);
    assert_eq!(learned_exact.source, "external-exception");

    let learned_truth = learned
        .classify_call("assertTrue", 1, &["makeValue() == 6".to_string()])
        .expect("override marks TestNG assertTrue as truth");
    assert_eq!(learned_truth.category, AssertCategory::Truth);
    assert_eq!(learned_truth.source, "external-exception");
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
