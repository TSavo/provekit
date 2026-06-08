# Audit: 3 pre-existing failures in mint_kit_integration.rs

**Date:** 2026-05-12
**Branch:** diag/mint-kit-pre-existing (off origin/main)
**Test file:** `implementations/rust/sugar-cli/tests/mint_kit_integration.rs`
**Conformance gate:** these tests are NOT gated in main CI conformance, so main stays green while they silently fail locally.

---

## Summary

All three failures share one root cause: the `mint-self-contracts` (rust) and `mint_cpp_self_contracts` (cpp) self-contracts binaries are not present in the worktree at the paths their manifests declare. When the dispatcher cannot spawn the binary (ENOENT), it falls back to a well-formed empty-set attestation and exits 0. The `!ok` skip guards inside the tests don't fire -- the spawn succeeds in the sense that `run_mint` returns `ok=true`. The tests then read the attestation, find the canonical empty-set CID (`d53d18c2...`), and fail.

## Resolution

Issue #70 wires these three checks into the Linux conformance gate with
`make test-mint-kit-integration-pins`. The target runs after `all-mint`, so
CI has already built and minted the rust and cpp self-contract surfaces before
the release-mode `mint_kit_integration` pin tests execute with `CI=1`.

That makes the local-dev behavior and CI behavior intentionally different:
local test runs may still skip the pin assertion when the self-contract binary
is absent, but CI treats the same empty-set CID as a hard failure.

---

## Failing tests

### 1. `kits_with_real_contracts_produce_nonempty_contract_set`

**File/line:** `mint_kit_integration.rs:546`

**Assertion:**
```rust
assert_ne!(
    cset, EMPTY_SET_CID,
    "kit `rust`: expected non-empty contractSetCid when lifter finds real contracts"
);
```

**Actual emission:**
```
left (actual): blake3-512:d53d18c23212ea7b6300594bb89bce60218f6eff2b9d628b8cc42d3e79bbd5ab09994845815cc7185113418f9fc2edc7606b06f0d57a6d581e7cff5b290f3229
right (EMPTY_SET_CID): blake3-512:d53d18c23212ea7b6300594bb89bce60218f6eff2b9d628b8cc42d3e79bbd5ab09994845815cc7185113418f9fc2edc7606b06f0d57a6d581e7cff5b290f3229
```

**Root cause:** `KITS_WITH_REAL_CONTRACTS` includes `"rust"`. The `rust` self-contracts lifter binary (`./target/release/mint-self-contracts` relative to `implementations/rust/`) is not built in this environment. The dispatcher hits ENOENT, returns ok=true with an empty-set attestation. The loop body has no empty-set skip for `rust` (only for `zig`), so the `assert_ne` fires. This is not a logic bug in the test -- it is an environment gap: the binary must be built via `make build-rust-self-contracts` (or equivalent) for the test to pass.

**Fix options:**

A. Build the binary: `cd implementations/rust && cargo build --release -p sugar-self-contracts` (or equivalent make target). The test then passes with the real non-empty CID. This is the correct fix for CI.

B. Add an empty-set skip guard for `rust` (and `cpp`) matching the zig pattern:
```rust
if (*kit == "rust" || *kit == "cpp") && cset == EMPTY_SET_CID {
    panic_if_empty_set_cid_in_ci(kit);
    eprintln!("kit `{kit}`: lifter binary not built locally; skipping");
    continue;
}
```
This is a defensible local-dev accommodation but weakens the test's guarantees unless `panic_if_empty_set_cid_in_ci` correctly identifies CI and panics there.

**Recommendation:** Fix A (build the binary). Fix B is acceptable as a local-dev guard only if `panic_if_empty_set_cid_in_ci` is wired correctly and CI always builds the binary.

---

### 2. `rust_kit_contract_set_cid_is_pinned_to_self_contracts_canonical`

**File/line:** `mint_kit_integration.rs:831`

**Assertion:**
```rust
assert_eq!(
    cset, RUST_KIT_FULL_SELF_CONTRACT_SURFACE_CID,
    "rust kit contractSetCid diverged from the pinned canonical self-contracts CID."
);
```

**Pinned constant (both linux and non-linux, same value):**
```
blake3-512:eb9979cc46b716217ece7340696ba2d0a97fac61a39f9673a1dfa8e38441737ca6e4dd307e2e1fb404093b98b6b412d1bd51a515e7405282bdd5ad32dff02dc0
```

**Actual emission:**
```
blake3-512:d53d18c23212ea7b6300594bb89bce60218f6eff2b9d628b8cc42d3e79bbd5ab09994845815cc7185113418f9fc2edc7606b06f0d57a6d581e7cff5b290f3229
```
(the canonical empty-set CID)

**Root cause:** Same as failure 1. The self-contracts binary `./target/release/mint-self-contracts` is absent. The dispatcher ENOENT fallback fires, producing the empty-set CID. The test has a `!ok` early-return skip, but `run_mint` returns `ok=true` when ENOENT fires -- because the dispatcher catches NotFound and returns a synthesized empty-set response, not a non-zero exit code. The `!ok` guard is therefore dead for this failure mode.

**Secondary note:** The comment in the test (`// Skip rather than fail: binary may not be built in this environment`) implies intent to skip on missing binary, but the skip mechanism (`!ok` check) does not cover the ENOENT-fallback path. This is a test design gap: the skip guard tests for mint exit code, not for whether the lifter was actually invoked.

**Fix options:**

A. Build the binary (same as failure 1, fixes both simultaneously).

B. Add an explicit empty-set guard before the `assert_eq`:
```rust
if cset == EMPTY_SET_CID {
    panic_if_empty_set_cid_in_ci("rust");
    eprintln!("rust kit: lifter binary not built -- skipping pinning assertion");
    return;
}
```
This mirrors the pattern already used in `c_kit_pins_expected_contract_set_cid` and `ruby_kit_pins_expected_contract_set_cid`, making the skip behavior consistent across all pinned-CID tests.

**Recommendation:** Fix B (add empty-set guard) as an immediate correctness patch; Fix A (build the binary) in CI. The c/ruby tests already use Fix B as their local-dev skip pattern -- rust should match.

---

### 3. `cpp_kit_contract_set_cid_is_pinned_to_self_contracts_canonical`

**File/line:** `mint_kit_integration.rs:958`

**Assertion:**
```rust
assert_eq!(
    cset, CPP_KIT_CANONICAL_CONTRACT_SET_CID,
    "cpp kit contractSetCid diverged from the pinned canonical self-contracts CID."
);
```

**Pinned constant:**
```
blake3-512:0e17f718740e9e22b0897d1f7c2ee42a61b65b0d65379024465b38441e232c25b28eb8bf8a425a8770b68614a95510fd84e5ff23b5b028751ae9acb0ffe62d5e
```

**Actual emission:**
```
blake3-512:d53d18c23212ea7b6300594bb89bce60218f6eff2b9d628b8cc42d3e79bbd5ab09994845815cc7185113418f9fc2edc7606b06f0d57a6d581e7cff5b290f3229
```
(the canonical empty-set CID)

**Manifest path:** `implementations/cpp/.sugar/lift/cpp-self-contracts/manifest.toml`
**Binary declared in manifest:** `./target/mint_cpp_self_contracts`
**Binary present:** No (`implementations/cpp/target/` does not exist)

**Root cause:** Identical mechanism to failure 2. The cpp self-contracts binary `./target/mint_cpp_self_contracts` is absent. The ENOENT fallback fires, returning ok=true with the empty-set CID. The `!ok` skip guard in the test doesn't fire. The test proceeds to the `assert_eq` and fails.

The comment `// Skip rather than fail -- binary may not be built in this environment` again expresses intent to skip, but the skip guard is insufficient.

The `.sugar/self-contracts-attestations/cpp.json` file also does not exist in the worktree, confirming this surface has never been successfully minted in this environment.

**Fix options:**

A. Build the cpp self-contracts binary (`cd implementations/cpp && make mint_cpp_self_contracts` or equivalent).

B. Add an empty-set guard before the `assert_eq`, mirroring `c_kit_pins_expected_contract_set_cid`:
```rust
if cset == EMPTY_SET_CID {
    eprintln!("cpp kit: lifter binary not built -- skipping pinning assertion");
    return;
}
```

**Recommendation:** Fix B as an immediate patch; Fix A for CI. The test already has the correct comment intent -- the skip guard just needs to cover the ENOENT fallback path, not just mint process exit code.

---

## Why main CI stayed green

Before issue #70, the conformance test runner excluded these targeted
`mint_kit_integration` checks from the gating suite. The 16 other tests in this
file passed cleanly, including other pinned-CID tests (go, java, ts, python,
ruby, c, swift, zig) that either had the required binaries present or had
correct empty-set skip guards.

---

## Cross-failure pattern

The `!ok` early-return pattern was designed to skip when the lifter binary is missing, but it has a blind spot: `cmd_mint::dispatch` converts ENOENT into a graceful empty-set attestation and exits 0. So `run_mint` returns `(true, ..., ...)` even when the binary was never invoked. The correct skip pattern -- already used by c, ruby, and zig tests -- is to check `cset == EMPTY_SET_CID` after reading the attestation, then call `panic_if_empty_set_cid_in_ci` and return early. The rust and cpp pinned tests were written before this pattern was established.

---

## Recommended follow-up (fix PR)

1. Add `cset == EMPTY_SET_CID` skip guards to `rust_kit_contract_set_cid_is_pinned_to_self_contracts_canonical` and `cpp_kit_contract_set_cid_is_pinned_to_self_contracts_canonical`, matching the c/ruby pattern.
2. Add a `rust`/`cpp` empty-set skip guard in `kits_with_real_contracts_produce_nonempty_contract_set`, matching the zig pattern.
3. Wire `make build-rust-self-contracts` and `make build-cpp-self-contracts` into the CI conformance job so the binaries are present and the tests exercise the real surfaces.

Steps 1-2 fix the silent failures locally; step 3 restores the actual regression-gate semantics in CI.
