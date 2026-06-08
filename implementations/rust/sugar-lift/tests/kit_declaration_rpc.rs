use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use serde_json::{json, Value};
use sugar_claim_envelope::{KitDeclaration, KitDeclarationMapping, KIT_DECLARATION_RPC_METHOD};

const RUST_CONTRACTS_SURFACE: &str = "rust-contracts";

#[test]
fn lift_rpc_serves_rust_contracts_kit_declaration() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_provekit-lift"))
        .arg("--rpc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn provekit-lift --rpc");
    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let mut reader = BufReader::new(stdout);

    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}})
    )
    .expect("write initialize");
    let init_response = read_response(&mut reader);
    assert!(
        init_response.get("error").is_none(),
        "initialize failed: {init_response}"
    );

    writeln!(
        stdin,
        "{}",
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": KIT_DECLARATION_RPC_METHOD,
            "params": {}
        })
    )
    .expect("write kit declaration request");
    let declaration_response = read_response(&mut reader);
    assert!(
        declaration_response.get("error").is_none(),
        "kit declaration RPC failed: {declaration_response}"
    );

    let declaration: KitDeclaration = serde_json::from_value(
        declaration_response
            .get("result")
            .cloned()
            .expect("kit declaration result"),
    )
    .expect("kit declaration schema");
    declaration.validate().expect("valid declaration");

    assert_eq!(declaration.kit.id, "provekit-lift");
    assert_eq!(declaration.kit.language, "rust");
    assert_method_declared(&declaration, "initialize");
    assert_method_declared(&declaration, "lift");
    assert_method_declared(&declaration, "shutdown");
    assert_method_declared(&declaration, KIT_DECLARATION_RPC_METHOD);
    assert_eq!(declaration.proof_resolution.strategy, "cargo");
    assert_eq!(declaration.effect_kinds, ["concept:panic-freedom"]);
    assert!(
        declaration.effect_leaves.is_empty(),
        "provekit-lift/contracts must not claim panic leaves owned by walk_rpc: {:?}",
        declaration.effect_leaves
    );
    assert_kit_declaration_mappings(
        "guardPredicates",
        &declaration.guard_predicates,
        &[
            ("is_ok", "concept:panic-freedom.result.ok"),
            ("is_err", "concept:panic-freedom.result.err"),
            ("is_some", "concept:panic-freedom.option.some"),
            ("is_none", "concept:panic-freedom.option.none"),
        ],
    );
    assert!(
        declaration.control_carriers.is_empty(),
        "provekit-lift/contracts must not claim control carriers owned by walk_rpc: {:?}",
        declaration.control_carriers
    );

    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc":"2.0","id":3,"method":"shutdown","params":{}})
    )
    .expect("write shutdown");
    let shutdown_response = read_response(&mut reader);
    assert!(
        shutdown_response.get("error").is_none(),
        "shutdown failed: {shutdown_response}"
    );
    drop(stdin);
    let status = child.wait().expect("wait for provekit-lift");
    assert!(status.success(), "provekit-lift exited with {status}");
}

fn read_response(reader: &mut BufReader<std::process::ChildStdout>) -> Value {
    let mut line = String::new();
    reader.read_line(&mut line).expect("read response");
    assert!(!line.is_empty(), "child closed stdout before response");
    serde_json::from_str(&line).expect("JSON-RPC response")
}

fn assert_method_declared(declaration: &KitDeclaration, method: &str) {
    assert!(
        declaration
            .rpc
            .methods
            .iter()
            .any(|declared| declared.name == method),
        "declaration must advertise {method}: {:?}",
        declaration.rpc.methods
    );
}

fn assert_kit_declaration_mappings(
    label: &str,
    mappings: &[KitDeclarationMapping],
    expected: &[(&str, &str)],
) {
    for mapping in mappings {
        assert_eq!(
            mapping.surface.as_deref(),
            Some(RUST_CONTRACTS_SURFACE),
            "{label} mapping must be scoped to rust-contracts: {mapping:?}"
        );
    }
    for (local, concept) in expected {
        assert!(
            mappings.iter().any(|mapping| mapping.local == *local
                && mapping.concept == *concept
                && mapping.surface.as_deref() == Some(RUST_CONTRACTS_SURFACE)),
            "{label} must include {local} -> {concept}: {mappings:?}"
        );
    }
    assert_eq!(
        mappings.len(),
        expected.len(),
        "{label} should not claim extra mappings: {mappings:?}"
    );
}
