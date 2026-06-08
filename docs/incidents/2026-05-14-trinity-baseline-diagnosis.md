# Trinity baseline diagnosis

## 1. Reproduction

cwd:

```text
/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust
```

Run this first:

```bash
CARGO_TARGET_DIR=$(mktemp -d /private/tmp/pk-trinity-target.XXXXXX) TMPDIR=/private/tmp RUST_BACKTRACE=1 cargo test -p sugar-cli --test trinity_roundtrip_test -- --nocapture
```

Expected: `trinity_round_trip` passes, either through Branch 1 byte-identical equality if composed loss is empty, or through the v0 loudly-bounded-lossy Branch 2 if the run emits honest gap records.

Actual: the test fails before Branch 2. Leg 1 exits zero but produces no `translated/java/` directory, so the test panics at `implementations/rust/sugar-cli/tests/trinity_roundtrip_test.rs:189`.

Diagnostic control:

```bash
cargo build -p sugar-walk --bin sugar-walk-rpc && cargo test -p sugar-cli --test trinity_roundtrip_test -- --nocapture
```

Expected: if the only blocker is the missing Rust bind lift kit binary, this should let the sibling-of-current-executable discovery path find `target/debug/sugar-walk-rpc`.

Actual: this passes. It prints a v0 loudly-bounded-lossy outcome with `bind-stub-body-emitted` gaps plus Java lift plugin unavailability on leg 2. That control isolates the red baseline to missing lift-kit discovery or missing lift-kit build state in a clean target directory, not to Java or C body template semantics.

Direct leg-1 reproducer:

```bash
fresh_target=$(mktemp -d /private/tmp/pk-trinity-target.XXXXXX)
CARGO_TARGET_DIR="$fresh_target" cargo test -p sugar-cli --test trinity_roundtrip_test --no-run
fixture=$(mktemp -d /private/tmp/pk-trinity-fixture.XXXXXX)
out=$(mktemp -d /private/tmp/pk-trinity-out.XXXXXX)
cp -R sugar-cli/tests/fixtures/trinity_roundtrip/. "$fixture"/
"$fresh_target/debug/sugar" bind --root "$fixture" --lang rust --output "$out" --rewrite canonical --mode monitor --quiet --target-language java
find "$out" -maxdepth 4 -print | sort
sed -n '1,220p' "$out/gaps.json"
```

Expected: `$out/translated/java/` exists and contains at least one `.java` file.

Actual from a red run:

```text
fixture=/private/tmp/pk-trinity-fixture.pwZxs3
out=/private/tmp/pk-trinity-out.nX0JTk
exit=0
tree:
/private/tmp/pk-trinity-out.nX0JTk
/private/tmp/pk-trinity-out.nX0JTk/gaps.json
gaps.json:
{
  "source_lang": "rust",
  "gaps": [
    {
      "kind": "kit-plugin-unavailable",
      "detail": "no `kind = \"lift\"` plugin available for source language `rust`: kit-plugin-unavailable: no lift plugin for language `rust` (no manifest at .sugar/lift/rust-bind/ or .sugar/lift/rust/, no env SUGAR_BIND_LIFT_RUST_BIN, no built-in binary under implementations/rust/, no `sugar-bind-lift-rust` on PATH). The bind pipeline cannot Verb 1 (Lift) without a kit; this leg is loudly-bounded-lossy at the lift boundary. Author or build a plugin per 2026-05-13-bind-ir-lift-result.md to close this gap."
    }
  ]
}
```

## 2. Failure trace

Failing command:

```bash
CARGO_TARGET_DIR=$(mktemp -d /private/tmp/pk-trinity-target.XXXXXX) TMPDIR=/private/tmp RUST_BACKTRACE=1 cargo test -p sugar-cli --test trinity_roundtrip_test -- --nocapture
```

Complete output from the fresh-target run:

```text
   Compiling unicode-ident v1.0.24
   Compiling quote v1.0.45
   Compiling proc-macro2 v1.0.106
   Compiling serde_core v1.0.228
   Compiling cfg-if v1.0.4
   Compiling hashbrown v0.17.0
   Compiling equivalent v1.0.2
   Compiling serde v1.0.228
   Compiling zmij v1.0.21
   Compiling find-msvc-tools v0.1.9
   Compiling shlex v1.3.0
   Compiling itoa v1.0.18
   Compiling version_check v0.9.5
   Compiling thiserror v1.0.69
   Compiling serde_json v1.0.149
   Compiling memchr v2.8.0
   Compiling constant_time_eq v0.4.2
   Compiling typenum v1.20.0
   Compiling arrayvec v0.7.6
   Compiling cc v1.2.61
   Compiling cpufeatures v0.3.0
   Compiling arrayref v0.3.9
   Compiling hex v0.4.3
   Compiling semver v1.0.28
   Compiling cpufeatures v0.2.17
   Compiling subtle v2.6.1
   Compiling signature v2.2.0
   Compiling generic-array v0.14.7
   Compiling zeroize v1.8.2
   Compiling ed25519 v2.2.3
   Compiling rustc_version v0.4.1
   Compiling rand_core v0.6.4
   Compiling base64 v0.22.1
   Compiling crossbeam-utils v0.8.21
   Compiling autocfg v1.5.0
   Compiling indexmap v2.14.0
   Compiling curve25519-dalek v4.1.3
   Compiling libc v0.2.186
   Compiling rayon-core v1.13.0
   Compiling toml_write v0.1.2
   Compiling same-file v1.0.6
   Compiling num-traits v0.2.19
   Compiling core-foundation-sys v0.8.7
   Compiling winnow v0.7.15
   Compiling iana-time-zone v0.1.65
   Compiling walkdir v2.5.0
   Compiling utf8parse v0.2.2
   Compiling either v1.15.0
   Compiling anstyle-parse v1.0.0
   Compiling colorchoice v1.0.5
   Compiling anstyle-query v1.1.5
   Compiling anstyle v1.0.14
   Compiling is_terminal_polyfill v1.70.2
   Compiling clap_lex v1.1.0
   Compiling strsim v0.11.1
   Compiling blake3 v1.8.5
   Compiling anyhow v1.0.102
   Compiling anstream v1.0.0
   Compiling parking_lot_core v0.9.12
   Compiling heck v0.5.0
   Compiling owo-colors v4.3.0
   Compiling clap_builder v4.6.0
   Compiling rustix v1.1.4
   Compiling pin-project-lite v0.2.17
   Compiling smallvec v1.15.1
   Compiling getrandom v0.4.2
   Compiling syn v2.0.117
   Compiling unsafe-libyaml v0.2.11
   Compiling slab v0.4.12
   Compiling ryu v1.0.23
   Compiling block-buffer v0.10.4
   Compiling crypto-common v0.1.7
   Compiling crossbeam-epoch v0.9.18
   Compiling digest v0.10.7
   Compiling futures-core v0.3.32
   Compiling futures-task v0.3.32
   Compiling sha2 v0.10.9
   Compiling scopeguard v1.2.0
   Compiling crossbeam-deque v0.8.6
   Compiling futures-util v0.3.32
   Compiling lock_api v0.4.14
   Compiling once_cell v1.21.4
   Compiling bitflags v2.11.1
   Compiling rayon v1.12.0
   Compiling sdd v3.0.10
   Compiling errno v0.3.14
   Compiling scc v2.4.0
   Compiling log v0.4.29
   Compiling fastrand v2.4.1
   Compiling chrono v0.4.44
   Compiling parking_lot v0.12.5
   Compiling futures-executor v0.3.32
   Compiling serde_derive v1.0.228
   Compiling thiserror-impl v1.0.69
   Compiling curve25519-dalek-derive v0.1.1
   Compiling clap_derive v4.6.1
   Compiling serial_test_derive v3.4.0
   Compiling serial_test v3.4.0
   Compiling clap v4.6.1
   Compiling sugar-canonicalizer v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-canonicalizer)
   Compiling tempfile v3.27.0
   Compiling ed25519-dalek v2.2.0
   Compiling sugar-proof-envelope v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-proof-envelope)
   Compiling sugar-claim-envelope v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-claim-envelope)
   Compiling sugar-ir-types v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-ir-types)
   Compiling sugar-ir-compiler v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-ir-compiler)
   Compiling toml_datetime v0.6.11
   Compiling serde_spanned v0.6.9
   Compiling serde_yaml v0.9.34+deprecated
   Compiling sugar-plugin-loader v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-plugin-loader)
   Compiling toml_edit v0.22.27
   Compiling toml v0.8.23
   Compiling sugar-ir-symbolic v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-ir-symbolic)
   Compiling sugar-ir-compiler-lean v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-ir-compiler-lean)
   Compiling sugar-ir-compiler-smt-lib v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-ir-compiler-smt-lib)
   Compiling sugar-ir-compiler-maude v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-ir-compiler-maude)
   Compiling sugar-ir-compiler-coq v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-ir-compiler-coq)
   Compiling libsugar v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/libsugar)
   Compiling sugar-lift-contracts v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-lift-contracts)
   Compiling sugar-lift-prusti v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-lift-prusti)
   Compiling sugar-lift-proptest v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-lift-proptest)
   Compiling sugar-lift-quickcheck v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-lift-quickcheck)
   Compiling sugar-lift-creusot v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-lift-creusot)
   Compiling sugar-lift-verus v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-lift-verus)
   Compiling sugar-lift-flux v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-lift-flux)
   Compiling sugar-lift-kani v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-lift-kani)
   Compiling sugar-agent v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-agent)
   Compiling sugar-verifier v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-verifier)
   Compiling sugar-walk v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-walk)
   Compiling sugar-mint-amp v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-mint-amp)
   Compiling sugar-lift-rust-tests v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-lift-rust-tests)
   Compiling sugar-self-contracts v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-self-contracts)
   Compiling sugar-linker v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-linker)
   Compiling sugar-lift v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-lift)
   Compiling sugar-cli v0.1.0 (/Users/tsavo/sugar-worktrees/pk-trinity-baseline-diagnosis/implementations/rust/sugar-cli)
warning: function `type_to_str` is never used
   --> sugar-cli/src/cmd_transport.rs:918:4
    |
918 | fn type_to_str(ty: &syn::Type) -> String {
    |    ^^^^^^^^^^^
    |
    = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: fields `pre` and `post` are never read
   --> sugar-cli/src/cmd_transport.rs:948:5
    |
946 | struct ContractAnnotations {
    |        ------------------- fields in this struct
947 |     /// `pre`-condition formula from `#[requires(...)]` (or language equivalent).
948 |     pre: Option<Rc<Formula>>,
    |     ^^^
949 |     /// `post`-condition formula from `#[ensures(...)]` (or language equivalent).
950 |     post: Option<Rc<Formula>>,
    |     ^^^^
    |
    = note: `ContractAnnotations` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: function `emit_term_syntax` is never used
   --> sugar-cli/src/cmd_transport.rs:985:4
    |
985 | fn emit_term_syntax(term: &SymTerm) -> String {
    |    ^^^^^^^^^^^^^^^^

warning: function `peel_quantifiers` is never used
    --> sugar-cli/src/cmd_transport.rs:1011:4
     |
1011 | fn peel_quantifiers(formula: &Formula) -> &Formula {
     |    ^^^^^^^^^^^^^^^^

warning: `sugar-cli` (lib) generated 4 warnings
warning: use of deprecated method `tempfile::TempDir::into_path`: use TempDir::keep()
   --> sugar-cli/tests/trinity_roundtrip_test.rs:176:61
    |
176 |     let fixture_tmp = tempfile::tempdir().expect("tempdir").into_path();
    |                                                             ^^^^^^^^^
    |
    = note: `#[warn(deprecated)]` on by default

warning: use of deprecated method `tempfile::TempDir::into_path`: use TempDir::keep()
   --> sugar-cli/tests/trinity_roundtrip_test.rs:180:54
    |
180 |     let out1 = tempfile::tempdir().expect("tempdir").into_path();
    |                                                      ^^^^^^^^^

warning: use of deprecated method `tempfile::TempDir::into_path`: use TempDir::keep()
   --> sugar-cli/tests/trinity_roundtrip_test.rs:225:54
    |
225 |     let out2 = tempfile::tempdir().expect("tempdir").into_path();
    |                                                      ^^^^^^^^^

warning: use of deprecated method `tempfile::TempDir::into_path`: use TempDir::keep()
   --> sugar-cli/tests/trinity_roundtrip_test.rs:252:62
    |
252 |         let out3_dir = tempfile::tempdir().expect("tempdir").into_path();
    |                                                              ^^^^^^^^^

warning: `sugar-cli` (test "trinity_roundtrip_test") generated 4 warnings
warning: fields `realization_plan_mementos` and `observation_wrapper_mementos` are never read
   --> sugar-cli/src/cmd_bind.rs:469:9
    |
459 | pub struct EngineResult {
    |            ------------ fields in this struct
...
469 |     pub realization_plan_mementos: Vec<RealizationPlanMemento>,
    |         ^^^^^^^^^^^^^^^^^^^^^^^^^
...
472 |     pub observation_wrapper_mementos: Vec<ObservationWrapperMemento>,
    |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    |
    = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: function `line_for_fn` is never used
    --> sugar-cli/src/cmd_bind.rs:2031:4
     |
2031 | fn line_for_fn(src: &str, fn_name: &str) -> usize {
     |    ^^^^^^^^^^^

warning: function `normalize_ws` is never used
    --> sugar-cli/src/cmd_bind.rs:2589:4
     |
2589 | fn normalize_ws(s: &str) -> String {
     |    ^^^^^^^^^^^^

warning: function `type_to_str` is never used
   --> sugar-cli/src/cmd_transport.rs:918:4
    |
918 | fn type_to_str(ty: &syn::Type) -> String {
    |    ^^^^^^^^^^^

warning: field `used_sugars` is never read
    --> sugar-cli/src/cmd_transport.rs:1034:9
     |
1018 | pub struct RealizedSource {
     |            -------------- field in this struct
...
1034 |     pub used_sugars: Vec<serde_json::Value>,
     |         ^^^^^^^^^^^
     |
     = note: `RealizedSource` has a derived impl for the trait `Debug`, but this is intentionally ignored during dead code analysis

warning: function `realize_for_bind` is never used
    --> sugar-cli/src/cmd_transport.rs:1250:8
     |
1250 | pub fn realize_for_bind(
     |        ^^^^^^^^^^^^^^^^

warning: field `diagnostics` is never read
   --> sugar-cli/src/kit_dispatch.rs:105:9
    |
103 | pub struct BindLiftResult {
    |            -------------- field in this struct
104 |     pub entries: Vec<BindLiftEntry>,
105 |     pub diagnostics: Vec<Value>,
    |         ^^^^^^^^^^^
    |
    = note: `BindLiftResult` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: `sugar-cli` (bin "sugar") generated 10 warnings (3 duplicates)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 49.96s
     Running tests/trinity_roundtrip_test.rs (/private/tmp/pk-trinity-target.IlGZGZ/debug/deps/trinity_roundtrip_test-90db657f80f0cbcc)

running 1 test

thread 'trinity_round_trip' (13313181) panicked at sugar-cli/tests/trinity_roundtrip_test.rs:189:5:
Leg 1 must produce translated/java/ dir
stack backtrace:
   0: __rustc::rust_begin_unwind
             at /rustc/59807616e1fa2540724bfbac14d7976d7e4a3860/library/std/src/panicking.rs:689:5
   1: core::panicking::panic_fmt
             at /rustc/59807616e1fa2540724bfbac14d7976d7e4a3860/library/core/src/panicking.rs:80:14
   2: trinity_roundtrip_test::trinity_round_trip
             at ./tests/trinity_roundtrip_test.rs:189:5
   3: trinity_roundtrip_test::trinity_round_trip::{{closure}}
             at ./tests/trinity_roundtrip_test.rs:174:24
   4: core::ops::function::FnOnce::call_once
             at /rustc/59807616e1fa2540724bfbac14d7976d7e4a3860/library/core/src/ops/function.rs:250:5
   5: <fn() -> core::result::Result<(), alloc::string::String> as core::ops::function::FnOnce<()>>::call_once
             at /rustc/59807616e1fa2540724bfbac14d7976d7e4a3860/library/core/src/ops/function.rs:250:5
note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace.
test trinity_round_trip ... FAILED

failures:

failures:
    trinity_round_trip

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.14s

error: test failed, to rerun pass `-p sugar-cli --test trinity_roundtrip_test`
```

Assertion location:

```text
implementations/rust/sugar-cli/tests/trinity_roundtrip_test.rs:188-189
let java_dir = out1.join("translated").join("java");
assert!(java_dir.exists(), "Leg 1 must produce translated/java/ dir");
```

## 3. Expected artifact vs actual state

At failure, the test has just run leg 1:

```text
sugar bind --root <fixture_tmp> --lang rust --output <out1> --rewrite canonical --mode monitor --quiet --target-language java
```

Expected at that exact point:

```text
<out1>/translated/java/
<out1>/translated/java/*.java
```

The next assertion would require at least one `.java` file under that directory, and the following sanity block would read `<out1>/index.json`.

Actual temp fixture root from the fresh-target run:

```text
/private/tmp/.tmpaXhkmC
/private/tmp/.tmpaXhkmC/src
/private/tmp/.tmpaXhkmC/src/lib.rs
```

Actual leg-1 output directory from the same run:

```text
/private/tmp/.tmpXlIMCm
/private/tmp/.tmpXlIMCm/gaps.json
```

Actual `gaps.json`:

```json
{
  "source_lang": "rust",
  "gaps": [
    {
      "kind": "kit-plugin-unavailable",
      "detail": "no `kind = \"lift\"` plugin available for source language `rust`: kit-plugin-unavailable: no lift plugin for language `rust` (no manifest at .sugar/lift/rust-bind/ or .sugar/lift/rust/, no env SUGAR_BIND_LIFT_RUST_BIN, no built-in binary under implementations/rust/, no `sugar-bind-lift-rust` on PATH). The bind pipeline cannot Verb 1 (Lift) without a kit; this leg is loudly-bounded-lossy at the lift boundary. Author or build a plugin per 2026-05-13-bind-ir-lift-result.md to close this gap."
    }
  ]
}
```

Control observation: in the fresh target dir used for the failing run, `debug/sugar` existed but `debug/sugar-walk-rpc` did not. After building `sugar-walk-rpc`, the same test passed.

## 4. Provenance trace: when did this go red?

Current HEAD:

```text
a0531102e1a80ff0907f66da7ea87083188bd5f3
```

The semantic assertion requiring leg 1 to produce `translated/java/` was introduced by:

```text
42c3b43f26754869e45753edb28a6dcde65df7a6
2026-05-12T10:13:49-07:00
T Savo <kevlar.sindome@gmail.com>
feat(cli): trinity round-trip integration test (k(k'(k''(I)))=t) (#724)
```

Evidence: `git show 42c3b43f -- implementations/rust/sugar-cli/tests/trinity_roundtrip_test.rs` shows the assertion added as:

```text
assert!(
    java_dir.exists(),
    "Leg 1 must produce translated/java/ dir"
);
```

Current `git blame -L 170,195` reports line 189 as `525a6a9c`, but that is a formatting-only one-line rewrite of the already-existing assertion:

```text
525a6a9cc T Savo 2026-05-13 14:43:15 -0700 line 189
assert!(java_dir.exists(), "Leg 1 must produce translated/java/ dir");
```

Last green commit found with a fresh target dir:

```text
d9ea20c9f54f2a208133c6a0654123777cb87345
2026-05-13T06:34:48-07:00
T Savo <kevlar.sindome@gmail.com>
feat(769): body templates for 9 remaining trinity concepts - verdict 13 to 4 (#769)
```

Command used:

```bash
git archive d9ea20c9f54f2a208133c6a0654123777cb87345 | tar -x -C /private/tmp/pk-trinity-d9ea-check/src
cd /private/tmp/pk-trinity-d9ea-check/src/implementations/rust
CARGO_TARGET_DIR=/private/tmp/pk-trinity-d9ea-check/target cargo test -q -p sugar-cli --test trinity_roundtrip_test -- --nocapture
```

Result:

```text
PASS d9ea20c9f54f2a208133c6a0654123777cb87345
walk_rpc_built no
```

First red commit found with a fresh target dir:

```text
91c408d6d9cb6809b4230b965d6c95048aba473e
2026-05-13T08:53:47-07:00
T Savo <kevlar.sindome@gmail.com>
feat(770): cmd_bind + cmd_transport kit-agnostic dispatcher - delete Rust special-case (#779)
```

Command used:

```bash
git archive 91c408d6d9cb6809b4230b965d6c95048aba473e | tar -x -C /private/tmp/pk-trinity-keycheck/src
cd /private/tmp/pk-trinity-keycheck/src/implementations/rust
CARGO_TARGET_DIR=/private/tmp/pk-trinity-keycheck/target cargo test -q -p sugar-cli --test trinity_roundtrip_test -- --nocapture
```

Result:

```text
FAIL 91c408d6d9cb
thread 'trinity_round_trip' panicked at sugar-cli/tests/trinity_roundtrip_test.rs:193:5:
Leg 1 must produce translated/java/ dir
walk_rpc_built no
```

Relevant changes between last green and HEAD:

```text
91c408d6 2026-05-13T08:53:47-07:00 feat(770): cmd_bind + cmd_transport kit-agnostic dispatcher - delete Rust special-case (#779)
62562635 2026-05-13T08:57:22-07:00 fix(783): remove stale v0-capability-gap lies from cmd_bind (#787)
fd453ed3 2026-05-13T08:59:28-07:00 feat(788): python realize kit - emits Python source from concepts (#788)
fcca2835 2026-05-13T09:23:09-07:00 feat(790): c realize kit - native C, emits real free() (#790)
88c86b45 2026-05-13T09:37:41-07:00 bind evidence through compound contracts (#797)
525a6a9c 2026-05-13T14:43:15-07:00 wire: sugar-verifier emits ProofRunMemento + StageReceipt per run (#838)
4204deab 2026-05-13T14:45:51-07:00 wire: sugar bind mints PromotionDecisionMemento per admitted evidence (#839)
187d16e5 2026-05-13T15:17:33-07:00 Route contract sugar through realize kits (#840)
```

The regression is in `91c408d6`:

* `cmd_bind.rs` stopped collecting Rust source files directly and now calls `kit_dispatch::dispatch_bind_lift(&root, &source_lang)` at `implementations/rust/sugar-cli/src/cmd_bind.rs:222-231`.
* If dispatch fails or returns zero entries, `cmd_bind.rs` emits `kit-plugin-unavailable` or `bind-lift-empty` into `gaps.json` and exits success at `implementations/rust/sugar-cli/src/cmd_bind.rs:247-278`.
* `kit_dispatch.rs` resolves lift kits under the passed workspace root, env, built-in paths under `<workspace_root>/implementations/<lang>/`, sibling binaries next to the running `sugar`, then PATH at `implementations/rust/sugar-cli/src/kit_dispatch.rs:156-227` and `238-274`.
* The checked-in Rust lift manifest is at repo root `.sugar/lift/rust-bind/manifest.toml:1-7`, with command `implementations/rust/target/debug/sugar-walk-rpc`.
* The trinity harness copies only `tests/fixtures/trinity_roundtrip` into a temp dir. That fixture contains only `src/lib.rs`; it does not contain `.sugar/lift/rust-bind/manifest.toml`.
* In a clean cargo target dir, sibling binary discovery also fails because cargo built `debug/sugar` for the integration test but not `debug/sugar-walk-rpc`.

The later commits changed body-template and realize plumbing but did not introduce the first red state. They matter for follow-up work:

* `fd453ed3` added the Python realize kit.
* `fcca2835` added the C realize kit.
* `187d16e5` routed contract sugar through realize kits.
* No Rust body-template file or Rust realize kit exists at HEAD.

## 5. Meta-question verdict

Verdict: MIXED.

Primary: REGRESSION. The test was green at `d9ea20c9` with a fresh target dir and first red at `91c408d6`. The regression is the PR #779 kit-dispatch cut: it moved Rust Verb 1 lift out of `cmd_bind` into `kit_dispatch`, but the trinity temp fixture root does not carry the root `.sugar/lift/rust-bind/manifest.toml`, and a clean `cargo test -p sugar-cli --test trinity_roundtrip_test` does not build the fallback sibling binary `sugar-walk-rpc`.

Secondary: OVERCLAIM. PR #779's commit text says "trinity_roundtrip_test: passes" and "After: leg 1 succeeds (Rust lift kit + Java realize kit, both first-class)." A clean-target run of the merge commit contradicts that. Separately, the current test is not a byte-identical trinity receipt in v0: its own Branch 2 explicitly accepts honest gap kinds when composed loss is non-empty.

Not a pure NOT-WIRED-INTO-CI finding. The workflow calls `make test-all` at `.github/workflows/ci.yml:273-278`; `test-all` depends on `test-rust` at `Makefile:701-704`; and `test-rust` runs `cargo test --release --manifest-path implementations/rust/Cargo.toml` at `Makefile:588-592`, which should include this integration test. The surprise is state masking: the CI job runs `make conformance` first at `.github/workflows/ci.yml:266-271`, and several conformance paths call `build-rust`, whose workspace build can create `target/release/sugar-walk-rpc` before `test-rust` runs. The isolated clean-target command still fails.

Under the no-`gh` constraint, I found no local trinity evidence tying task IDs `#152`, `#153`, or `#155` to this receipt. Local grep only finds unrelated or older references for `#129`, `#137`, and `#155`; the actionable overclaim evidence here is the merged PR #779 commit text plus the current test code.

## 6. The five gaps revisited

1. Existing trinity is RED on main: confirmed under a clean target dir. The failing command in section 1 exits 101 and panics at `sugar-cli/tests/trinity_roundtrip_test.rs:189:5` with `Leg 1 must produce translated/java/ dir`.

2. Existing trinity is v0 lossy, not byte-identical: confirmed. The test says Branch 1 does byte-identical equality only when `composed_loss.is_empty()` at `implementations/rust/sugar-cli/tests/trinity_roundtrip_test.rs:341-372`. The v0 expected path is Branch 2, which asserts non-empty honest gap kinds at `implementations/rust/sugar-cli/tests/trinity_roundtrip_test.rs:373-455`. The header comments also state the v0 expected outcome is loudly-bounded-lossy at `implementations/rust/sugar-cli/tests/trinity_roundtrip_test.rs:17-29`.

3. No Rust body-template file, no Rust realize kit: confirmed. Body-template files at HEAD:

```text
menagerie/c-language-signature/specs/body-templates/c-canonical-bodies.json
menagerie/java-language-signature/specs/body-templates/java-canonical-bodies.json
menagerie/python-language-signature/specs/body-templates/python-canonical-bodies.json
```

No `rust-canonical-bodies.json`, no `menagerie/rust-language-signature/specs/body-templates/*`, and no `sugar-realize-rust` implementation exists. Realize kit directories are:

```text
implementations/c/sugar-realize-c-core
implementations/java/sugar-realize-java-core
implementations/python/sugar-realize-python-core
```

The dispatcher would look for a Rust realize binary under `implementations/rust/target/{release,debug}/sugar-realize-rust` or a `.sugar/realize/rust/manifest.toml` style plugin, per `implementations/rust/sugar-cli/src/kit_dispatch.rs:620-687`.

4. C realizer emits only body text, not parseable C function source: confirmed. `implementations/c/sugar-realize-c-core/src/main.c:935-940` selects a body template or stub body, then calls `indent_body(body)`. The JSON-RPC result emits that indented body as `result.source` at `implementations/c/sugar-realize-c-core/src/main.c:946-949`. It parses `function` at line 925, but there is no function signature assembly around the emitted source. The output is a body fragment such as an indented template or stub, not a complete parseable C function.

5. Plugin discovery wiring incomplete for temp fixture roots: confirmed. The harness fixture root is `tests/fixtures/trinity_roundtrip` at `implementations/rust/sugar-cli/tests/trinity_roundtrip_test.rs:40-45`. The recursive copy helper copies only that fixture tree at `implementations/rust/sugar-cli/tests/trinity_roundtrip_test.rs:47-61`, and the test copies it to a temp dir at lines 175-177. The fixture tree contains only `src/lib.rs`. It does not contain `.sugar/lift/rust-bind/manifest.toml`. Then leg 1 calls bind with `--root <fixture_tmp>` at `implementations/rust/sugar-cli/tests/trinity_roundtrip_test.rs:180-181`, so `cmd_bind` passes the temp fixture root into `dispatch_bind_lift` at `implementations/rust/sugar-cli/src/cmd_bind.rs:222-231`. The dispatcher searches `<workspace_root>/.sugar/lift/...` first at `implementations/rust/sugar-cli/src/kit_dispatch.rs:156-186`, but in this test `workspace_root` is the temp fixture, not the repo root.

## 7. Recommended PR dependency order

PR 2: plugin discovery and hermetic test fixture root. First blocker. The trinity harness must not depend on ambient `target/{debug,release}/sugar-walk-rpc` state. The receipt should fail or pass the same way in a fresh `CARGO_TARGET_DIR`, after `cargo test -p sugar-cli --test trinity_roundtrip_test`, and inside `make test-rust`.

PR 3: Rust body-template file and Rust realize kit, if the intended trinity closure includes leg 3 Python to Rust with real Rust output. Today there is no Rust realize kit and no Rust body-template catalog, so a byte-identical Rust return leg is not expressible yet.

PR 4: C realizer parseability. The C realizer currently returns a body fragment, not a full function source. That should be closed before treating C realization cells as parseable source receipts.

PR 5: re-run the baseline gate. The gate should include a fresh-target reproduction, not only the normal repo target dir. A useful acceptance command is the section 1 "run this first" command, because it prevents ambient target artifacts from hiding the lift-kit discovery gap.

If Java and Python lift kits are in scope for exact multi-hop closure, they sit between PR 2 and PR 5 as their own work. The current test accepts Java lift absence as a v0 loss record, so they are not the first blocker for making the existing test honestly green again.

## 8. Surprises

The biggest surprise is target-dir state dependence. In a clean target dir the test is red. After `cargo build -p sugar-walk --bin sugar-walk-rpc`, the same test passes because the dispatcher finds the sibling binary next to `sugar`. That means a CI job that builds the workspace before running tests can mask the isolated integration test failure.

The second surprise is blame noise. Current line blame points at `525a6a9c` for the failing assertion, but the assertion's behavior was introduced by `42c3b43f`; `525a6a9c` only reformatted it.

The third surprise is that PR #779 says the Rust bind-lift manifest makes the kit discoverable out of the box, but the actual trinity harness deliberately runs against a copied temp fixture root. Repo-root manifests do not participate unless the harness copies them or the dispatcher has a repo-root fallback.
