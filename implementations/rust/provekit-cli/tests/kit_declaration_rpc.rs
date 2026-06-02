use std::fs;
use std::path::Path;

use tempfile::TempDir;

use provekit_claim_envelope::{KitDeclaration, KIT_DECLARATION_RPC_METHOD};

fn make_executable(path: &Path, body: &str) {
    fs::write(path, body).expect("write stub");
    let mut perms = fs::metadata(path).expect("metadata").permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o755);
    }
    fs::set_permissions(path, perms).expect("chmod");
}

#[test]
fn loader_sends_kit_declaration_rpc_and_parses_result() {
    let td = TempDir::new().expect("tempdir");
    let marker = td.path().join("method-seen");
    let stub = td.path().join("kit.sh");
    make_executable(
        &stub,
        &format!(
            r#"#!/bin/sh
marker={marker}
while IFS= read -r line; do
  case "$line" in
    *initialize*)
      printf '%s\n' '{{"jsonrpc":"2.0","id":1,"result":{{"name":"stub-kit","protocol_version":"pep/1.7.0","capabilities":{{}}}}}}'
      ;;
    *provekit.plugin.kit_declaration*)
      printf '%s' "$line" > "$marker"
      printf '%s\n' '{{"jsonrpc":"2.0","id":2,"result":{{"kit":{{"id":"stub-kit","language":"rust","version":"0.1.0"}},"rpc":{{"methods":[{{"name":"provekit.plugin.kit_declaration","required":true}}]}},"proofResolution":{{"strategy":"rpc-proof-bytes","rpcMethod":"provekit.plugin.resolve_dependency_proofs"}},"effectKinds":["concept:panic-freedom"],"effectLeaves":[{{"surface":"rust-implications","local":"method:unwrap","concept":"concept:panic-freedom.leaf.unwrap"}}],"guardPredicates":[],"controlCarriers":[],"residueCategories":[]}}}}'
      ;;
    *shutdown*)
      printf '%s\n' '{{"jsonrpc":"2.0","id":3,"result":null}}'
      exit 0
      ;;
  esac
done
"#,
            marker = marker.display()
        ),
    );

    let declaration: KitDeclaration =
        provekit_cli::kit_declaration::load_kit_declaration_with_command(
            &[stub.display().to_string()],
            Some(td.path()),
        )
        .expect("load declaration");

    assert_eq!(declaration.kit.id, "stub-kit");
    assert_eq!(declaration.effect_kinds, ["concept:panic-freedom"]);
    assert!(
        fs::read_to_string(marker)
            .expect("marker")
            .contains(KIT_DECLARATION_RPC_METHOD),
        "loader must request the dedicated declaration RPC method"
    );
}

#[test]
fn loader_rejects_conflicting_declaration_mappings() {
    let td = TempDir::new().expect("tempdir");
    let stub = td.path().join("kit.sh");
    make_executable(
        &stub,
        r#"#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *initialize*)
      printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"name":"stub-kit","protocol_version":"pep/1.7.0","capabilities":{}}}'
      ;;
    *provekit.plugin.kit_declaration*)
      printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"kit":{"id":"stub-kit","language":"rust","version":"0.1.0"},"rpc":{"methods":[{"name":"provekit.plugin.kit_declaration","required":true}]},"proofResolution":{"strategy":"rpc-proof-bytes"},"effectKinds":["concept:panic-freedom"],"effectLeaves":[{"surface":"rust-implications","local":"method:unwrap","concept":"concept:panic-freedom.leaf.unwrap"},{"surface":"rust-implications","local":"method:unwrap","concept":"concept:panic-freedom.leaf.expect"}],"guardPredicates":[],"controlCarriers":[],"residueCategories":[]}}'
      ;;
  esac
done
"#,
    );

    let err = provekit_cli::kit_declaration::load_kit_declaration_with_command(
        &[stub.display().to_string()],
        Some(td.path()),
    )
    .expect_err("conflicting declaration should fail");

    assert!(
        err.to_string().contains("effectLeaves"),
        "error should identify declaration conflict: {err}"
    );
}

#[test]
fn loader_reports_missing_kit_declaration_method() {
    let td = TempDir::new().expect("tempdir");
    let stub = td.path().join("kit.sh");
    make_executable(
        &stub,
        r#"#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *initialize*)
      printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"name":"stub-kit","protocol_version":"pep/1.7.0","capabilities":{}}}'
      ;;
    *provekit.plugin.kit_declaration*)
      printf '%s\n' '{"jsonrpc":"2.0","id":2,"error":{"code":-32601,"message":"method not found: provekit.plugin.kit_declaration"}}'
      ;;
  esac
done
"#,
    );

    let err = provekit_cli::kit_declaration::load_kit_declaration_with_command(
        &[stub.display().to_string()],
        Some(td.path()),
    )
    .expect_err("missing declaration method should fail");

    assert!(
        err.to_string().contains(KIT_DECLARATION_RPC_METHOD),
        "error should name missing RPC method: {err}"
    );
}

#[test]
fn loader_accepts_empty_effect_kinds_for_emit_only_kit() {
    let td = TempDir::new().expect("tempdir");
    let stub = td.path().join("kit.sh");
    make_executable(
        &stub,
        r#"#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *initialize*)
      printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"name":"python-hypothesis","protocol_version":"pep/1.7.0","capabilities":{}}}'
      ;;
    *provekit.plugin.kit_declaration*)
      printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"kit":{"id":"python-hypothesis","language":"python","version":"0.1.0"},"rpc":{"methods":[{"name":"initialize","required":true},{"name":"provekit.plugin.kit_declaration","required":true},{"name":"provekit.plugin.invoke","required":true},{"name":"provekit.plugin.check","required":false},{"name":"provekit.plugin.shutdown","required":false}]},"proofResolution":{"strategy":"pip"},"effectKinds":[],"effectLeaves":[],"guardPredicates":[],"controlCarriers":[],"residueCategories":[]}}'
      ;;
    *shutdown*)
      printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":null}'
      exit 0
      ;;
  esac
done
"#,
    );

    let declaration: KitDeclaration =
        provekit_cli::kit_declaration::load_kit_declaration_with_command(
            &[stub.display().to_string()],
            Some(td.path()),
        )
        .expect("load declaration");

    assert_eq!(declaration.kit.id, "python-hypothesis");
    assert_eq!(declaration.kit.language, "python");
    assert!(declaration.effect_kinds.is_empty());
    assert_eq!(declaration.proof_resolution.strategy, "pip");
}

#[test]
fn loader_accepts_empty_effect_kinds_for_python_lift_kit() {
    let td = TempDir::new().expect("tempdir");
    let stub = td.path().join("kit.sh");
    make_executable(
        &stub,
        r#"#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *initialize*)
      printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"name":"provekit-lsp-python","version":"0.1.0","protocol_version":"provekit-lsp-shared/1","kit_id":"python","capabilities":{}}}'
      ;;
    *provekit.plugin.kit_declaration*)
      printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"kit":{"id":"python","language":"python","version":"0.1.0"},"rpc":{"methods":[{"name":"initialize","required":true},{"name":"provekit.plugin.kit_declaration","required":true},{"name":"analyzeDocument","required":false},{"name":"parse","required":false},{"name":"lift","required":true},{"name":"provekit.plugin.lift_implications","required":false},{"name":"shutdown","required":false}]},"proofResolution":{"strategy":"pip"},"effectKinds":[],"effectLeaves":[],"guardPredicates":[],"controlCarriers":[],"residueCategories":[]}}'
      ;;
    *shutdown*)
      printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":null}'
      exit 0
      ;;
  esac
done
"#,
    );

    let declaration: KitDeclaration =
        provekit_cli::kit_declaration::load_kit_declaration_with_command(
            &[stub.display().to_string()],
            Some(td.path()),
        )
        .expect("load declaration");

    assert_eq!(declaration.kit.id, "python");
    assert_eq!(declaration.kit.language, "python");
    assert!(declaration.effect_kinds.is_empty());
    assert_eq!(declaration.proof_resolution.strategy, "pip");
    assert!(declaration
        .rpc
        .methods
        .iter()
        .any(|method| method.name == "provekit.plugin.lift_implications"));
}

#[test]
fn loader_rejects_kit_declaration_response_id_mismatch() {
    let td = TempDir::new().expect("tempdir");
    let stub = td.path().join("kit.sh");
    make_executable(
        &stub,
        r#"#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *initialize*)
      printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"name":"stub-kit","protocol_version":"pep/1.7.0","capabilities":{}}}'
      ;;
    *provekit.plugin.kit_declaration*)
      printf '%s\n' '{"jsonrpc":"2.0","id":99,"result":{"kit":{"id":"stub-kit","language":"rust","version":"0.1.0"},"rpc":{"methods":[{"name":"provekit.plugin.kit_declaration","required":true}]},"proofResolution":{"strategy":"rpc-proof-bytes"},"effectKinds":["concept:panic-freedom"],"effectLeaves":[],"guardPredicates":[],"controlCarriers":[],"residueCategories":[]}}'
      ;;
  esac
done
"#,
    );

    let err = provekit_cli::kit_declaration::load_kit_declaration_with_command(
        &[stub.display().to_string()],
        Some(td.path()),
    )
    .expect_err("response id mismatch should fail");

    assert!(
        err.to_string().contains("response id mismatch"),
        "error should describe mismatched response id: {err}"
    );
}
