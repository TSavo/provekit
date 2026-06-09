use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

use sugar_ir_symbolic::{serialize::formula_to_value, ConstValue, Formula, Term};
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
    public static void assertSameText(String expected, String actual) {
        if (!java.util.Objects.equals(expected, actual)) {
            throw new AssertionError("not equal");
        }
    }
    public static void assertSameFool(Fool expected, Fool actual) {
        if (!expected.equals(actual)) {
            throw new AssertionError("not equal");
        }
    }
    public static void assertSameObject(Object expected, Object actual) {
        if (!java.util.Objects.equals(expected, actual)) {
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

/// The `real_*_javap_*` tests shell out to `javap` (a JDK tool) to derive the
/// assertion vocab from the actual jar. CI runners without a JDK on PATH have no
/// `javap`, so these tests skip there rather than hard-fail on a missing
/// external tool -- they still run wherever a JDK is installed. (`macro_rules!`
/// `return`s from the calling test fn.)
fn javap_available() -> bool {
    Command::new("javap")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

macro_rules! require_javap {
    () => {
        if !javap_available() {
            eprintln!("SKIP: `javap` not on PATH (no JDK); javap-derived vocab test skipped");
            return;
        }
    };
}

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

fn assert_string_eq_atom(formula: &Formula, expected_rhs: &str) {
    match formula {
        Formula::Atomic { name, args } => {
            assert_eq!(name, "=");
            assert_eq!(args.len(), 2);
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

fn assert_operator_bool_atom(
    formula: &Formula,
    expected_call: &str,
    expected_lhs_ctor: &str,
    expected_lhs_value: i64,
    expected_rhs_ctor: &str,
    expected_rhs_value: i64,
    expected_result: bool,
) {
    match formula {
        Formula::Atomic { name, args } => {
            assert_eq!(name, "=");
            assert_eq!(args.len(), 2);
            match args[0].as_ref() {
                Term::Ctor { name, args } => {
                    assert_eq!(name, expected_call);
                    assert_eq!(args.len(), 2);
                    assert_single_int_ctor(&args[0], expected_lhs_ctor, expected_lhs_value);
                    assert_single_int_ctor(&args[1], expected_rhs_ctor, expected_rhs_value);
                }
                other => panic!("expected operator call lhs, got {other:?}"),
            }
            match args[1].as_ref() {
                Term::Const {
                    value: ConstValue::Bool(value),
                    ..
                } => assert_eq!(*value, expected_result),
                other => panic!("expected bool result rhs, got {other:?}"),
            }
        }
        other => panic!("expected operator result equality atom, got {other:?}"),
    }
}

fn assert_operator_var_atom(
    formula: &Formula,
    expected_call: &str,
    expected_lhs_var: &str,
    expected_rhs_var: &str,
    expected_result: bool,
) {
    match formula {
        Formula::Atomic { name, args } => {
            assert_eq!(name, "=");
            assert_eq!(args.len(), 2);
            match args[0].as_ref() {
                Term::Ctor { name, args } => {
                    assert_eq!(name, expected_call);
                    assert_eq!(args.len(), 2);
                    match args[0].as_ref() {
                        Term::Var { name } => assert_eq!(name, expected_lhs_var),
                        other => panic!("expected lhs var, got {other:?}"),
                    }
                    match args[1].as_ref() {
                        Term::Var { name } => assert_eq!(name, expected_rhs_var),
                        other => panic!("expected rhs var, got {other:?}"),
                    }
                }
                other => panic!("expected operator call lhs, got {other:?}"),
            }
            match args[1].as_ref() {
                Term::Const {
                    value: ConstValue::Bool(value),
                    ..
                } => assert_eq!(*value, expected_result),
                other => panic!("expected bool result rhs, got {other:?}"),
            }
        }
        other => panic!("expected operator result equality atom, got {other:?}"),
    }
}

fn assert_single_int_ctor(term: &Term, expected_ctor: &str, expected_value: i64) {
    match term {
        Term::Ctor { name, args } => {
            assert_eq!(name, expected_ctor);
            assert_eq!(args.len(), 1);
            match args[0].as_ref() {
                Term::Const {
                    value: ConstValue::Int(value),
                    ..
                } => assert_eq!(*value, expected_value),
                other => panic!("expected int constructor arg, got {other:?}"),
            }
        }
        other => panic!("expected constructor arg, got {other:?}"),
    }
}

fn formula_jcs(formula: &Formula) -> String {
    sugar_canonicalizer::encode_jcs(&formula_to_value(formula))
}

fn formula_cid(formula: &Formula) -> String {
    let jcs = formula_jcs(formula);
    sugar_canonicalizer::blake3_512_of(jcs.as_bytes())
}

fn rust_lift(src: &str, source_path: &str) -> sugar_lift_rust_tests::AdapterOutput {
    let file = syn::parse_file(src).expect("rust fixture parses");
    sugar_lift_rust_tests::lift_file(&file, source_path)
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
    require_javap!();
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
    require_javap!();
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
    require_javap!();
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
    require_javap!();
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
        "makeValue#euf#c:callresult_makeValue_a0()::assertion"
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
fn exact_string_assertions_lift_from_real_java_tests() {
    let vocab =
        derive_vocab_from_source("demo.assertions.LearnedAssertions", ASSERT_LIB, &[]).unwrap();
    let src = r#"
package demo;

import static demo.assertions.LearnedAssertions.assertSameText;
import org.junit.jupiter.api.Test;

class TextTest {
    static String encode(String input) { return input; }

    @Test
    void textRoundTrip() {
        assertSameText("codec", encode("codec"));
    }
}
"#;

    let out = lift_source_with_vocab(src, "src/test/java/demo/TextTest.java", &vocab);
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1);
    assert_eq!(
        out.decls[0].name,
        "encode#euf#c:callresult_encode_a1(s:\"codec\")::assertion"
    );
    let operands = inv_operands(&out.decls[0]);
    assert_eq!(operands.len(), 1);
    assert_string_eq_atom(&operands[0], "codec");
}

#[test]
fn string_constructor_assertion_preserves_legacy_new_string_key() {
    let vocab =
        derive_vocab_from_source("demo.assertions.LearnedAssertions", ASSERT_LIB, &[]).unwrap();
    let src = r#"
package demo;

import static demo.assertions.LearnedAssertions.assertSameText;
import org.junit.jupiter.api.Test;

class TextConstructorTest {
    @Test
    void textConstructor() {
        assertSameText("codec", new String(bytes));
    }
}
"#;

    let out = lift_source_with_vocab(src, "src/test/java/demo/TextConstructorTest.java", &vocab);
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(
        out.decls[0].name,
        "new String#euf#c:callresult_new_String_a1(v:bytes)::assertion"
    );
    let operands = inv_operands(&out.decls[0]);
    assert_eq!(operands.len(), 1);
    assert_string_eq_atom(&operands[0], "codec");
    match operands[0].as_ref() {
        Formula::Atomic { args, .. } => match args[0].as_ref() {
            Term::Ctor { name, .. } => assert_eq!(name, "call:new String"),
            other => panic!("expected new String call lhs, got {other:?}"),
        },
        other => panic!("expected equality atom, got {other:?}"),
    }
}

#[test]
fn direct_call_assertion_uses_python_style_euf_callsite_key() {
    let vocab =
        derive_vocab_from_source("demo.assertions.LearnedAssertions", ASSERT_LIB, &[]).unwrap();
    let src = r#"
package demo;

import static demo.assertions.LearnedAssertions.assertSameText;
import org.apache.commons.codec.binary.Base64;
import org.junit.jupiter.api.Test;

class CodecConsumerTest {
    @Test
    void standardBase64() {
        assertSameText("K/fMJwH+Q5e0nr7tWsxwkA==", Base64.encodeBase64String(b4));
    }
}
"#;

    let out = lift_source_with_vocab(src, "src/test/java/demo/CodecConsumerTest.java", &vocab);
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);
    let name = &out.decls[0].name;
    assert!(
        name.starts_with("org.apache.commons.codec.binary.Base64.encodeBase64String#euf#"),
        "direct library-call assertion must be keyed by callee+args, got {name}"
    );
    assert!(
        name.ends_with("::assertion"),
        "callsite assertion name must be verifier cross-proof shape, got {name}"
    );
}

#[test]
fn unsupported_junit_assertion_does_not_drop_supported_assertion_in_same_method() {
    let vocab =
        derive_vocab_from_source("demo.assertions.LearnedAssertions", ASSERT_LIB, &[]).unwrap();
    let src = r#"
package demo;

import static demo.assertions.LearnedAssertions.assertSameText;
import org.apache.commons.codec.binary.Base64;
import org.junit.jupiter.api.Test;

class MixedAssertionsTest {
    @Test
    void mixedAssertions() {
        assertUnsupported(value);
        assertSameText("K/fMJwH+Q5e0nr7tWsxwkA==", Base64.encodeBase64String(b4));
    }
}
"#;

    let out = lift_source_with_vocab(src, "src/test/java/demo/MixedAssertionsTest.java", &vocab);
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);
    assert!(
        out.decls[0]
            .name
            .starts_with("org.apache.commons.codec.binary.Base64.encodeBase64String#euf#"),
        "supported assertion should survive under callsite key: {:?}",
        out.decls
    );
    assert!(
        out.warnings
            .iter()
            .any(|w| w.reason.contains("assertUnsupported")),
        "unsupported assertion must be reported loudly, warnings: {:?}",
        out.warnings
    );
}

#[test]
fn computed_expected_call_equality_is_scoped_out_not_lifted() {
    let vocab =
        derive_vocab_from_source("demo.assertions.LearnedAssertions", ASSERT_LIB, &[]).unwrap();
    let src = r#"
package demo;

import static demo.assertions.LearnedAssertions.assertSameValue;
import org.junit.jupiter.api.Test;

class ComputedExpectedTest {
    static int expected() { return 6; }
    static int actual() { return 6; }

    @Test
    void computedExpected() {
        assertSameValue(expected(), actual());
    }
}
"#;

    let out = lift_source_with_vocab(src, "src/test/java/demo/ComputedExpectedTest.java", &vocab);
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 0);
    assert!(
        out.warnings
            .iter()
            .any(|w| w.reason.contains("computed call-vs-call equality")),
        "warnings: {:?}",
        out.warnings
    );
}

#[test]
fn call_first_literal_second_equality_stays_lifted() {
    let vocab =
        derive_vocab_from_source("demo.assertions.LearnedAssertions", ASSERT_LIB, &[]).unwrap();
    let src = r#"
package demo;

import static demo.assertions.LearnedAssertions.assertSameValue;
import org.junit.jupiter.api.Test;

class CallFirstTest {
    @Test
    void callFirst() {
        assertSameValue(ScalarBox.scalarSum(), 6);
    }
}
"#;

    let out = lift_source_with_vocab(src, "src/test/java/demo/CallFirstTest.java", &vocab);
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
}

#[test]
fn repeated_state_sensitive_receiver_call_conflict_is_scoped_out() {
    let vocab =
        derive_vocab_from_source("demo.assertions.LearnedAssertions", ASSERT_LIB, &[]).unwrap();
    let src = r#"
package demo;

import static demo.assertions.LearnedAssertions.assertSameText;
import org.junit.jupiter.api.Test;

class StatefulTextTest {
    @Test
    void statefulReceiver() {
        assertSameText("A", codec.next());
        codec.advance();
        assertSameText("B", codec.next());
    }
}
"#;

    let out = lift_source_with_vocab(src, "src/test/java/demo/StatefulTextTest.java", &vocab);
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 0);
    assert!(
        out.warnings
            .iter()
            .any(|w| w.reason.contains("state-sensitive repeated receiver call")),
        "warnings: {:?}",
        out.warnings
    );
}

#[test]
fn repeated_static_class_call_conflict_stays_lifted_as_contradiction() {
    let vocab =
        derive_vocab_from_source("demo.assertions.LearnedAssertions", ASSERT_LIB, &[]).unwrap();
    let src = r#"
package demo;

import static demo.assertions.LearnedAssertions.assertSameValue;
import org.junit.jupiter.api.Test;

class StaticCallTest {
    @Test
    void staticCallContradiction() {
        assertSameValue(6, ScalarBox.scalarSum());
        assertSameValue(7, ScalarBox.scalarSum());
    }
}
"#;

    let out = lift_source_with_vocab(src, "src/test/java/demo/StaticCallTest.java", &vocab);
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    let operands = inv_operands(&out.decls[0]);
    assert_eq!(operands.len(), 2);
}

#[test]
fn reassigned_actual_variable_conflict_is_scoped_out() {
    let vocab =
        derive_vocab_from_source("demo.assertions.LearnedAssertions", ASSERT_LIB, &[]).unwrap();
    let src = r#"
package demo;

import static demo.assertions.LearnedAssertions.assertSameText;
import org.junit.jupiter.api.Test;

class ReassignedActualTest {
    @Test
    void reassignedActual() {
        String actual = "A";
        assertSameText("A", actual);
        actual = "B";
        assertSameText("B", actual);
    }
}
"#;

    let out = lift_source_with_vocab(src, "src/test/java/demo/ReassignedActualTest.java", &vocab);
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 0);
    assert!(
        out.warnings
            .iter()
            .any(|w| w.reason.contains("reassigned actual variable")),
        "warnings: {:?}",
        out.warnings
    );
}

#[test]
fn receiver_field_path_conflict_is_scoped_out() {
    let vocab =
        derive_vocab_from_source("demo.assertions.LearnedAssertions", ASSERT_LIB, &[]).unwrap();
    let src = r#"
package demo;

import static demo.assertions.LearnedAssertions.assertSameValue;
import org.junit.jupiter.api.Test;

class ReceiverFieldStateTest {
    @Test
    void receiverFieldState() {
        assertSameValue(0, context.ibitWorkArea);
        decode(context);
        assertSameValue(15, context.ibitWorkArea);
    }
}
"#;

    let out = lift_source_with_vocab(
        src,
        "src/test/java/demo/ReceiverFieldStateTest.java",
        &vocab,
    );
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 0);
    assert!(
        out.warnings
            .iter()
            .any(|w| w.reason.contains("receiver field/path actual")),
        "warnings: {:?}",
        out.warnings
    );
}

#[test]
fn mutated_argument_call_conflict_is_scoped_out() {
    let vocab =
        derive_vocab_from_source("demo.assertions.LearnedAssertions", ASSERT_LIB, &[]).unwrap();
    let src = r#"
package demo;

import static demo.assertions.LearnedAssertions.assertSameText;
import java.util.Map;
import java.util.TreeMap;
import org.junit.jupiter.api.Test;

class MutatedArgumentTest {
    static String encode(Map<String, String> args, String input) { return input; }

    @Test
    void mutatedArgument() {
        Map<String, String> args = new TreeMap<>();
        args.put("mode", "A");
        assertSameText("A", encode(args, "x"));
        args.put("mode", "B");
        assertSameText("B", encode(args, "x"));
    }
}
"#;

    let out = lift_source_with_vocab(src, "src/test/java/demo/MutatedArgumentTest.java", &vocab);
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 0);
    assert!(
        out.warnings
            .iter()
            .any(|w| w.reason.contains("mutated call argument")),
        "warnings: {:?}",
        out.warnings
    );
}

#[test]
fn byte_buffer_position_limit_conflict_is_scoped_out() {
    let vocab =
        derive_vocab_from_source("demo.assertions.LearnedAssertions", ASSERT_LIB, &[]).unwrap();
    let src = r#"
package demo;

import static demo.assertions.LearnedAssertions.assertSameText;
import java.nio.ByteBuffer;
import org.junit.jupiter.api.Test;

class ByteBufferStateTest {
    static String encode(ByteBuffer bb) { return ""; }

    @Test
    void byteBufferState() {
        ByteBuffer bb = ByteBuffer.allocate(36);
        bb.limit(3);
        assertSameText("000000", encode(bb));
        bb.position(1);
        bb.limit(3);
        assertSameText("0000", encode(bb));
    }
}
"#;

    let out = lift_source_with_vocab(src, "src/test/java/demo/ByteBufferStateTest.java", &vocab);
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 0);
    assert!(
        out.warnings
            .iter()
            .any(|w| w.reason.contains("mutated call argument")),
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

#[test]
fn object_assert_equals_operator_dispatch_atom() {
    let vocab =
        derive_vocab_from_source("demo.assertions.LearnedAssertions", ASSERT_LIB, &[]).unwrap();
    let src = r#"
package demo;

import static demo.assertions.LearnedAssertions.assertSameFool;
import org.junit.jupiter.api.Test;

class FoolDispatchTest {
    static final class Fool {
        Fool(int value) {}
    }

    @Test
    void foolDispatch() {
        assertSameFool(new Fool(1), new Fool(0));
    }
}
"#;

    let out = lift_source_with_vocab(src, "src/test/java/demo/FoolDispatchTest.java", &vocab);
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);
    assert_eq!(
        out.decls[0].name,
        "src/test/java/demo/FoolDispatchTest.java::demo.FoolDispatchTest.foolDispatch"
    );
    let operands = inv_operands(&out.decls[0]);
    assert_eq!(operands.len(), 1);
    assert_operator_bool_atom(
        &operands[0],
        "call:eq:Fool",
        "call:Fool",
        1,
        "call:Fool",
        0,
        true,
    );
}

#[test]
fn object_reference_eq_operator_dispatch_atom() {
    let vocab =
        derive_vocab_from_source("demo.assertions.LearnedAssertions", ASSERT_LIB, &[]).unwrap();
    let src = r#"
package demo;

import static demo.assertions.LearnedAssertions.assertTruth;
import org.junit.jupiter.api.Test;

class FoolReferenceTest {
    static final class Fool {
        Fool(int value) {}
    }

    @Test
    void foolReference() {
        assertTruth(new Fool(1) == new Fool(0));
    }
}
"#;

    let out = lift_source_with_vocab(src, "src/test/java/demo/FoolReferenceTest.java", &vocab);
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);
    let operands = inv_operands(&out.decls[0]);
    assert_eq!(operands.len(), 1);
    assert_operator_bool_atom(
        &operands[0],
        "call:eq:Fool",
        "call:Fool",
        1,
        "call:Fool",
        0,
        true,
    );
}

#[test]
fn object_var_vs_var_assert_equals_uses_coarse_operator_dispatch_atom() {
    let vocab =
        derive_vocab_from_source("demo.assertions.LearnedAssertions", ASSERT_LIB, &[]).unwrap();
    let src = r#"
package demo;

import static demo.assertions.LearnedAssertions.assertSameObject;
import org.junit.jupiter.api.Test;

class ObjectVarDispatchTest {
    @Test
    void objectVarDispatch() {
        assertSameObject(json, streamed);
    }
}
"#;

    let out = lift_source_with_vocab(src, "src/test/java/demo/ObjectVarDispatchTest.java", &vocab);
    assert_eq!(out.seen, 1);
    assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    assert_eq!(out.decls.len(), 1);
    let operands = inv_operands(&out.decls[0]);
    assert_eq!(operands.len(), 1);
    assert_operator_var_atom(&operands[0], "call:eq:Object", "json", "streamed", true);
}

#[test]
fn java_object_operator_dispatch_atom_matches_rust_canonical_bytes() {
    let vocab =
        derive_vocab_from_source("demo.assertions.LearnedAssertions", ASSERT_LIB, &[]).unwrap();
    let java_src = r#"
package demo;

import static demo.assertions.LearnedAssertions.assertSameFool;
import org.junit.jupiter.api.Test;

class FoolFederationTest {
    static final class Fool {
        Fool(int value) {}
    }

    @Test
    void foolFederation() {
        assertSameFool(new Fool(1), new Fool(0));
    }
}
"#;
    let java_out = lift_source_with_vocab(
        java_src,
        "src/test/java/demo/FoolFederationTest.java",
        &vocab,
    );
    assert_eq!(java_out.seen, 1);
    assert_eq!(java_out.lifted, 1, "warnings: {:?}", java_out.warnings);
    let java_atom = inv_operands(&java_out.decls[0])[0].clone();

    let rust_src = r#"
struct Fool(i32);

#[test]
fn java_federation_shape() {
    assert_eq!(Fool(1), Fool(0));
}
"#;
    let rust_out = rust_lift(rust_src, "tests/cmp.rs");
    assert_eq!(rust_out.seen, 1);
    assert_eq!(rust_out.lifted, 1, "warnings: {:?}", rust_out.warnings);
    let rust_atom = inv_operands(&rust_out.decls[0])[0].clone();

    let java_cid = formula_cid(&java_atom);
    let rust_cid = formula_cid(&rust_atom);
    println!("java_operator_cid={java_cid}");
    println!("rust_operator_cid={rust_cid}");
    assert_eq!(formula_jcs(&java_atom), formula_jcs(&rust_atom));
    assert_eq!(java_cid, rust_cid);
}
