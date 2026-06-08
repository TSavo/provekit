use sugar_canonicalizer::blake3_512_of;
use sugar_lift_java_tests::{
    build_bundle, junit_witness_package_contract_ir, parse_junit_xml_reports,
    testng_witness_package_contract_ir, testng_witness_package_memento, witness_body,
    witness_package_memento, witness_package_proof_data, Witness, WITNESS_SIGNER_SEED,
};
use sugar_proof_envelope::ed25519_verify_string;

const CODE_CID: &str = "blake3-512:code";
const RUNTIME_CID: &str = "blake3-512:runtime";

fn witness(test_id: &str, outcome: &str) -> Witness {
    Witness::new_for_test(
        CODE_CID,
        RUNTIME_CID,
        test_id,
        outcome,
        &["src/main/java/demo/Calculator.java".to_string()],
    )
}

#[test]
fn parser_extracts_junit_xml_pass_fail_and_drops_skips() {
    let xml = r#"
<testsuite name="demo.ScalarTest" tests="3" skipped="1" failures="1" errors="0">
  <testcase name="scalarIsSix()" classname="demo.ScalarTest"/>
  <testcase name="scalarContradiction()" classname="demo.ScalarTest">
    <failure message="expected: &lt;7&gt; but was: &lt;6&gt;"/>
  </testcase>
  <testcase name="ignored()" classname="demo.ScalarTest">
    <skipped/>
  </testcase>
</testsuite>
"#;

    let parsed = parse_junit_xml_reports(xml);
    assert_eq!(parsed.len(), 3);
    assert_eq!(parsed[0].test_id, "demo.ScalarTest.scalarIsSix");
    assert_eq!(parsed[0].raw, "passed");
    assert_eq!(parsed[1].test_id, "demo.ScalarTest.scalarContradiction");
    assert_eq!(parsed[1].raw, "failed");
    assert_eq!(parsed[2].raw, "skipped");
}

#[test]
fn per_test_body_is_content_addressed() {
    let w = witness("demo.ScalarTest.scalarIsSix", "passed");
    assert_eq!(blake3_512_of(&witness_body(&w)), w.cid);
    let body = String::from_utf8(witness_body(&w)).unwrap();
    assert!(body.contains(r#""kind":"junit-test-witness""#));
    assert!(body.contains(r#""outcome":"passed""#));
}

#[test]
fn bundle_cid_changes_when_any_test_fails() {
    let good = vec![
        witness("demo.ScalarTest.a", "passed"),
        witness("demo.ScalarTest.b", "passed"),
    ];
    let bad = vec![
        witness("demo.ScalarTest.a", "passed"),
        witness("demo.ScalarTest.b", "failed"),
    ];
    let (_, good_cid, _) = build_bundle(&good);
    let (_, bad_cid, _) = build_bundle(&bad);
    assert_ne!(good_cid, bad_cid);
    assert_eq!(good.iter().filter(|w| w.outcome != "passed").count(), 0);
    assert_eq!(bad.iter().filter(|w| w.outcome != "passed").count(), 1);
}

#[test]
fn package_memento_signature_verifies_over_bundle_cid() {
    let (_, cid, _) = build_bundle(&[witness("demo.ScalarTest.a", "passed")]);
    let m = witness_package_memento(
        &cid,
        &[".".to_string()],
        &["src/main/java/demo/Calculator.java".to_string()],
        1,
        1,
        Some(WITNESS_SIGNER_SEED),
    )
    .unwrap();
    assert_eq!(m["witness_cid"], cid);
    assert_eq!(m["witness_kind"], "junit-test-witness-package");
    let signer = m["signer"].as_str().unwrap();
    let signature = m["signature"].as_str().unwrap();
    assert!(ed25519_verify_string(signer, signature, cid.as_bytes()));
    assert!(!ed25519_verify_string(signer, signature, b"not-the-cid"));
}

#[test]
fn testng_package_memento_signature_verifies_over_bundle_cid() {
    let (_, cid, _) = build_bundle(&[witness("demo.ScalarTest.a", "passed")]);
    let m = testng_witness_package_memento(
        &cid,
        &[".".to_string()],
        &["src/main/java/demo/Calculator.java".to_string()],
        1,
        1,
        Some(WITNESS_SIGNER_SEED),
    )
    .unwrap();
    assert_eq!(m["witness_cid"], cid);
    assert_eq!(m["witness_kind"], "testng-test-witness-package");
    let signer = m["signer"].as_str().unwrap();
    let signature = m["signature"].as_str().unwrap();
    assert!(ed25519_verify_string(signer, signature, cid.as_bytes()));
}

#[test]
fn contract_ir_carries_junit_custom_evidence() {
    let cid = "blake3-512:bundle";
    let proof_data = witness_package_proof_data(
        cid,
        &[".".to_string()],
        &["src/main/java/demo/Calculator.java".to_string()],
        2,
        2,
    );
    assert_eq!(
        proof_data,
        r#"{"codeFiles":["src/main/java/demo/Calculator.java"],"count":2,"kind":"witness-package","packageCid":"blake3-512:bundle","passed":2,"testFiles":["."]}"#
    );

    let ir = junit_witness_package_contract_ir(
        cid,
        RUNTIME_CID,
        &[".".to_string()],
        &["src/main/java/demo/Calculator.java".to_string()],
        2,
        2,
    );
    assert_eq!(ir["kind"], "contract");
    assert_eq!(ir["name"], "witness-package:blake3-512:bundle");
    assert_eq!(ir["inv"]["name"], "witnessed");
    assert_eq!(ir["evidence"]["proofType"], "custom");
    assert_eq!(ir["evidence"]["certificate"]["tool"], "junit");
    assert_eq!(ir["evidence"]["certificate"]["formulaHash"], cid);
}

#[test]
fn contract_ir_carries_testng_custom_evidence() {
    let cid = "blake3-512:bundle";
    let ir = testng_witness_package_contract_ir(
        cid,
        RUNTIME_CID,
        &[".".to_string()],
        &["src/main/java/demo/Calculator.java".to_string()],
        2,
        2,
    );
    assert_eq!(ir["kind"], "contract");
    assert_eq!(ir["name"], "witness-package:blake3-512:bundle");
    assert_eq!(ir["inv"]["name"], "witnessed");
    assert_eq!(ir["evidence"]["proofType"], "custom");
    assert_eq!(ir["evidence"]["certificate"]["tool"], "testng");
    assert_eq!(ir["evidence"]["certificate"]["formulaHash"], cid);
}
