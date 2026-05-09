# C Lifter Family — Design Proposal

**Status:** draft for review
**Author:** T Savo
**Date:** 2026-05-09

## TL;DR

ProvekIt has three C lifters today (`kernel-doc`, `sparse`, `assertions`), all annotation-driven. Most kernel C has no annotations, so they emit zero `declarations[]` on most files. The Rust walker (`provekit-walk`) and Java JUnit extractor demonstrate two synthesis patterns we should port to C: **defensive-pattern derivation** (`if cond panic` → `¬cond`) and **test-body derivation** (assertion in test → contract on function under test).

This doc proposes **two new lifters + one extension** that together unblock the gold pipeline (lift → compose → prove → K ∩ M'') against unannotated kernel C.

## Verified facts

### Existing C lifters (`implementations/c/`)

- `provekit-lift-c-kernel-doc`: lifts `/** kernel-doc */` comments. Empirical: rxkad.c has 1, kernel mostly has none on internal functions.
- `provekit-lift-c-sparse`: lifts sparse annotations (`__must_hold`, `__acquires`, `__user`, `__rcu`). Empirical: rxkad.c has 0, sibling files have 0-10 each.
- `provekit-lift-c-assertions`: lifts `assert()` / `BUG_ON` / `WARN_ON` (per name). Not yet verified empirically against kernel C.

All three populate `pk_c_lift_result.declarations`. Schema:
```c
typedef struct pk_c_lift_result {
    pk_c_json_array declarations;
    pk_c_json_array call_edges;
    pk_c_json_array diagnostics;
    pk_c_json_array opacity_report;
    pk_c_json_array refusals;
} pk_c_lift_result;
```

### `provekit-walk` (Rust) — the model

`implementations/rust/provekit-walk/src/lift.rs` does real predicate derivation from Rust source via `syn`:

| Pattern | Lifts to |
|---|---|
| `if cond { panic!() }` (no else) | `¬cond` (contraposition becomes precondition) |
| `assert!(cond)` | `cond` (precondition + postcondition if not later shadowed) |
| `assert!(c1 && c2)` | `c1 ∧ c2` |
| Trailing expression `expr` (no `;`) | `result = <lifted expr>` postcondition |
| `return expr;` | `result = <lifted expr>` postcondition |
| `!cond` in if-then-panic | De Morgan + comparison flip applied |
| `assert!(x >= 5); let x = 0;` | Drop the assert (rebind makes it unsound) |
| `is_some()`, `is_none()`, `is_empty()`, `is_err()`, `is_ok()` | Atomic predicate with method name |
| Closures `|x| body` | Lambda with fresh `x#N` binder id (Barendregt form) |

Pre and post end up as real `IrFormula` (the canonical ProofIR formula type) and feed `libprovekit::compose::FunctionContractMemento`.

The walk module (`walk.rs`) does Dijkstra-style backward WP propagation: from each callsite, walk backward through statements applying `wp(let x = e, P) = P[e/x]`. Final WP at function entry is the proof obligation.

### JUnit extractor (Java) — the test-body model

`JUnitExtractor.java` walks `@Test` method bodies. Each `assertEquals(expected, actual)` becomes a postcondition `eq(actual, expected)`. Each `assertTrue(expr)` becomes `expr`. `if/else` branches yield scoped contracts. The contract is named `testMethod::assertion_index` and represents a real semantic claim about whatever the test calls.

This generalizes: **test code is de facto specification.** Every assertion is an executable contract.

### `libprovekit::compose` — the consumer

The composition primitive consumes `FunctionContractMemento { name, formals, formal_sorts, return_sort, pre, post, effects, locus, body_cid? }`. The `pre` and `post` are `IrFormula` (canonical ProofIR). The `effects` is an `EffectSet`. `compose_chain_contracts` walks call chains and produces `ComposedFunctionContract` mementos with byte-deterministic CIDs via JCS.

Whatever any C lifter emits as a contract MUST conform to this struct shape.

## The proposal

Three deliverables (one new Rust crate, one new C lifter, one extension to existing):

### Deliverable 1: `provekit-lift-c-walker` (new C lifter)

New C lifter at `implementations/c/provekit-lift-c-walker/`, mirroring the sibling C lifter pattern (kernel-doc, sparse, assertions). Emits synthesized contracts as `declarations[]` entries via the same JSON-RPC `lift` method.

- `src/main.c` — JSON-RPC scaffold (copy from kernel-doc lifter; identical protocol handling)
- `src/walker.c` — synthesis pass: walk `clang_ast.c`'s emitted AST + `effects.c`'s per-function effects, recognize defensive patterns, build contract JSON per function
- `src/patterns.c` — C-side pattern recognition (port of `provekit-walk::lift::lift_function_precondition` / `lift_function_postcondition` algorithms to libclang cursor types)
- `src/formula.c` — builds the JSON shape that deserializes to `provekit_ir_types::IrFormula` on the Rust consumer side
- `Makefile` — same shape as `provekit-lift-c-kernel-doc` Makefile (libclang-enabled with effects.c, fallback to stub)
- `tests/integration.sh` — smoke tests on fixture C files (BUG_ON, if-return-error, assert patterns)

**JSON-RPC surface:** `c-walker`. Reads source files, emits `pk_c_lift_result`-shaped JSON with synthesized contracts in `declarations[]`. Plug-compatible with existing `provekit lift` / `provekit compose --rpc` consumers.

**Why C instead of Rust:** keeps the lifter in `implementations/c/` family (consistent build, test, RPC pattern). `clang_ast.c` already does the libclang AST traversal — synthesis adds a new pass over the existing visitor, no clang-sys binding overhead. `effects.c` already linked in (per #519). The pattern recognition algorithms transfer from `provekit-walk::lift` regardless of language; we don't get to reuse `lift.rs` source either way (it's tightly bound to `syn::Expr`/`syn::Stmt` types). The C path is ~800-1200 lines of new code; Rust would be ~1500+ (clang-sys bindings + visitor + algorithm port).

**Formula representation:** the contracts emitted in `declarations[]` use the JSON shape that the canonical `provekit_ir_types::IrFormula` deserializes from. The Rust consumer side (provekit compose, prove) sees them as ordinary IrFormula values and feeds them into `libprovekit::compose::FunctionContractMemento`. No new schema, no FFI from C side.

### Deliverable 2: `provekit-lift-c-kunit` (new C lifter)

Mirror of `JUnitExtractor`. Walks KUnit test bodies (or assertion macros in any C test code) and mints contracts per assertion.

KUnit pattern set:
| Macro | Lifts to |
|---|---|
| `KUNIT_EXPECT_EQ(test, a, b)` | `a = b` |
| `KUNIT_EXPECT_NE(test, a, b)` | `a ≠ b` |
| `KUNIT_EXPECT_TRUE(test, expr)` | `expr` |
| `KUNIT_EXPECT_FALSE(test, expr)` | `¬expr` |
| `KUNIT_EXPECT_NULL(test, ptr)` | `ptr = NULL` |
| `KUNIT_EXPECT_NOT_NULL(test, ptr)` | `ptr ≠ NULL` |
| `KUNIT_EXPECT_LT/LE/GT/GE(test, a, b)` | `a < b`, etc. |
| `KUNIT_ASSERT_*` (same as EXPECT but fatal) | Same formula, marks contract as `assert` strength |
| Surrounding `if/else` | Scopes the contract to the branch |

Contract names: `testFunction::assertion_index`. The contract attaches to the function under test (extracted from the assertion's left-hand side or test naming convention).

**Plug shape:** new C lifter binary, same Makefile pattern as `provekit-lift-c-kernel-doc`. Surface name: `c-kunit`. Consumes through `provekit lift / compose / prove` unchanged.

### Deliverable 3: Extension to `provekit-lift-c-assertions`

Verify what it currently does. If it doesn't already lift `BUG_ON` / `WARN_ON` / `if (!ptr) return -ENOMEM` patterns, extend it to do so. These are production-side runtime invariants, complementing test-side KUnit invariants.

Pattern set for assertions lifter (production code):
| Pattern | Lifts to |
|---|---|
| `BUG_ON(cond);` | `¬cond` precondition (BUG_ON aborts; ¬cond must hold for normal continuation) |
| `WARN_ON(cond);` | Weaker — log it as an opacity entry, don't lift as hard contract |
| `if (cond) BUG();` | `¬cond` precondition |
| `if (!ptr) return -ENOMEM;` | `ptr ≠ NULL` precondition (post-return continuation requires it) |
| `if (ret < 0) return ret;` | `ret ≥ 0` for non-error continuation |
| `if (cond) goto error;` | `¬cond` for the fall-through (non-error label leads to early return) |
| `assert(cond);` | `cond` precondition |

## Tier breakdown across deliverables

| Tier | What it adds | Coverage | Engineering scope | Lifter |
|---|---|---|---|---|
| (a.1) Trivial | `{pre=true, post=true, effects=<from libclang>}` per function | All functions in lifted files | Smallest. Just package effects.c output as contract. | `provekit-lift-c-walker` skeleton |
| (a.2) Pattern-derived | Real preconditions from BUG_ON, if-return-error, assert, goto-error | All functions with defensive patterns (most kernel functions have at least one) | Medium. Port lift.rs algorithms C-side. | `provekit-lift-c-walker` + extended assertions lifter |
| (a.3) Test-derived | Real semantic claims from KUnit assertion macros | All functions covered by KUnit tests | Medium. Mirror JUnitExtractor. | `provekit-lift-c-kunit` |
| (a.4) Type-derived | Constraints from `__user`, `__rcu`, `__must_hold(lock)`, `size_t`, `loff_t`, `gfp_t`, `__bitwise`, etc. | All functions touching kernel-typed parameters (essentially every kernel function) | Medium-hard. Per-attribute predicate library. | `provekit-lift-c-walker` extension (folds into PR 2) |

## PR sequence with explicit gates

### PR 1: `provekit-lift-c-walker` skeleton + (a.1) trivial synthesis

**Deliverable:**
- New C lifter `implementations/c/provekit-lift-c-walker/` with src/main.c (RPC scaffold copied from kernel-doc lifter), src/walker.c (synthesis driver), Makefile (libclang + effects.c, mirror kernel-doc Makefile)
- Skeleton: walk libclang AST via existing clang_ast.c infrastructure, enumerate functions, emit one declaration per function with `pre=true, post=true, effects=<from effects.c>`
- CLI binary with `--rpc` mode, surface `c-walker`

**Smoke gate:**
1. Lift `net/rxrpc/rxkad.c` via `c-walker` surface → expect ~30-50 declarations (one per function)
2. `provekit compose --rpc` consumes the declarations → expect composed contracts emitted
3. `provekit prove --formula <one_composed>` returns a verdict (will be trivial discharge since pre=post=true, but pipeline is end-to-end)

**Merge condition:** all three smoke steps green. Trivial discharge is OK at this tier.

### PR 2: (a.2) defensive-pattern derivation + (a.4) type-derived predicates in `provekit-lift-c-walker`

**Deliverable:**
- Port `provekit-walk::lift` algorithms to `src/patterns.c`:
  - `BUG_ON(cond)` → `¬cond` precondition
  - `if (!ptr) return -ENOMEM` (or `-E*`) → `ptr ≠ NULL` precondition
  - `if (ret < 0) return ret` → `ret ≥ 0` for non-error continuation
  - `if (cond) goto error` → `¬cond` for fall-through
  - `assert(cond)` → `cond` precondition
  - `return expr` → `result = <expr>` postcondition
  - De Morgan + double-neg elim + comparison flips (mirror provekit-walk::lift::negate)
- Add (a.4) type-derived predicates to `src/types.c`:
  - `__user T *` parameter → `is_user_ptr(T)` precondition
  - `__rcu T *` parameter → `is_rcu_protected(T)` precondition (must be in rcu_read_lock())
  - `__must_hold(lock)` attribute → `lock_held(lock)` precondition
  - `__acquires(lock)` / `__releases(lock)` attributes → effect-set entries (lock state change)
  - `size_t n` parameter → `n ≥ 0` (vacuous for solver but explicit)
  - `gfp_t gfp` → `valid_gfp_flags(gfp)` (capture allocation context)

**Smoke gate:**
1. Lift a known kernel function with `if (!ptr) return -ENOMEM;` pattern → verify contract has `ptr ≠ NULL` in pre
2. Lift a function with `__user *buf` parameter → verify `is_user_ptr(buf)` in pre
3. Lift a function with `__must_hold(my_lock)` → verify `lock_held(my_lock)` in pre
4. Hand-craft a violating caller for one of the above and verify `provekit prove` REFUTES the composed implication with a counterexample

**Merge condition:** at least one real refutation observed end-to-end across (a.2) and (a.4) combined.

### PR 3: `provekit-lift-c-kunit` mirror of JUnitExtractor

**Deliverable:**
- New C lifter at `implementations/c/provekit-lift-c-kunit/`
- Walks KUnit test files, mints contract per `KUNIT_*` macro

**Smoke gate:**
1. Lift `lib/kunit_test.c` (or whichever KUnit self-test file) via `c-kunit` surface
2. Expect declarations[] populated with per-assertion contracts
3. Verify contract names follow `testFunction::N` pattern
4. Compose with PR 1's c-walker output for the production functions; verify the kunit contracts cite production functions

**Merge condition:** at least one composed contract that joins a KUnit assertion with a production function's body, prove portfolio dispatched on it.

### PR 4: Extend `provekit-lift-c-assertions`

**Deliverable:**
- Verify existing behavior on a kernel file with BUG_ON
- Extend to recognize all production-side patterns from the table above
- No new binary; modify existing assertions lifter in place

**Smoke gate:**
1. Lift `mm/slab.c` (or any kernel file with multiple `BUG_ON` / `WARN_ON` calls) via `c-assertions` surface
2. Expect declarations[] populated with one contract per BUG_ON / WARN_ON / if-return-error
3. Composed with PR 1's output, verify contracts merge cleanly

**Merge condition:** declarations[] non-empty on a real kernel file with assertions.

### PR 5 (folded into PR 2): formerly (a.4)

(a.4) type-derived predicates are now part of PR 2 per the user's "in scope" call. The MVP must include kernel-typed parameter constraints (`__user`, `__rcu`, `__must_hold`, `__acquires`, `__releases`, `size_t`, `gfp_t`) — most kernel functions touch at least one of these and the type-derived constraints are often the only contract signal available for a given function.

## Open questions

1. **Where does `provekit-walk-c` live?** Sibling crate `implementations/rust/provekit-walk-c/`, or extension module inside `provekit-walk` (`provekit-walk::c`)? Sibling probably cleaner — different parser, different binary, different test fixtures.
2. **Do we need clang-sys or the higher-level `clang` crate?** Higher-level is friendlier; clang-sys is closer to the libclang C API. Likely start with `clang` crate (Rust idiomatic), drop to clang-sys if we hit limits.
3. **How does the kunit lifter resolve which production function a `KUNIT_EXPECT_EQ(test, foo(5), 10)` constrains?** The Java pattern names contracts after the test method. C analogue: same. The contract's name is `testFunction::N`; consumers (compose, prove) treat the test method itself as the function whose contract this is. The fact that the assertion CALLS foo is captured via call_edges, and composition then propagates the test's postcondition through the call chain to foo.
4. **Sparse + assertions Makefile fix (#519)** already landed. PR 1 should not re-touch those Makefiles.

## Verification once all four PRs land

End-to-end smoke on the full kernel:
1. Lift entire kernel with c-walker (a.1+a.2) → atomic contract per function with real preconditions where defensive patterns exist
2. Lift KUnit tests with c-kunit (a.3) → test-body contracts on tested functions
3. Lift production assertions with c-assertions (a.4 production patterns) → BUG_ON / WARN_ON contracts
4. Optionally lift kernel-doc / sparse where they exist
5. Compose via canonical libprovekit compose → composed contracts for every chain
6. Fire all 6 PPP predicates against composed contracts (not just call_edges)
7. Dispatch prove portfolio per composed contract, partition (discharged / refuted / timeout)
8. K ∩ M'' = predicate hits whose composed contract is refuted = high-confidence bugs

## Out of scope (this design)

- Implementing `provekit-walk-c` (this is the design; implementation comes after Sir's review)
- Type-derived contracts (a.4) — deferred to PR 5
- Cross-language federation (C contracts composed with Rust contracts via canonical CCP)
- ProofIR formula language extensions for kernel-specific predicates (e.g. lock state, RCU read-side critical section)
- Performance / scaling (libclang on full kernel takes hours; that's a phase-tuning concern, not architectural)

T Savo
