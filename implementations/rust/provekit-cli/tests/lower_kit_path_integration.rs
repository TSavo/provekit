// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use libprovekit::compose::{
    build_value, cid_of_value, jcs_bytes_of_value, EffectSet, FunctionContractMemento, Locus,
};
use libprovekit::core::{
    address, execute_path, Cid, ConformanceDeclaration, DomainClaim, DomainKind,
    HashMapInputCatalog, Input, KitRegistry, LowerKit, Path as CorePath, PathAlgebra, Term, Verb,
    Verdict,
};
use provekit_cli::kit_dispatch::DispatchRealizeTransport;
use provekit_ir_types::{IrFormula, Sort};
use serde_json::json;

fn valid_cid(fill: char) -> String {
    format!("blake3-512:{}", fill.to_string().repeat(128))
}

fn parse_cid(fill: char) -> Cid {
    Cid::parse(valid_cid(fill)).expect("valid CID")
}

fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).expect("write script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod script");
    }
}

fn fake_realizer(root: &Path) -> PathBuf {
    let script = root.join("fake-python-realizer.sh");
    write_executable(
        &script,
        &format!(
            r#"#!/usr/bin/env bash
set -euo pipefail
while IFS= read -r line; do
  if [[ "$line" == *'"method":"provekit.plugin.invoke"'* ]]; then
    printf '%s\n' '{{"jsonrpc":"2.0","id":1,"result":{{"source":"def add(x, y):\n    return x + y\n","is_stub":false,"extension":"py","emitted_artifact_cid":"{}","observed_loss_record":{{"kind":"loss-record","cid":"{}"}},"used_sugars":[{{"header":{{"cid":"{}"}}}}]}}}}'
    exit 0
  fi
done
"#,
            valid_cid('b'),
            valid_cid('c'),
            valid_cid('d')
        ),
    );
    script
}

fn register_fake_realizer(project: &Path, script: &Path) {
    let realize_dir = project.join(".provekit").join("realize").join("python");
    fs::create_dir_all(&realize_dir).expect("create realize manifest dir");
    fs::write(
        realize_dir.join("manifest.toml"),
        format!(
            "name = \"fake-python-realizer\"\nlibrary_tag = \"default\"\ncommand = [\"bash\", \"{}\"]\n",
            script.display()
        ),
    )
    .expect("write manifest");
}

fn fake_missing_template_realizer(root: &Path) -> PathBuf {
    let script = root.join("fake-missing-template-realizer.sh");
    write_executable(
        &script,
        r#"#!/usr/bin/env bash
set -euo pipefail
while IFS= read -r line; do
  if [[ "$line" == *'"method":"provekit.plugin.invoke"'* ]]; then
    printf '%s\n' '{"jsonrpc":"2.0","id":1,"error":{"code":-32100,"message":"missing body-template entry","data":[{"operation_kind":"call:Widget::build","args_shape":["int"],"function":"unknown_call","term_position":"body.return"}]}}'
    exit 0
  fi
done
"#,
    );
    script
}

fn minimal_contract() -> FunctionContractMemento {
    let formals = vec!["x".to_string()];
    let formal_sorts = vec![Sort::Primitive {
        name: "int".to_string(),
    }];
    let return_sort = Sort::Primitive {
        name: "int".to_string(),
    };
    let pre = IrFormula::Atomic {
        name: "true".to_string(),
        args: vec![],
    };
    let post = IrFormula::Atomic {
        name: "true".to_string(),
        args: vec![],
    };
    let effects = EffectSet::empty();
    let locus = Locus::unknown();
    let value = build_value(
        "prior",
        &formals,
        &formal_sorts,
        &return_sort,
        &pre,
        &post,
        None,
        &effects,
        &locus,
        &[],
    );
    FunctionContractMemento {
        fn_name: "prior".to_string(),
        formals,
        formal_sorts,
        formal_regions: vec![],
        return_sort,
        return_region: None,
        pre,
        post,
        body_cid: None,
        effects,
        locus,
        canonical_bytes: jcs_bytes_of_value(&value),
        cid: cid_of_value(&value),
        auto_minted_mementos: vec![],
        concept_hint: None,
    }
}

#[test]
fn lower_python_path_claim_input_cites_from_premise_to_and_loss_cids() {
    let temp = tempfile::tempdir().expect("tempdir");
    let script = fake_realizer(temp.path());
    register_fake_realizer(temp.path(), &script);

    let term_cid = parse_cid('a');
    let policy_cid = parse_cid('e');
    let sugar_cid = parse_cid('d');
    let body_template_cid = parse_cid('f');
    let spec = json!({
        "kind": "RealizeRequest",
        "function": "add",
        "params": ["x", "y"],
        "paramTypes": ["int", "int"],
        "returnType": "int",
        "conceptName": "concept:add",
        "namedTermTree": {
            "conceptName": "concept:add",
            "operationKind": "add",
            "shapeCid": term_cid,
            "args": []
        },
        "termShapeCid": term_cid,
        "policyCid": policy_cid,
        "sugarCids": [sugar_cid],
        "bodyTemplateCids": [body_template_cid]
    });
    let prior_claim = DomainClaim {
        domain: DomainKind::Other("prior-step".to_string()),
        contract: minimal_contract(),
        artifacts: vec![],
        from: vec![term_cid.clone()],
        premises: vec![],
        to: term_cid.clone(),
        witness: None,
        payload: Some(Term::Const {
            value: spec,
            sort: Sort::Primitive {
                name: "LowerSpec".to_string(),
            },
        }),
        verdict: Verdict::Unresolved,
        attestation: None,
    };
    let prior_claim_cid = prior_claim.cid();
    let mut inputs = HashMapInputCatalog::default();
    let prior_input = Input::Claim(prior_claim);
    let prior_input_cid = inputs.insert(prior_input);
    let path = Input::Path(Box::new(CorePath {
        algebra: vec![PathAlgebra {
            name: "lower".to_string(),
            kit: "lower-python".to_string(),
            inputs: vec![prior_input_cid],
            depends_on: vec![],
            verb: Verb::Transform,
        }],
    }));
    let mut registry = KitRegistry::default();
    registry.register(
        "lower-python",
        LowerKit::new(
            temp.path().to_path_buf(),
            "python",
            None,
            DispatchRealizeTransport,
        ),
        ConformanceDeclaration::Carrier {
            fixtures_path: temp
                .path()
                .join("implementations/python/conformance/fixtures"),
            platform_semantics: None,
        },
    );

    let chain = execute_path(&path, &registry, &inputs).expect("lower path executes");
    let claim = chain.terminal_claim();

    assert_eq!(claim.from, vec![term_cid]);
    assert_eq!(claim.premises, vec![prior_claim_cid]);
    assert_eq!(claim.to, parse_cid('b'));
    assert!(claim.artifacts.contains(&parse_cid('c')));
    assert!(claim.artifacts.contains(&policy_cid));
    assert!(claim.artifacts.contains(&sugar_cid));
    assert!(claim.artifacts.contains(&body_template_cid));
    assert_eq!(
        claim.payload.as_ref().map(address),
        Some(address(claim.payload.as_ref().expect("payload term")))
    );
}

#[test]
fn lower_cli_missing_body_template_refuses_without_partial_output() {
    let temp = tempfile::tempdir().expect("tempdir");
    let script = fake_missing_template_realizer(temp.path());
    register_fake_realizer(temp.path(), &script);
    let input = temp.path().join("named-terms.json");
    let output = temp.path().join("out.py");
    fs::write(
        &input,
        serde_json::to_vec_pretty(&json!({
            "kind": "named-term-document",
            "promotionDecisionMementos": [],
            "schemaVersion": "1",
            "sourceLanguage": "rust",
            "workspaceRoot": temp.path(),
            "terms": [{
                "conceptName": "concept:add",
                "dischargeVerdict": "exact",
                "file": "src/lib.rs",
                "function": "add",
                "name": "add",
                "namedTermTree": {
                    "conceptName": "concept:add",
                    "operationKind": "add",
                    "shapeCid": valid_cid('a'),
                    "args": []
                },
                "paramTypes": ["int", "int"],
                "params": ["x", "y"],
                "returnType": "int",
                "siteMementoCid": valid_cid('e'),
                "termShape": {"kind": "op", "name": "add"},
                "termShapeCid": valid_cid('a'),
                "witnesses": []
            }]
        }))
        .expect("encode named terms"),
    )
    .expect("write named terms");

    let output_result = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("lower")
        .arg(&input)
        .arg("--target")
        .arg("python")
        .arg("--project")
        .arg(temp.path())
        .arg("-o")
        .arg(&output)
        .output()
        .expect("spawn provekit lower");

    let stderr = String::from_utf8_lossy(&output_result.stderr);
    assert!(
        !output_result.status.success(),
        "missing template must fail\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("missing body-template entry")
            && stderr.contains("call:Widget::build")
            && stderr.contains("unknown_call"),
        "stderr should carry missing template receipt\nstderr:\n{stderr}"
    );
    assert!(
        !output.exists(),
        "missing template refusal must not leave a partial output file"
    );
}
