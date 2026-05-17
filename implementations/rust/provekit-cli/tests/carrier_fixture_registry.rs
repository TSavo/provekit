// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use libprovekit::core::{ConformanceDeclaration, KitRegistry, LowerKit};
use provekit_cli::kit_dispatch::DispatchRealizeTransport;

const REQUIRED_FIXTURES: &[&str] = &[
    "arithmetic_add",
    "control_flow_if",
    "hello_world",
    "recursive_factorial",
    "transported_op_via_concept_citation_comment",
];

#[test]
fn lower_java_carrier_registration_points_at_required_fixture_set() {
    let repo_root = repo_root();
    let fixtures_path = repo_root.join("implementations/java/conformance/fixtures");
    let mut registry = KitRegistry::default();

    registry.register_with_platform_semantics(
        "lower-java",
        LowerKit::new(repo_root, "java", None, DispatchRealizeTransport),
        "java",
        fixtures_path.clone(),
    );

    let Some(ConformanceDeclaration::Carrier {
        fixtures_path,
        platform_semantics,
    }) = registry.conformance("lower-java")
    else {
        panic!("lower-java must register as a carrier");
    };
    assert_eq!(platform_semantics, &None);
    assert_required_fixtures(fixtures_path);
}

fn assert_required_fixtures(fixtures_path: &Path) {
    assert!(
        fixtures_path.is_dir(),
        "carrier fixtures_path must resolve to a directory: {}",
        fixtures_path.display()
    );

    let present = std::fs::read_dir(fixtures_path)
        .unwrap_or_else(|err| panic!("read {}: {err}", fixtures_path.display()))
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect::<BTreeSet<_>>();

    for required in REQUIRED_FIXTURES {
        assert!(
            present.contains(*required),
            "carrier fixture set at {} is missing `{required}`; present: {present:?}",
            fixtures_path.display()
        );
        let fixture_json = fixtures_path.join(required).join("fixture.json");
        assert!(
            fixture_json.is_file(),
            "carrier fixture `{required}` must include fixture.json at {}",
            fixture_json.display()
        );
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}
