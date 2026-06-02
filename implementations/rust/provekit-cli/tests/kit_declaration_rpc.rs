use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use tempfile::TempDir;

use provekit_claim_envelope::{KitDeclaration, KIT_DECLARATION_RPC_METHOD};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .to_path_buf()
}

fn typescript_env_enabled() -> bool {
    std::env::var("BCARGO_TYPESCRIPT_ENV").map_or(false, |value| value == "1")
}

fn command_available(command: &str) -> bool {
    std::process::Command::new(command)
        .arg("version")
        .output()
        .map_or(false, |output| output.status.success())
}

fn run_with_timeout(cmd: &mut Command, timeout: Duration) -> std::io::Result<Output> {
    let mut child = cmd.spawn()?;
    let started = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            return child.wait_with_output();
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            return child.wait_with_output();
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn java_bin() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(java_home) = std::env::var_os("JAVA_HOME") {
        candidates.push(PathBuf::from(java_home).join("bin").join("java"));
    }
    candidates.push(PathBuf::from("java"));
    candidates.push(PathBuf::from(
        "/usr/local/opt/openjdk/libexec/openjdk.jdk/Contents/Home/bin/java",
    ));
    candidates.push(PathBuf::from(
        "/opt/homebrew/opt/openjdk/libexec/openjdk.jdk/Contents/Home/bin/java",
    ));

    candidates.into_iter().find(|candidate| {
        run_with_timeout(
            Command::new(candidate).arg("-version"),
            Duration::from_secs(5),
        )
        .map(|output| output.status.success())
        .unwrap_or(false)
    })
}

fn maven_available() -> bool {
    run_with_timeout(Command::new("mvn").arg("-version"), Duration::from_secs(8))
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn java_source_kit_command() -> Option<Vec<String>> {
    static JAVA_SOURCE_KIT_COMMAND: OnceLock<Option<Vec<String>>> = OnceLock::new();
    JAVA_SOURCE_KIT_COMMAND
        .get_or_init(|| {
            let Some(java) = java_bin() else {
                eprintln!("skipping: no working java binary found");
                return None;
            };
            if !maven_available() {
                eprintln!("skipping: mvn is unavailable");
                return None;
            }

            let root = repo_root();
            let java_root = root.join("implementations").join("java");
            let mut mvn = Command::new("mvn");
            mvn.current_dir(&java_root).args([
                "-B",
                "-ntp",
                "-pl",
                "provekit-lift-java-source",
                "-am",
                "-DskipTests",
                "package",
            ]);
            let out =
                run_with_timeout(&mut mvn, Duration::from_secs(120)).expect("spawn mvn package");
            assert!(
                out.status.success(),
                "mvn package provekit-lift-java-source failed\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );

            let jar = java_root
                .join("provekit-lift-java-source")
                .join("target")
                .join("provekit-lift-java-source.jar");
            assert!(
                jar.exists(),
                "maven build produced no jar at {}",
                jar.display()
            );
            Some(vec![
                java.display().to_string(),
                "-jar".to_string(),
                jar.display().to_string(),
                "--rpc".to_string(),
            ])
        })
        .clone()
}

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

fn shell_stub_command(path: &Path) -> Vec<String> {
    vec!["sh".to_string(), path.display().to_string()]
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
            &shell_stub_command(&stub),
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
        &shell_stub_command(&stub),
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
        &shell_stub_command(&stub),
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
            &shell_stub_command(&stub),
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
            &shell_stub_command(&stub),
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
fn loader_dispatches_to_typescript_source_kit_declaration() {
    if !typescript_env_enabled() {
        eprintln!("skipping: BCARGO_TYPESCRIPT_ENV is not enabled");
        return;
    }

    let repo = repo_root();
    let typescript_dir = repo.join("implementations/typescript");
    let command = [
        "npx".to_string(),
        "tsx".to_string(),
        "src/lift/typescript-source/bin.ts".to_string(),
        "--rpc".to_string(),
    ];

    let declaration: KitDeclaration =
        provekit_cli::kit_declaration::load_kit_declaration_with_command(
            &command,
            Some(&typescript_dir),
        )
        .expect("load TypeScript source kit declaration");

    assert_eq!(declaration.kit.id, "typescript-source");
    assert_eq!(declaration.kit.language, "typescript");
    assert_eq!(declaration.kit.version, "0.1.0-draft");
    assert_eq!(declaration.proof_resolution.strategy, "npm");
    assert_eq!(declaration.effect_kinds, ["concept:panic-freedom"]);
    assert_eq!(declaration.effect_leaves.len(), 1);
    assert_eq!(
        declaration.effect_leaves[0].surface.as_deref(),
        Some("typescript-source")
    );
    assert_eq!(declaration.effect_leaves[0].local, "ts:throw");
    assert_eq!(
        declaration.effect_leaves[0].concept,
        "concept:panic-freedom.leaf.runtime-failure-site"
    );
    assert!(declaration.guard_predicates.is_empty());
    assert!(declaration.control_carriers.is_empty());
    assert!(declaration.residue_categories.is_empty());

    let required_by_name = declaration
        .rpc
        .methods
        .iter()
        .map(|method| (method.name.as_str(), method.required))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        required_by_name,
        std::collections::BTreeMap::from([
            ("initialize", true),
            (KIT_DECLARATION_RPC_METHOD, true),
            ("lift", true),
            ("compile", false),
            ("provekit.plugin.recognize", false),
            ("shutdown", false),
        ])
    );
}

#[test]
fn loader_dispatches_to_go_source_kit_declaration() {
    if !command_available("go") {
        eprintln!("skipping: go is not available");
        return;
    }

    let repo = repo_root();
    let go_source_dir = repo.join("implementations/go/provekit-lift-go");
    let command = [
        "go".to_string(),
        "run".to_string(),
        "./cmd/provekit-lift-go".to_string(),
        "--rpc".to_string(),
    ];

    let declaration: KitDeclaration =
        provekit_cli::kit_declaration::load_kit_declaration_with_command(
            &command,
            Some(&go_source_dir),
        )
        .expect("load Go source kit declaration");

    assert_eq!(declaration.kit.id, "go-source");
    assert_eq!(declaration.kit.language, "go");
    assert_eq!(declaration.kit.version, "0.1.0-draft");
    assert_eq!(declaration.proof_resolution.strategy, "go-mod");
    assert_eq!(declaration.effect_kinds, ["concept:panic-freedom"]);
    assert_eq!(declaration.effect_leaves.len(), 1);
    assert_eq!(
        declaration.effect_leaves[0].surface.as_deref(),
        Some("go-source")
    );
    assert_eq!(declaration.effect_leaves[0].local, "go:panic");
    assert_eq!(
        declaration.effect_leaves[0].concept,
        "concept:panic-freedom.leaf.runtime-failure-site"
    );
    assert!(declaration.guard_predicates.is_empty());
    assert!(declaration.control_carriers.is_empty());
    assert!(declaration.residue_categories.is_empty());

    let required_by_name = declaration
        .rpc
        .methods
        .iter()
        .map(|method| (method.name.as_str(), method.required))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        required_by_name,
        std::collections::BTreeMap::from([
            ("initialize", true),
            (KIT_DECLARATION_RPC_METHOD, true),
            ("lift", true),
            ("provekit.plugin.lift_implications", false),
            ("compile", false),
            ("provekit.plugin.recognize", false),
            ("shutdown", false),
        ])
    );
}

#[test]
fn loader_dispatches_to_go_verify_kit_declaration() {
    if !command_available("go") {
        eprintln!("skipping: go is not available");
        return;
    }

    let repo = repo_root();
    let go_dir = repo.join("implementations/go");
    let command = [
        "go".to_string(),
        "run".to_string(),
        "./cmd/provekit-lift-go-verify".to_string(),
        "--rpc".to_string(),
    ];

    let declaration: KitDeclaration =
        provekit_cli::kit_declaration::load_kit_declaration_with_command(&command, Some(&go_dir))
            .expect("load Go verify-facing kit declaration");

    assert_eq!(declaration.kit.id, "go");
    assert_eq!(declaration.kit.language, "go");
    assert_eq!(declaration.kit.version, "0.1.0");
    assert_eq!(declaration.proof_resolution.strategy, "go-mod");
    assert_eq!(declaration.effect_kinds, ["concept:panic-freedom"]);
    assert_eq!(declaration.effect_leaves.len(), 1);
    assert_eq!(declaration.effect_leaves[0].surface.as_deref(), Some("go"));
    assert_eq!(declaration.effect_leaves[0].local, "go:panic");
    assert_eq!(
        declaration.effect_leaves[0].concept,
        "concept:panic-freedom.leaf.runtime-failure-site"
    );
    assert!(declaration.guard_predicates.is_empty());
    assert!(declaration.control_carriers.is_empty());
    assert!(declaration.residue_categories.is_empty());

    let required_by_name = declaration
        .rpc
        .methods
        .iter()
        .map(|method| (method.name.as_str(), method.required))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        required_by_name,
        std::collections::BTreeMap::from([
            ("initialize", true),
            (KIT_DECLARATION_RPC_METHOD, true),
            ("lift", true),
            ("shutdown", false),
        ])
    );
}

#[test]
fn loader_dispatches_to_java_source_kit_declaration() {
    let Some(command) = java_source_kit_command() else {
        return;
    };

    let repo = repo_root();
    let java_source_dir = repo.join("implementations/java");

    let declaration: KitDeclaration =
        provekit_cli::kit_declaration::load_kit_declaration_with_command(
            &command,
            Some(&java_source_dir),
        )
        .expect("load Java source kit declaration");

    assert_eq!(declaration.kit.id, "java-source");
    assert_eq!(declaration.kit.language, "java");
    assert_eq!(declaration.kit.version, "0.1.0");
    assert_eq!(declaration.proof_resolution.strategy, "maven");
    assert_eq!(declaration.effect_kinds, ["concept:panic-freedom"]);
    assert_eq!(declaration.effect_leaves.len(), 1);
    assert_eq!(
        declaration.effect_leaves[0].surface.as_deref(),
        Some("java-source")
    );
    assert_eq!(declaration.effect_leaves[0].local, "java:throw");
    assert_eq!(
        declaration.effect_leaves[0].concept,
        "concept:panic-freedom.leaf.runtime-failure-site"
    );
    assert!(declaration.guard_predicates.is_empty());
    assert!(declaration.control_carriers.is_empty());
    assert!(declaration.residue_categories.is_empty());

    let required_by_name = declaration
        .rpc
        .methods
        .iter()
        .map(|method| (method.name.as_str(), method.required))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        required_by_name,
        std::collections::BTreeMap::from([
            ("initialize", true),
            (KIT_DECLARATION_RPC_METHOD, true),
            ("lift", true),
            ("shutdown", false),
        ])
    );
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
        &shell_stub_command(&stub),
        Some(td.path()),
    )
    .expect_err("response id mismatch should fail");

    assert!(
        err.to_string().contains("response id mismatch"),
        "error should describe mismatched response id: {err}"
    );
}
