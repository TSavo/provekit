# Exhibit Transport Policy

Architect ruling 2026-05-16. Closes #1060 (A5 prereq for the Trinity exhibit #1024).

## The decision

When a test exercises a registered Kit whose transform mechanism is subprocess-based (Python via `python3`, Java via `javac`/`java`, C via `cc`, etc.), the test must invoke that Kit's real subprocess transport. Three options were considered:

1. **Real subprocess in CI** (legitimate). Cargo test runs slow when the exhibit runs; that is acceptable. The substrate's first principle (Supra omnia, rectum) pays for the slowness.
2. **Fixture-transport stub** (forbidden). A stub that pretends to be the Kit but bypasses its real transport is the substrate's lying-shape applied to test mechanics.
3. **`#[ignore]` marker on the structural-property test** (forbidden). Ignored tests look like proofs but do not execute. Same lying-shape as #1043's deletion-rule miss.

Option 1 is the only on-policy choice. Options 2 and 3 are explicitly out-of-policy.

## What this means in practice

### Allowed mechanics

- CI can opt to run the exhibit in a separate slow-test lane that does not block other tests (e.g., a `cargo test --release --test trinity_exhibit --features slow-tests` invocation). The slow lane runs in CI; it does not run on every local `cargo test`. The lane is not optional; it runs on every PR.
- Tests can use `#[cfg(not(target_env = "musl"))]` or platform-specific guards if a particular toolchain (e.g., `javac` on a minimal CI image) is genuinely unavailable. The guard names the platform constraint explicitly; it does not skip the test for convenience.
- Tests can use a real "test fixtures" directory containing real source files, real lift inputs, real expected outputs. The fixtures are data, not transport stubs. The Kit's transport processes the fixtures with its real implementation.

### Forbidden mechanics

- Any test-only Kit trait impl that bypasses subprocess invocation.
- Any `cfg(test)` branch inside production Kit code that returns canned output without invoking the transport.
- Any `MockPythonKit` / `FakeJavaKit` / `StubCKit` registered in test setup.
- Any `#[ignore]` marker on a test that asserts a structural property the substrate is supposed to verify.
- Any `if cfg!(ci) { return canned_result } else { spawn_subprocess() }` pattern.

## The build-on-existing-kits boundary

The "no parallel code path" rule applies to substrate-internal code, not test-harness mechanics. Spawning subprocesses to invoke registered kits through their declared transport IS the legitimate path. Stubbing the Kit's transport in tests is the parallel path that is forbidden. The distinction is whether the test exercises the registered Kit's real `transform()` or a substitute.

## Why this matters

The Trinity exhibit (#1024) and the per-kit conformance fixtures (#1039) are the empirical assertion of the substrate's claims. If their transport is stubbed, the assertion is a hash trick at best and a lying-shape at worst. Real subprocesses are the only way the assertion stays empirically real. The CI cost (slow test lane) is the cost of being right; the substrate's first principle pays for it.

This ruling also serves as the durable answer to the "should we just `#[ignore]` this test?" question that arises naturally on capstone PRs. The answer is no, and the reason is on record. Future contributors and codex sessions reading this policy inherit the discipline without needing the architect to re-derive it.

## Scope of application

This policy applies to:

- The Trinity exhibit (#1024) when it lands.
- Every per-kit conformance fixture PR in #1039 (Python #1052, Java #1053, C #1054).
- Every future test that asserts a structural property the substrate is supposed to verify.

It does NOT apply to:

- Unit tests of substrate-internal code that does not invoke kits (e.g., a `Term::walk` iterator test, a `Catalog::contains` predicate test, a `PathExecutionChain` accessor test, a `walk_premises_to_root` chain-walk test). These exercise data structures and pure functions, not Kit transport.
- Documentation, formatter, or style checks that have no transport.

## Audit obligation

If a future PR introduces a fixture-transport stub or an `#[ignore]` on a structural-property test, the reviewer is obligated to refuse the merge and cite this policy. The substrate's correctness claim cannot survive selectively-executed structural tests.

## Cross-references

- #1024 Trinity exhibit (consumes this policy in its acceptance criteria).
- #1039 per-kit conformance fixtures (Python #1052, Java #1053, C #1054) (consume this policy).
- #1043 deletion-rule miss (the original lying-shape instance this policy generalizes).
- `docs/plans/2026-05-16-trinity-completion-checklist.md` (the in-repo Trinity state).
- Substrate first principle (Supra omnia, rectum) (the principle this policy implements at the test-harness boundary).

## Authority

Architect ruling, locked. Tracked under #1060. Apply on every relevant PR going forward.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
