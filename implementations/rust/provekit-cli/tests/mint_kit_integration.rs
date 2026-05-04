// SPDX-License-Identifier: Apache-2.0
//
// mint_kit_integration — integration test for `provekit mint --kit=<kit>`.
//
// Verifies that the unified mint pipeline:
//   1. Produces a valid signed attestation JSON for all 11 kits.
//   2. Kits with a real lifter binary produce a non-empty contractSetCid
//      (not the empty-set sentinel).
//   3. Kits without a lifter binary produce the empty-set CID.
//   4. All attestations pass structural validation (required fields, types).
//   5. Two consecutive mints for the same kit produce byte-identical
//      attestations (determinism).
//
// The test runs `provekit mint --kit=<kit> --quiet` as a subprocess against
// a per-test scratch repo (issue #218). The scratch repo mirrors the
// canonical repo layout via top-level symlinks, with a writable
// `.provekit/self-contracts-attestations/` directory inside the scratch.
// Mint writes attestations into the scratch, so the canonical working
// tree stays byte-identical across the entire test run.
//
// This test requires:
//   - The `provekit` binary to be built (release or debug).
//   - The canonical repo to be at the location resolved by CARGO_MANIFEST_DIR.

use serial_test::serial;
use std::path::{Path, PathBuf};
use std::process::Command;

/// BLAKE3-512 of JCS(`[]`) — the empty-set contractSetCid.
/// Produced by `compute_contract_set_cid(vec![])`. Verified empirically.
const EMPTY_SET_CID: &str = "blake3-512:d53d18c23212ea7b6300594bb89bce60218f6eff2b9d628b8cc42d3e79bbd5ab09994845815cc7185113418f9fc2edc7606b06f0d57a6d581e7cff5b290f3229";

/// Return the path to the `provekit` binary (release or debug, whichever exists).
fn provekit_bin() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace = PathBuf::from(manifest_dir).parent().unwrap().to_path_buf();
    let release = workspace.join("target").join("release").join("provekit");
    let debug = workspace.join("target").join("debug").join("provekit");
    if release.exists() {
        release
    } else {
        debug
    }
}

/// Return the canonical repo root (two levels above the workspace root).
///
/// implementations/rust/ is the workspace; the repo root contains
/// `.provekit/self-contracts-attestations/` and `implementations/`.
fn canonical_repo_root() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    // CARGO_MANIFEST_DIR = .../implementations/rust/provekit-cli
    // parent = .../implementations/rust
    // parent.parent = .../implementations
    // parent.parent.parent = repo root
    PathBuf::from(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

// ---------------------------------------------------------------------------
// Per-test scratch repo (issue #218)
// ---------------------------------------------------------------------------
//
// The mint pipeline writes its signed attestation to
// `<repo-root>/.provekit/self-contracts-attestations/<lang>.json` by walking
// up from the project_root and looking for that directory. Without
// isolation, every test run rewrites attestation files for every kit,
// leaving the working tree dirty.
//
// `ScratchRepo` builds a temp directory mirroring the canonical layout via
// symlinks for read-only state and a real writable
// `.provekit/self-contracts-attestations/` directory. The kit project
// directories (`implementations/<lang>/`) are themselves real directories
// with their **contents** symlinked from canonical, which is critical:
// `find_attestation_dir` calls `canonicalize()` on the project_root, which
// would follow a symlinked project root back into the canonical tree and
// write the real attestation file. Real-dir-with-symlinked-contents stops
// the canonicalize traversal at the scratch boundary.

struct ScratchRepo {
    dir: tempfile::TempDir,
}

impl ScratchRepo {
    /// Build a scratch repo mirroring all 11 kit implementation dirs.
    fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("provekit-mint-test-")
            .tempdir()
            .expect("create scratch tempdir");

        let canonical = canonical_repo_root();
        let scratch_root = dir.path();

        // Pre-create the writable attestation directory at the scratch root
        // so the mint pipeline's walk-up terminates inside the scratch.
        let attest_dir = scratch_root
            .join(".provekit")
            .join("self-contracts-attestations");
        std::fs::create_dir_all(&attest_dir)
            .expect("create scratch .provekit/self-contracts-attestations");

        // Mirror every kit project dir as a real directory whose entries are
        // symlinks to the canonical kit's entries.
        let kits: &[&str] = &[
            "rust",
            "go",
            "cpp",
            "typescript",
            "csharp",
            "swift",
            "java",
            "python",
            "ruby",
            "zig",
            "c",
        ];
        let scratch_impls = scratch_root.join("implementations");
        std::fs::create_dir_all(&scratch_impls)
            .expect("create scratch implementations dir");
        for kit_dir_name in kits {
            let canonical_kit = canonical.join("implementations").join(kit_dir_name);
            if !canonical_kit.exists() {
                // Skip kits that don't exist in this checkout — the test will
                // surface that gap via mint's ENOENT-empty-set behavior.
                continue;
            }
            let scratch_kit = scratch_impls.join(kit_dir_name);
            std::fs::create_dir_all(&scratch_kit)
                .unwrap_or_else(|e| panic!("create {}: {e}", scratch_kit.display()));
            symlink_contents(&canonical_kit, &scratch_kit);
        }

        ScratchRepo { dir }
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }
}

/// Symlink every top-level entry from `src` into `dst`. `dst` must already exist
/// as a real directory. This breaks the canonicalize-traversal at `dst`'s
/// boundary while still giving the lifter access to read-only source files
/// and pre-built binaries (e.g. `./target/release/<lifter>`).
fn symlink_contents(src: &Path, dst: &Path) {
    let entries = std::fs::read_dir(src)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", src.display()));
    for entry in entries {
        let entry = entry.expect("dir entry");
        let name = entry.file_name();
        let from = src.join(&name);
        let to = dst.join(&name);
        // Skip if it already exists (idempotent).
        if to.exists() || to.is_symlink() {
            continue;
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&from, &to)
            .unwrap_or_else(|e| panic!("symlink {} -> {}: {e}", to.display(), from.display()));
        #[cfg(windows)]
        {
            // Windows symlinks need the right kind. Tests don't run on
            // Windows in CI today, but keep the code compilable.
            if from.is_dir() {
                std::os::windows::fs::symlink_dir(&from, &to)
                    .unwrap_or_else(|e| panic!("symlink_dir {}: {e}", to.display()));
            } else {
                std::os::windows::fs::symlink_file(&from, &to)
                    .unwrap_or_else(|e| panic!("symlink_file {}: {e}", to.display()));
            }
        }
    }
}

/// Run `provekit mint --kit=<kit> --quiet` from `root`.
/// Returns (exit_status, stdout, stderr).
fn run_mint(root: &Path, kit: &str) -> (bool, String, String) {
    let bin = provekit_bin();
    let out = Command::new(&bin)
        .arg("mint")
        .arg(format!("--kit={kit}"))
        .arg("--quiet")
        .current_dir(root)
        .output()
        .expect("failed to spawn provekit");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

/// Read the attestation JSON from `<root>/.provekit/self-contracts-attestations/<lang>.json`.
fn read_attestation(root: &Path, lang: &str) -> serde_json::Value {
    let path = root
        .join(".provekit")
        .join("self-contracts-attestations")
        .join(format!("{lang}.json"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// Assert that an attestation JSON has all required fields with correct types.
fn assert_attestation_structure(v: &serde_json::Value, lang: &str) {
    assert_eq!(
        v["schemaVersion"].as_str(),
        Some("1"),
        "{lang}: schemaVersion must be '1'"
    );
    assert_eq!(
        v["kind"].as_str(),
        Some("self-contracts-attestation"),
        "{lang}: kind must be 'self-contracts-attestation'"
    );
    assert_eq!(
        v["lang"].as_str(),
        Some(lang),
        "{lang}: lang field must match"
    );
    let cset = v["contractSetCid"].as_str().unwrap_or_else(|| {
        panic!("{lang}: contractSetCid must be a string")
    });
    assert!(
        cset.starts_with("blake3-512:"),
        "{lang}: contractSetCid must start with 'blake3-512:', got {cset}"
    );
    let sig = v["signature"].as_str().unwrap_or_else(|| {
        panic!("{lang}: signature must be a string")
    });
    assert!(
        sig.starts_with("ed25519:"),
        "{lang}: signature must start with 'ed25519:', got {sig}"
    );
    let signer = v["signer"].as_str().unwrap_or_else(|| {
        panic!("{lang}: signer must be a string")
    });
    assert!(
        signer.starts_with("ed25519:"),
        "{lang}: signer must start with 'ed25519:', got {signer}"
    );
    let declared_at = v["declaredAt"].as_str().unwrap_or_else(|| {
        panic!("{lang}: declaredAt must be a string")
    });
    assert!(
        !declared_at.is_empty(),
        "{lang}: declaredAt must be non-empty"
    );
}

// ---------------------------------------------------------------------------
// Test 1: All 11 kits produce a structurally valid attestation
// ---------------------------------------------------------------------------

/// Kits with a real lifter binary installed. These kits run lift-protocol RPCs.
/// Note: a kit having a lifter binary does not guarantee a non-empty contractSetCid;
/// the CID depends on how many contracts the lifter finds in the workspace.
///
/// `swift` is included unconditionally on macOS (the swift toolchain is
/// platform-restricted; see Package.swift `platforms: [.macOS(.v13)]`).
/// On Linux the swift release binary is absent and the dispatcher's ENOENT
/// fallback fires, producing an empty-set CID. The all-kits structure test
/// tolerates this; the pinned-CID test (`swift_kit_pins_expected_contract_set_cid`)
/// is `#[cfg_attr(not(target_os = "macos"), ignore)]` so it doesn't fail on Linux.
const KITS_WITH_LIFTERS: &[&str] = &["rust", "go", "cpp", "ts", "csharp", "swift", "java", "python"];

/// Kits without a lifter binary yet — produce the empty-set CID because the
/// binary cannot be found (ENOENT on spawn). These declare the binary name but
/// the binary is not installed; the gap surfaces as an empty-set attestation.
const KITS_WITHOUT_LIFTERS: &[&str] = &["ruby", "zig", "c"];

/// Kits that have a lifter AND are expected to find real contracts.
/// Only include kits where the test environment reliably has the lifter built
/// and the kit's workspace has liftable annotations.
///
/// `swift` is on macOS only; the all-kits run handles Linux gracefully via the
/// `failed_kits` skip path because the release binary is missing.
const KITS_WITH_REAL_CONTRACTS: &[&str] = &["rust", "go", "cpp", "python"];

/// Pinned contractSetCid for `--kit=go` after Tier 1 wiring fix (#176).
/// Reflects the 11 canonical contracts in `implementations/go/provekit-self-contracts/slabs/`.
/// Update this constant when contracts change (re-run `make mint-go` and capture the new CID).
const GO_CONTRACT_SET_CID: &str = "blake3-512:e23649f383162398556a508c2e69e035d6d231bfaf6e8926ced547fb19ddd9c65779f39fe31d85519c957bc40afa432c9be468eadfa5aac77f74f5de8c56324c";

#[test]
#[serial(mint_kit_files)]
fn all_kits_mint_produces_valid_attestation_structure() {
    let all_kits: Vec<&str> = KITS_WITH_LIFTERS
        .iter()
        .chain(KITS_WITHOUT_LIFTERS.iter())
        .copied()
        .collect();

    let scratch = ScratchRepo::new();
    let root = scratch.root();
    let mut failed_kits: Vec<String> = Vec::new();

    for kit in &all_kits {
        let (ok, stdout, stderr) = run_mint(root, kit);
        if !ok {
            // Kits with optional toolchain (dotnet, go, npx) may not be available.
            // Record the failure but don't fail the test outright for those kits.
            // Only the KITS_WITHOUT_LIFTERS (ENOENT-based) must succeed unconditionally.
            if KITS_WITHOUT_LIFTERS.contains(kit) {
                panic!(
                    "kit `{kit}`: mint exited non-zero (no-lifter kits must always succeed)\n  stdout: {stdout}\n  stderr: {stderr}"
                );
            }
            eprintln!(
                "kit `{kit}`: mint exited non-zero (toolchain may not be available)\n  stderr: {stderr}"
            );
            failed_kits.push(kit.to_string());
            continue;
        }
        // stdout in quiet mode: optional bundle CID line + contractSetCid line
        assert!(
            stdout.contains("contractSetCid:"),
            "kit `{kit}`: stdout must contain 'contractSetCid:'\n  stdout: {stdout}"
        );

        let lang = if *kit == "ts" { "ts" } else { kit };
        let attest = read_attestation(root, lang);
        assert_attestation_structure(&attest, lang);

        eprintln!(
            "kit={kit} contractSetCid={}",
            attest["contractSetCid"].as_str().unwrap_or("?")
        );
    }

    if !failed_kits.is_empty() {
        eprintln!(
            "NOTE: {} kits skipped due to missing toolchain: {:?}",
            failed_kits.len(),
            failed_kits
        );
    }
}

// ---------------------------------------------------------------------------
// Test 2: Kits without lifters produce the known empty-set CID
// ---------------------------------------------------------------------------

#[test]
#[serial(mint_kit_files)]
fn kits_without_lifters_produce_empty_set_cid() {
    let scratch = ScratchRepo::new();
    let root = scratch.root();
    for kit in KITS_WITHOUT_LIFTERS {
        let (ok, _, _) = run_mint(root, kit);
        assert!(ok, "kit `{kit}`: mint must succeed even without a lifter binary");

        let lang = if *kit == "ts" { "ts" } else { kit };
        let attest = read_attestation(root, lang);
        let cset = attest["contractSetCid"].as_str().unwrap();
        assert_eq!(
            cset, EMPTY_SET_CID,
            "kit `{kit}`: expected empty-set CID, got {cset}"
        );
        // cid field should be empty string (no bundle produced)
        let cid = attest["cid"].as_str().unwrap();
        assert!(
            cid.is_empty(),
            "kit `{kit}`: cid should be empty string when no lifter, got {cid}"
        );
    }
}

// ---------------------------------------------------------------------------
// Test 3: Kits with real contracts produce non-empty-set contractSetCid
// ---------------------------------------------------------------------------

#[test]
#[serial(mint_kit_files)]
fn kits_with_real_contracts_produce_nonempty_contract_set() {
    let scratch = ScratchRepo::new();
    let root = scratch.root();
    for kit in KITS_WITH_REAL_CONTRACTS {
        let (ok, _, stderr) = run_mint(root, kit);
        if !ok {
            eprintln!("kit `{kit}`: mint failed (lifter may not be built yet)\n  stderr: {stderr}");
            // Skip rather than fail — lifter may not be built in test environment.
            continue;
        }

        let lang = if *kit == "ts" { "ts" } else { kit };
        let attest = read_attestation(root, lang);
        let cset = attest["contractSetCid"].as_str().unwrap();
        assert_ne!(
            cset, EMPTY_SET_CID,
            "kit `{kit}`: expected non-empty contractSetCid when lifter finds real contracts"
        );
        // bundle CID should be non-empty
        let cid = attest["cid"].as_str().unwrap();
        assert!(
            cid.starts_with("blake3-512:"),
            "kit `{kit}`: cid should start with blake3-512:, got {cid}"
        );
        eprintln!("kit={kit} cid={cid}");
        eprintln!("kit={kit} contractSetCid={cset}");
    }
}

// ---------------------------------------------------------------------------
// Test 4: Rust kit is byte-deterministic across two consecutive mints
// ---------------------------------------------------------------------------

#[test]
#[serial(mint_kit_files)]
fn rust_kit_mint_is_byte_deterministic() {
    let scratch = ScratchRepo::new();
    let root = scratch.root();

    // First mint
    let (ok1, _, _) = run_mint(root, "rust");
    if !ok1 {
        eprintln!("rust kit: first mint failed — skipping determinism test (lifter not built)");
        return;
    }
    let attest1 = read_attestation(root, "rust");

    // Second mint
    let (ok2, _, _) = run_mint(root, "rust");
    assert!(ok2, "rust kit: second mint must succeed");
    let attest2 = read_attestation(root, "rust");

    assert_eq!(
        attest1, attest2,
        "rust kit: two consecutive mints must produce byte-identical attestations"
    );

    eprintln!(
        "rust determinism confirmed: contractSetCid={}",
        attest1["contractSetCid"].as_str().unwrap_or("?")
    );
}

// ---------------------------------------------------------------------------
// Test 5: --kit shortcut and --project are equivalent for rust
// ---------------------------------------------------------------------------

#[test]
#[serial(mint_kit_files)]
fn kit_shortcut_and_project_flag_are_equivalent() {
    let bin = provekit_bin();
    let scratch = ScratchRepo::new();
    let root = scratch.root();

    // Via --kit
    let kit_out = Command::new(&bin)
        .arg("mint")
        .arg("--kit=rust")
        .arg("--quiet")
        .arg("--no-attest")
        .current_dir(root)
        .output()
        .expect("spawn provekit --kit=rust");

    // Via --project (must use rust-self-contracts surface to match --kit=rust;
    // issue #176 Tier 1: --kit=rust routes to rust-self-contracts, not rust)
    let proj_out = Command::new(&bin)
        .arg("mint")
        .arg("--project")
        .arg("implementations/rust")
        .arg("--surface")
        .arg("rust-self-contracts")
        .arg("--quiet")
        .arg("--no-attest")
        .current_dir(root)
        .output()
        .expect("spawn provekit --project");

    if !kit_out.status.success() || !proj_out.status.success() {
        eprintln!("rust lifter not available — skipping equivalence test");
        return;
    }

    let kit_stdout = String::from_utf8_lossy(&kit_out.stdout).to_string();
    let proj_stdout = String::from_utf8_lossy(&proj_out.stdout).to_string();

    // Both should produce the same contractSetCid line.
    let extract_cset = |s: &String| -> String {
        s.lines()
            .find(|l| l.starts_with("contractSetCid:"))
            .unwrap_or("")
            .to_string()
    };
    assert_eq!(
        extract_cset(&kit_stdout),
        extract_cset(&proj_stdout),
        "--kit and --project must produce identical contractSetCid output"
    );
}

// ---------------------------------------------------------------------------
// Test 6: --kit=go pins the expected contractSetCid (Tier 1 wiring fix #176)
// ---------------------------------------------------------------------------

/// Verify that `--kit=go` routes to the go-self-contracts surface (not the
/// test-fixture lifter) and that the resulting contractSetCid matches the
/// known-good CID computed from the 11 canonical contracts in the go slab.
///
/// If this test fails with the old empty-set CID (`d53d18c2...`), the KIT_TABLE
/// routing regression has been reintroduced. If it fails with an unknown CID,
/// the go slab contracts have changed -- update GO_CONTRACT_SET_CID accordingly.
#[test]
#[serial(mint_kit_files)]
fn go_kit_pins_expected_contract_set_cid() {
    let scratch = ScratchRepo::new();
    let root = scratch.root();

    let (ok, stdout, stderr) = run_mint(root, "go");
    if !ok {
        eprintln!("go kit: mint failed (go toolchain may not be available)\n  stderr: {stderr}");
        // Skip rather than fail -- go toolchain may not be present in all environments.
        return;
    }

    assert!(
        stdout.contains("contractSetCid:"),
        "go kit: stdout must contain 'contractSetCid:'\n  stdout: {stdout}"
    );

    let attest = read_attestation(root, "go");
    let cset = attest["contractSetCid"].as_str().unwrap();

    assert_ne!(
        cset, EMPTY_SET_CID,
        "go kit: contractSetCid must NOT be the empty-set sentinel -- routing regression detected"
    );
    assert_eq!(
        cset, GO_CONTRACT_SET_CID,
        "go kit: contractSetCid does not match pinned value from 11-contract slab (issue #176 Tier 1)"
    );

    eprintln!("go kit contractSetCid pinned correctly: {cset}");
}

// ---------------------------------------------------------------------------
// Test 7: rust kit contractSetCid is pinned to the canonical self-contracts CID
//         (issue #176 Tier 1 regression gate, PR #183)
// ---------------------------------------------------------------------------

/// Pinned contractSetCid produced by `--kit=rust` after routing to the
/// `rust-self-contracts` surface (mint-self-contracts binary, canonical
/// 11-contract slab). Any change to this value means either the surface
/// wiring changed or the canonical contracts changed — both require explicit
/// review and re-pinning.
///
/// This constant must be updated whenever the canonical self-contracts slab is
/// intentionally changed. It MUST NOT be the empty-set CID (d53d18c2...) or
/// the generic workspace-lifter CID (ca9638b4...).
const RUST_KIT_CANONICAL_CONTRACT_SET_CID: &str =
    "blake3-512:8f4bcc3c3e748ae303f8c8da80245f291e803eb2d241224c75c7ac470631e4dee7ff2e0ff59af571db3d506485c115acaddfd91e4e4315eb04ee37c035ddbc69";

/// Pinned contractSetCid produced by `--kit=cpp` after routing to the
/// `cpp-self-contracts` surface (mint_cpp_self_contracts binary, canonical
/// cpp slab). Mirrors the rust and go pinning pattern (PR #180, PR #183).
/// Any change to this value means either the surface wiring changed or the
/// canonical contracts changed -- both require explicit review and re-pinning.
///
/// This constant must NOT be the empty-set CID (d53d18c2...) and must match
/// the `contractSetCid` field in `.provekit/self-contracts-attestations/cpp.json`.
/// Fix: issue #203.
const CPP_KIT_CANONICAL_CONTRACT_SET_CID: &str =
    "blake3-512:0e17f718740e9e22b0897d1f7c2ee42a61b65b0d65379024465b38441e232c25b28eb8bf8a425a8770b68614a95510fd84e5ff23b5b028751ae9acb0ffe62d5e";

#[test]
#[serial(mint_kit_files)]
fn rust_kit_contract_set_cid_is_pinned_to_self_contracts_canonical() {
    let scratch = ScratchRepo::new();
    let root = scratch.root();

    let (ok, _, stderr) = run_mint(root, "rust");
    if !ok {
        eprintln!("rust kit: mint failed (mint-self-contracts may not be built)\n  stderr: {stderr}");
        // Skip rather than fail — binary may not be built in this environment.
        return;
    }

    let attest = read_attestation(root, "rust");
    let cset = attest["contractSetCid"].as_str().expect("contractSetCid must be string");

    // Pinned value: must match the canonical self-contracts CID.
    assert_eq!(
        cset,
        RUST_KIT_CANONICAL_CONTRACT_SET_CID,
        "rust kit contractSetCid diverged from the pinned canonical self-contracts CID.\n\
         This is the issue #176 Tier 1 regression gate.\n\
         If the self-contracts changed intentionally, update RUST_KIT_CANONICAL_CONTRACT_SET_CID.\n\
         Current: {cset}\n\
         Pinned:  {RUST_KIT_CANONICAL_CONTRACT_SET_CID}"
    );

    // Belt-and-suspenders: must NOT be the empty-set sentinel.
    assert_ne!(
        cset, EMPTY_SET_CID,
        "rust kit must not produce the empty-set CID — the self-contracts binary is missing or broken"
    );

    eprintln!("rust kit pinned contractSetCid confirmed: {cset}");
}

// ---------------------------------------------------------------------------
// Test 8: --kit=ts pins the expected contractSetCid (issue #204 wiring fix)
// ---------------------------------------------------------------------------

/// Pinned contractSetCid produced by `--kit=ts` after routing to the
/// `typescript-self-contracts` surface (mint-ts-self-contracts-rpc.cjs,
/// canonical 14-slab, 69-contract set). Verified by `make mint-ts`.
///
/// If this test fails with the old empty-set CID (`d53d18c2...`), the KIT_TABLE
/// routing regression has been reintroduced. If it fails with an unknown CID,
/// the TypeScript slab contracts have changed -- update TS_CONTRACT_SET_CID.
///
/// The surface is reached via:
///   `implementations/typescript/.provekit/lift/typescript-self-contracts/manifest.toml`
/// which spawns: `node --experimental-require-module src/bin/mint-ts-self-contracts-rpc.cjs`
const TS_CONTRACT_SET_CID: &str =
    "blake3-512:5a45314fdfb0fa1357a78a3f5c22e794fcbc10d3e649c39989710887eb742a6610861b9ab5c9e941ab01bc4357f5671a61c9d8a1d431dff8da3b6280a66d0d6a";

#[test]
#[serial(mint_kit_files)]
fn ts_kit_pins_expected_contract_set_cid() {
    let scratch = ScratchRepo::new();
    let root = scratch.root();

    let (ok, stdout, stderr) = run_mint(root, "ts");
    if !ok {
        eprintln!(
            "ts kit: mint failed (node/tsx may not be available)\n  stderr: {stderr}"
        );
        // Skip rather than fail -- node toolchain may not be present in all environments.
        return;
    }

    assert!(
        stdout.contains("contractSetCid:"),
        "ts kit: stdout must contain 'contractSetCid:'\n  stdout: {stdout}"
    );

    let attest = read_attestation(root, "ts");
    let cset = attest["contractSetCid"].as_str().unwrap();

    assert_ne!(
        cset, EMPTY_SET_CID,
        "ts kit: contractSetCid must NOT be the empty-set sentinel -- routing regression detected (issue #204)"
    );
    assert_eq!(
        cset, TS_CONTRACT_SET_CID,
        "ts kit: contractSetCid does not match pinned value from 14-slab, 69-contract set (issue #204)"
    );

    eprintln!("ts kit contractSetCid pinned correctly: {cset}");
}

// ---------------------------------------------------------------------------
// Test 8: cpp kit contractSetCid is pinned to the canonical self-contracts CID
//         (issue #203 regression gate, PR wiring cpp-self-contracts surface)
// ---------------------------------------------------------------------------

/// Verify that `--kit=cpp` routes to the cpp-self-contracts surface (not the
/// generic cpp lifter) and that the resulting contractSetCid matches the
/// known-good CID from the canonical cpp slab.
///
/// If this test fails with the empty-set CID (`d53d18c2...`), the KIT_TABLE
/// routing regression has been reintroduced. If it fails with an unknown CID,
/// the cpp slab contracts have changed -- update CPP_KIT_CANONICAL_CONTRACT_SET_CID.
#[test]
#[serial(mint_kit_files)]
fn cpp_kit_contract_set_cid_is_pinned_to_self_contracts_canonical() {
    let scratch = ScratchRepo::new();
    let root = scratch.root();

    let (ok, _, stderr) = run_mint(root, "cpp");
    if !ok {
        eprintln!("cpp kit: mint failed (mint_cpp_self_contracts may not be built)\n  stderr: {stderr}");
        // Skip rather than fail -- binary may not be built in this environment.
        return;
    }

    let attest = read_attestation(root, "cpp");
    let cset = attest["contractSetCid"].as_str().expect("contractSetCid must be string");

    // Pinned value: must match the canonical self-contracts CID.
    assert_eq!(
        cset,
        CPP_KIT_CANONICAL_CONTRACT_SET_CID,
        "cpp kit contractSetCid diverged from the pinned canonical self-contracts CID.\n\
         This is the issue #203 regression gate.\n\
         If the self-contracts changed intentionally, update CPP_KIT_CANONICAL_CONTRACT_SET_CID.\n\
         Current: {cset}\n\
         Pinned:  {CPP_KIT_CANONICAL_CONTRACT_SET_CID}"
    );

    // Belt-and-suspenders: must NOT be the empty-set sentinel.
    assert_ne!(
        cset, EMPTY_SET_CID,
        "cpp kit must not produce the empty-set CID -- the self-contracts binary is missing or broken"
    );

    eprintln!("cpp kit pinned contractSetCid confirmed: {cset}");
}

// ---------------------------------------------------------------------------
// Test 9: java kit contractSetCid is pinned to the canonical self-contracts CID
//         (issue #207 regression gate, PR wiring java-self-contracts surface)
// ---------------------------------------------------------------------------

/// Pinned contractSetCid produced by `--kit=java` after routing to the
/// `java-self-contracts` surface (`provekit-java-self-contracts.jar`,
/// canonical 6-slab, 30-contract set). Mirrors the rust / cpp / ts / go
/// pinning pattern.
///
/// If this test fails with the old empty-set CID (`d53d18c2...`), the
/// KIT_TABLE routing regression has been reintroduced. If it fails with
/// an unknown CID, the java slab contracts have changed -- update
/// JAVA_CONTRACT_SET_CID accordingly.
///
/// The surface is reached via:
///   `implementations/java/.provekit/lift/java-self-contracts/manifest.toml`
/// which spawns: `./provekit-java-self-contracts/run-rpc.sh --rpc`.
const JAVA_CONTRACT_SET_CID: &str =
    "blake3-512:a22c97362e15faf1e848eeb7d668ba50eba8cfb851a72465f2cccb0ca9e12af198ec14cc0e65453a18b1e40bbd17497f8975b6e3625bbf2b6b31e6ca6aacb6e3";

#[test]
#[serial(mint_kit_files)]
fn java_kit_pins_expected_contract_set_cid() {
    let scratch = ScratchRepo::new();
    let root = scratch.root();

    let (ok, stdout, stderr) = run_mint(root, "java");
    if !ok {
        eprintln!(
            "java kit: mint failed (java/jar may not be available)\n  stderr: {stderr}"
        );
        // Skip rather than fail -- jdk + built jar may not be present in
        // every test environment. CI builds the jar via `make build-java-self-contracts`.
        return;
    }

    assert!(
        stdout.contains("contractSetCid:"),
        "java kit: stdout must contain 'contractSetCid:'\n  stdout: {stdout}"
    );

    let attest = read_attestation(root, "java");
    let cset = attest["contractSetCid"].as_str().unwrap();

    assert_ne!(
        cset, EMPTY_SET_CID,
        "java kit: contractSetCid must NOT be the empty-set sentinel -- routing regression detected (issue #207)"
    );
    assert_eq!(
        cset, JAVA_CONTRACT_SET_CID,
        "java kit: contractSetCid does not match pinned value from 6-slab, 30-contract set (issue #207).\n\
         If the self-contracts changed intentionally, update JAVA_CONTRACT_SET_CID."
    );

    eprintln!("java kit pinned contractSetCid confirmed: {cset}");
}

// ---------------------------------------------------------------------------
// Test 10: --kit=swift pins the expected contractSetCid (issue #211 regression gate)
// ---------------------------------------------------------------------------

/// Pinned contractSetCid produced by `--kit=swift` after routing to the
/// `swift-self-contracts` surface (mint-swift-self-contracts --rpc, canonical
/// 11-contract slab in `implementations/swift/Sources/MintSwiftSelfContracts/Slab.swift`).
///
/// Computed via the canonical formula
/// (protocol/specs/2026-05-03-contract-set-extension.md §1):
///   contractSetCid = "blake3-512:" + hex(BLAKE3-512(JCS(<sorted contractCids>)))
/// where each contractCid follows
/// (protocol/specs/2026-05-03-contract-cid-vs-attestation-cid.md §1):
///   contractCid = "blake3-512:" + hex(BLAKE3-512(JCS({name, outBinding, pre?, post?, inv?})))
///
/// Because the swift kit uses the same JCS encoder and BLAKE3-512 hash as the
/// rust/go/cpp/ts kits, this CID is byte-equivalent to what those kits would
/// produce for the same 11-contract set.
///
/// If this test fails with the old empty-set CID (`d53d18c2...`), the KIT_TABLE
/// routing regression has been reintroduced. If it fails with an unknown CID,
/// the swift slab contracts have changed -- update SWIFT_CONTRACT_SET_CID.
///
/// macOS-only: the swift release binary requires the swift toolchain.
const SWIFT_CONTRACT_SET_CID: &str =
    "blake3-512:272543efe7c47b911659e1fc6a7368431b6eaa6010d2560a5d3e6717fcd470b50b24b607b481272941764b731d890d6973ab88e6000bde96fd306163a5742c56";

#[test]
#[serial(mint_kit_files)]
#[cfg_attr(not(target_os = "macos"), ignore)]
fn swift_kit_pins_expected_contract_set_cid() {
    let scratch = ScratchRepo::new();
    let root = scratch.root();

    let (ok, stdout, stderr) = run_mint(root, "swift");
    if !ok {
        eprintln!(
            "swift kit: mint failed (swift toolchain may not be available or release binary not built)\n  stderr: {stderr}"
        );
        // Skip rather than fail -- swift toolchain or release binary may not be present.
        return;
    }

    assert!(
        stdout.contains("contractSetCid:"),
        "swift kit: stdout must contain 'contractSetCid:'\n  stdout: {stdout}"
    );

    let attest = read_attestation(root, "swift");
    let cset = attest["contractSetCid"].as_str().unwrap();

    assert_ne!(
        cset, EMPTY_SET_CID,
        "swift kit: contractSetCid must NOT be the empty-set sentinel -- routing regression detected (issue #211)"
    );
    assert_eq!(
        cset, SWIFT_CONTRACT_SET_CID,
        "swift kit: contractSetCid does not match pinned value from 11-contract slab (issue #211)"
    );

    eprintln!("swift kit contractSetCid pinned correctly: {cset}");
}

// ---------------------------------------------------------------------------
// Test 11: --kit=python pins the expected contractSetCid (issue #205 wiring).
// ---------------------------------------------------------------------------

/// Pinned contractSetCid produced by `--kit=python` after routing to the
/// `python-self-contracts` surface (mint-python-self-contracts orchestrator,
/// canonical 5-slab, 15-contract set).
///
/// If this test fails with the old empty-set CID (`d53d18c2...`), the
/// KIT_TABLE routing regression has been reintroduced. If it fails with an
/// unknown CID, the python slab contracts have changed -- update
/// PYTHON_CONTRACT_SET_CID accordingly.
///
/// The surface is reached via:
///   `implementations/python/.provekit/lift/python-self-contracts/manifest.toml`
/// which spawns: `python3 bin/mint-python-self-contracts`.
const PYTHON_CONTRACT_SET_CID: &str =
    "blake3-512:b1de941756d0a3b352ca79ebed8b75644b7c782c3afe4163273220384125ec100457d5e969a921b7ceb277e329a24c5a4ea21ffd54963b51c1756befdb1793dc";

#[test]
#[serial(mint_kit_files)]
fn python_kit_pins_expected_contract_set_cid() {
    let scratch = ScratchRepo::new();
    let root = scratch.root();

    let (ok, stdout, stderr) = run_mint(root, "python");
    if !ok {
        eprintln!(
            "python kit: mint failed (python3 / blake3 / pynacl / cbor2 may not be available)\n  stderr: {stderr}"
        );
        // Skip rather than fail -- python toolchain or wheels may not be
        // present in all environments. CI installs deps via test-python's
        // `pip install -e .`, but if mint runs before test-python, the
        // wheels are still installed because cbor2/blake3/pynacl are part
        // of the package dependencies.
        return;
    }

    assert!(
        stdout.contains("contractSetCid:"),
        "python kit: stdout must contain 'contractSetCid:'\n  stdout: {stdout}"
    );

    let attest = read_attestation(root, "python");
    let cset = attest["contractSetCid"].as_str().expect("contractSetCid must be string");

    assert_ne!(
        cset, EMPTY_SET_CID,
        "python kit: contractSetCid must NOT be the empty-set sentinel -- routing regression detected (issue #205)"
    );
    assert_eq!(
        cset, PYTHON_CONTRACT_SET_CID,
        "python kit contractSetCid diverged from the pinned canonical self-contracts CID.\n\
         This is the issue #205 regression gate.\n\
         If the self-contracts changed intentionally, update PYTHON_CONTRACT_SET_CID.\n\
         Current: {cset}\n\
         Pinned:  {PYTHON_CONTRACT_SET_CID}"
    );

    eprintln!("python kit pinned contractSetCid confirmed: {cset}");
}
